//! Dictzip (`.dict.dz`) files: the gzip header with its `RA` chunk table, read to find where each
//! independently deflated chunk starts, and the plan for reading a range of the uncompressed text:
//! which chunks, how much of each to skip and how much to take. Inflating the chunks is the
//! caller's.

/// Why a dictzip file or range cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fail {
    /// A read or seek failed, the file ended, or the range is outside the text.
    Read,
    /// The file is not dictzip, or its table does not add up.
    Corrupt,
    /// The chunk table could not be allocated.
    Memory,
}

/// Chunk tables hold at most this many chunks (32 KB of offsets).
pub const MAX_CHUNKS: u16 = 8192;

/// A file read in order, with seeks.
pub trait File {
    /// Fills `buf`; false when the file ended or the read failed first.
    fn read_exact(&mut self, buf: &mut [u8]) -> bool;
    fn seek(&mut self, pos: u32) -> bool;
    fn position(&self) -> u32;
    fn size(&self) -> u32;
}

/// What the header says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// Where the first chunk starts.
    pub data_offset: u32,
    /// The text's size (the gzip trailer's ISIZE).
    pub total_size: u32,
    /// Uncompressed bytes in each chunk but the last.
    pub chunk_length: u16,
}

fn le16(file: &mut impl File) -> Result<u16, Fail> {
    let mut raw = [0u8; 2];
    if file.read_exact(&mut raw) {
        Ok(u16::from_le_bytes(raw))
    } else {
        Err(Fail::Read)
    }
}

/// Skips a NUL-terminated field.
fn skip_string(file: &mut impl File) -> Result<(), Fail> {
    let mut b = [1u8];
    while b[0] != 0 {
        if !file.read_exact(&mut b) {
            return Err(Fail::Read);
        }
    }
    Ok(())
}

/// Reads the header of a dictzip file (read from its start), asking `table` for room for the
/// chunk table (`chunk_count + 1` offsets) and filling it with each chunk's compressed offset from
/// the first, the last being the end of the data.
///
/// # Errors
/// [`Fail::Read`] when the file comes up short, [`Fail::Corrupt`] when it is not dictzip (no
/// `FEXTRA`, no or two `RA` fields, version other than 1, counts out of range, lengths that do not
/// add up, an empty text) and [`Fail::Memory`] when `table` has no room.
pub fn parse<'t>(
    file: &mut impl File,
    table: impl FnOnce(usize) -> Option<&'t mut [u32]>,
) -> Result<Header, Fail> {
    let mut header = [0u8; 10];
    if !file.read_exact(&mut header) {
        return Err(Fail::Read);
    }
    let [0x1f, 0x8b, 8, flags, ..] = header else {
        return Err(Fail::Corrupt);
    };
    // Dictzip keeps its table in FEXTRA.
    if flags & 0x04 == 0 {
        return Err(Fail::Corrupt);
    }
    let extra_len = u32::from(le16(file)?);
    let mut out = Header::default();
    let mut table = Some(table);
    let mut found = false;
    let mut read = 0u32;
    while read + 4 <= extra_len {
        let mut sub = [0u8; 4];
        if !file.read_exact(&mut sub) {
            return Err(Fail::Read);
        }
        read += 4;
        let [id0, id1, l0, l1] = sub;
        let sub_len = u16::from_le_bytes([l0, l1]);
        if read + u32::from(sub_len) > extra_len {
            return Err(Fail::Corrupt);
        }
        if [id0, id1] != *b"RA" {
            if !file.seek(file.position().wrapping_add(u32::from(sub_len))) {
                return Err(Fail::Read);
            }
            read += u32::from(sub_len);
            continue;
        }
        // A second table is refused rather than skipped.
        let Some(table) = table.take() else {
            return Err(Fail::Corrupt);
        };
        if sub_len < 6 {
            return Err(Fail::Corrupt);
        }
        let (version, chunk_length, chunk_count) = (le16(file)?, le16(file)?, le16(file)?);
        read += 6;
        if version != 1 || chunk_length == 0 || chunk_count == 0 || chunk_count > MAX_CHUNKS {
            return Err(Fail::Corrupt);
        }
        if u32::from(sub_len) != 6 + u32::from(chunk_count) * 2 {
            return Err(Fail::Corrupt);
        }
        out.chunk_length = chunk_length;
        let offsets = table(usize::from(chunk_count) + 1).ok_or(Fail::Memory)?;
        let mut cumulative = 0u32;
        if let Some(first) = offsets.first_mut() {
            *first = 0;
        }
        for slot in offsets.iter_mut().skip(1).take(usize::from(chunk_count)) {
            cumulative = cumulative.wrapping_add(u32::from(le16(file)?));
            read += 2;
            *slot = cumulative;
        }
        found = true;
    }
    if read != extra_len || !found {
        return Err(Fail::Corrupt);
    }
    if flags & 0x08 != 0 {
        skip_string(file)?;
    }
    if flags & 0x10 != 0 {
        skip_string(file)?;
    }
    if flags & 0x02 != 0 {
        let mut crc = [0u8; 2];
        if !file.read_exact(&mut crc) {
            return Err(Fail::Read);
        }
    }
    out.data_offset = file.position();
    let size = file.size();
    if size < 4 {
        return Err(Fail::Corrupt);
    }
    if !file.seek(size - 4) {
        return Err(Fail::Read);
    }
    let mut isize = [0u8; 4];
    if !file.read_exact(&mut isize) {
        return Err(Fail::Read);
    }
    out.total_size = u32::from_le_bytes(isize);
    if out.total_size == 0 {
        return Err(Fail::Corrupt);
    }
    Ok(out)
}

/// Part of a chunk to read: its compressed bytes at `offset` (from the file's start), `size` long,
/// inflated, skipping the first `skip` bytes and taking the next `take`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slice {
    pub offset: u32,
    pub size: u32,
    pub skip: u32,
    pub take: u32,
}

/// Hands `slice` the chunk slices that make up `size` bytes of the text from `offset`, in order.
///
/// # Errors
/// [`Fail::Read`] for a range outside the text or past the chunk table, [`Fail::Corrupt`] when the
/// chunks do not cover the range, or `slice`'s own failure.
pub fn slices(
    header: &Header,
    table: &[u32],
    offset: u32,
    size: u32,
    mut slice: impl FnMut(Slice) -> Result<(), Fail>,
) -> Result<(), Fail> {
    if size == 0 {
        return Ok(());
    }
    let (total, length) = (header.total_size, u32::from(header.chunk_length));
    if length == 0 || offset > total || size > total - offset {
        return Err(Fail::Read);
    }
    let start = offset / length;
    let end = (offset + size - 1) / length;
    let entries = u32::try_from(table.len()).unwrap_or(u32::MAX);
    if end.saturating_add(1) >= entries {
        return Err(Fail::Read);
    }
    let last = entries - 2;
    let mut remaining = size;
    for chunk in start..=end {
        let mut out_size = length;
        if chunk == last {
            out_size = total.wrapping_sub(chunk.wrapping_mul(length));
        }
        if out_size == 0 || out_size > length {
            out_size = length;
        }
        let skip = if chunk == start { offset % length } else { 0 };
        // A last chunk shorter than the skip wraps, as the old unsigned subtraction did.
        let take = remaining.min(out_size.wrapping_sub(skip));
        let at = |i: u32| table.get(i as usize).copied().unwrap_or(0);
        slice(Slice {
            offset: header.data_offset.wrapping_add(at(chunk)),
            size: at(chunk + 1).wrapping_sub(at(chunk)),
            skip,
            take,
        })?;
        remaining -= take;
        if remaining == 0 {
            break;
        }
    }
    if remaining == 0 {
        Ok(())
    } else {
        Err(Fail::Corrupt)
    }
}
