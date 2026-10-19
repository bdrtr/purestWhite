//! The JPEG decoder for chapter images and covers: our own, decoding as JPEGDEC (the library it
//! replaced, at the commit platformio.ini pinned, with CrossPoint's patches) did in the firmware,
//! which `test/jpeg_decoder` pins. It hands out 8-bit gray blocks of MCUs, at full size or scaled
//! by 1/2, 1/4 or 1/8, for the converters to scale and dither.
//!
//! Like JPEGDEC it decodes baseline Huffman JPEGs, and of a progressive one only the first scan (its
//! DC values, at 1/8 size); reads the file through a 2048-byte buffer from which it strips stuffed
//! zeros and markers; keeps a 32-bit bit buffer, which loses the last bits of a long code with many
//! extra bits, as JPEGDEC built for the firmware did; and draws color images from their luma
//! alone (and four-component ones as if every block of the scan were the first component's: the
//! picture comes out scrambled, as it did with JPEGDEC). Like JPEGDEC, `open` can also loop
//! without end on a crafted file whose segment lengths keep jumping past its end (its file
//! position runs round an `i32`); that is to be fixed in its own commit.
//!
//! Its internals follow JPEGDEC function by function, to be read against it, and its state keeps
//! JPEGDEC's layout where JPEGDEC read or wrote past an array: the read buffer runs on into the DC
//! and AC Huffman tables, and the MCU blocks are i16 coefficients that the IDCT replaces with 8-bit
//! pixels in place. Changing that layout can change the output for broken files, so such a change
//! goes in its own commit. It departs from JPEGDEC where JPEGDEC's behaviour was undefined:
//! - Reads and writes past the end of that layout read 0 and are dropped (JPEGDEC went on into the
//!   rest of its state, or past it).
//! - A frame of more than four components keeps the first four; JPEGDEC wrote the rest past its
//!   component table, and scaled quantization tables past its own into the read buffer.
//! - A sampling factor it has no MCU layout for fails the decode with
//!   [`Error::Unsupported`]; JPEGDEC divided by zero.
//!
//! and where no caller sees it:
//! - EXIF segments are not read: JPEGDEC took only the orientation and the thumbnail from them,
//!   which no caller uses.
//! - An image decodes once, after it opened ([`Error::NotOpen`] otherwise); JPEGDEC decoded again
//!   with its quantization tables scaled twice.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
// The byte and bit accessors run for every coefficient; at opt-level "s" LLVM left them as calls,
// several times the decode's instructions on the device (counted in the emulator).
#![allow(clippy::inline_always, reason = "measured: see above")]

mod decode;
mod tables;

use core::mem::MaybeUninit;

/// The size [`Jpeg::decode`] decodes at. A progressive image asked for [`Scale::Full`] decodes at
/// 1/8 (all its first scan holds). Asked for 1/2 or 1/4 it fills that size from the same DC
/// values, as JPEGDEC did: blocky for gray and 4:4:4, and for subsampled color only one pixel per
/// luma block, the rest left as the pixel buffer held it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Full,
    Half,
    Quarter,
    Eighth,
}

/// JPEGDEC's scale option bits, which the decode tests one by one in its order (a progressive
/// image adds `EIGHTH` to what was asked).
mod scale {
    pub const HALF: u8 = 2;
    pub const QUARTER: u8 = 4;
    pub const EIGHTH: u8 = 8;
}

/// Why `open` or `decode` failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `decode` without a successful `open`, or a second time.
    NotOpen,
    /// A broken header (bad tables, a segment past the read buffer, no scan) or bad data in the
    /// scan.
    Decode,
    /// Extended (SOF1) or lossless (SOF3) coding, Huffman codes its lookup tables cannot hold (or
    /// more than two AC tables), or a sampling factor it has no MCU layout for. (An
    /// arithmetic-coded file opens at 0x0, which the converters refuse.)
    Unsupported,
    /// Not a JPEG file, or under 256 bytes.
    InvalidFile,
}

/// The file being decoded.
pub trait Source {
    /// Reads up to `buf.len()` bytes at `pos` and returns how many it read: fewer at the end of
    /// the file, 0 past it or on an error.
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> usize;
}

/// A block of decoded MCUs, handed out left to right, top to bottom.
pub struct Block<'a> {
    pub x: i32,
    pub y: i32,
    /// Pixels a row, and the row stride in `pixels`.
    pub width: i32,
    pub height: i32,
    /// The columns of each row that are inside the image.
    pub width_used: i32,
    /// `height` rows of `width` 8-bit gray pixels.
    pub pixels: &'a [u8],
}

const FILE_BUF_SIZE: usize = 2048;
const FILE_BUF_I32: i32 = 2048;
const FILE_HIGHWATER: i32 = 1536;
const HUFF_TABLEN: usize = 273;
const DC_TABLE_SIZE: usize = 1024;
/// AC table entries (u16) per table.
const HUFF11SIZE: usize = 2048;
const MAX_BUFFERED_PIXELS: i32 = 2048;
const DCTSIZE: usize = 64;
const MCU_SKIP: i32 = -8;
/// The most MCUs in a block.
const MAX_MCUS: i32 = 1000;

/// Where JPEGDEC's read buffer, DC tables (2 KB, for two of the four it fills) and AC tables
/// (two of 2048 u16) lay one after another.
const DC_BASE: usize = FILE_BUF_SIZE;
const AC_BASE: usize = DC_BASE + 2 * DC_TABLE_SIZE;
const ARENA_SIZE: usize = AC_BASE + 2 * 2 * HUFF11SIZE;
/// The output pixel buffer: 2048 u16 plus JPEGDEC's 8 for alignment.
const PIXELS_SIZE: usize = 2 * (MAX_BUFFERED_PIXELS as usize + 8);
/// The MCU blocks: six of 64 i16, after 8 that `MCU_SKIP` (-8) indexes; nothing reads or writes
/// them.
const MCU_BASE: usize = 16;
const MCUS_SIZE: usize = MCU_BASE + 2 * DCTSIZE * 6;

#[derive(Clone, Copy)]
struct Component {
    needed: bool,
    id: u8,
    quant: u8,
    dc: u8,
    ac: u8,
}

/// An image being decoded. About 18 KB: keep it off the stack ([`Jpeg::init`]).
pub struct Jpeg {
    arena: [u8; ARENA_SIZE],
    pixels: [u8; PIXELS_SIZE],
    mcus: [u8; MCUS_SIZE],
    quant: [u16; DCTSIZE * 4],
    components: [Component; 4],
    width: i32,
    height: i32,
    /// Where the next read goes, and the file's size.
    file_pos: i32,
    file_size: i32,
    /// Whether `open` succeeded and `decode` has not run.
    opened: bool,
    /// The `scale` bits the decode runs with.
    options: u8,
    vlc_off: i32,
    vlc_size: i32,
    res_interval: i32,
    res_count: i32,
    // The bit reader: 32 bits from `bit_buf` (an arena offset), `bit_off` of them used.
    bits: u32,
    bit_off: u32,
    bit_buf: usize,
    mcu_flags: u16,
    sub_sample: u8,
    huff_tables_used: u8,
    mode: u8,
    approx_low: u8,
    approx_high: u8,
    scan_start: u8,
    scan_end: u8,
    ff: bool,
    num_components: u8,
    ac_table: u8,
    dc_table: u8,
}

impl Jpeg {
    /// Makes `slot` a fresh decoder in place.
    pub fn init(slot: &mut MaybeUninit<Self>) -> &mut Self {
        // SAFETY: integers, byte arrays and bools, all valid as zero.
        unsafe {
            slot.as_mut_ptr().write_bytes(0, 1);
            slot.assume_init_mut()
        }
    }

    /// Reads the header of the `size`-byte file in `source`, as JPEGDEC's `open` did. The size is
    /// JPEGDEC's `int32_t`.
    ///
    /// # Errors
    /// Why the header was refused.
    pub fn open(&mut self, source: &mut impl Source, size: i32) -> Result<(), Error> {
        self.arena.fill(0);
        self.pixels.fill(0);
        self.mcus.fill(0);
        self.quant.fill(0);
        self.components = [Component {
            needed: false,
            id: 0,
            quant: 0,
            dc: 0,
            ac: 0,
        }; 4];
        self.width = 0;
        self.height = 0;
        self.file_pos = 0;
        self.file_size = size;
        self.options = 0;
        self.vlc_off = 0;
        self.vlc_size = 0;
        self.res_interval = 0;
        self.res_count = 0;
        self.bits = 0;
        self.bit_off = 0;
        self.bit_buf = 0;
        self.mcu_flags = 0;
        self.sub_sample = 0;
        self.huff_tables_used = 0;
        self.mode = 0;
        self.approx_low = 0;
        self.approx_high = 0;
        self.scan_start = 0;
        self.scan_end = 0;
        self.ff = false;
        self.num_components = 0;
        self.ac_table = 0;
        self.dc_table = 0;
        let result = self.parse_info(source);
        self.opened = result.is_ok();
        result
    }

    #[must_use]
    pub fn width(&self) -> i32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> i32 {
        self.height
    }

    /// Whether the frame is progressive (only its first scan is decoded; see [`Scale`]).
    #[must_use]
    pub fn is_progressive(&self) -> bool {
        self.mode == 0xc2
    }

    // The arena, as JPEGDEC's adjacent arrays: past its end, reads give 0 and writes are dropped.

    #[inline(always)]
    fn byte(&self, at: usize) -> u8 {
        self.arena.get(at).copied().unwrap_or(0)
    }

    #[inline(always)]
    fn set_byte(&mut self, at: usize, value: u8) {
        if let Some(slot) = self.arena.get_mut(at) {
            *slot = value;
        }
    }

    #[inline(always)]
    fn be16(&self, at: usize) -> u16 {
        match self.arena.get(at..at + 2) {
            Some(&[a, b]) => u16::from_be_bytes([a, b]),
            _ => u16::from_be_bytes([self.byte(at), self.byte(at + 1)]),
        }
    }

    #[inline(always)]
    fn be32(&self, at: usize) -> u32 {
        match self.arena.get(at..at + 4) {
            Some(&[a, b, c, d]) => u32::from_be_bytes([a, b, c, d]),
            _ => u32::from_be_bytes([
                self.byte(at),
                self.byte(at + 1),
                self.byte(at + 2),
                self.byte(at + 3),
            ]),
        }
    }

    /// AC table entry `i` (u16, little-endian in the arena).
    #[inline(always)]
    fn ac(&self, i: usize) -> u16 {
        let at = AC_BASE + 2 * i;
        match self.arena.get(at..at + 2) {
            Some(&[a, b]) => u16::from_le_bytes([a, b]),
            _ => 0,
        }
    }

    fn set_ac(&mut self, i: usize, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        self.set_byte(AC_BASE + 2 * i, lo);
        self.set_byte(AC_BASE + 2 * i + 1, hi);
    }

    #[inline(always)]
    fn pixel(&self, at: usize) -> u8 {
        self.pixels.get(at).copied().unwrap_or(0)
    }

    #[inline(always)]
    fn set_pixel(&mut self, at: usize, value: u8) {
        if let Some(slot) = self.pixels.get_mut(at) {
            *slot = value;
        }
    }

    /// Reads into `arena[at..at + len]` at the file position and moves it on.
    fn read(&mut self, source: &mut impl Source, at: usize, len: usize) -> i32 {
        let Ok(pos) = u32::try_from(self.file_pos) else {
            return 0;
        };
        let end = (at + len).min(ARENA_SIZE);
        let Some(buf) = self.arena.get_mut(at..end) else {
            return 0;
        };
        let n = source.read_at(pos, buf).min(buf.len());
        let n = i32::try_from(n).unwrap_or(0);
        self.file_pos += n;
        n
    }

    /// The converters' seek: past the end of the file it fails and the position stays.
    fn seek(&mut self, pos: i32) {
        if (0..=self.file_size).contains(&pos) {
            self.file_pos = pos;
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "JPEGDEC's JPEGParseInfo in one piece, to read against it"
    )]
    fn parse_info(&mut self, source: &mut impl Source) -> Result<(), Error> {
        let mut bytes_read = self.read(source, 0, FILE_BUF_SIZE);
        if bytes_read < 256 {
            // A JPEG file this tiny is probably bad, as JPEGDEC had it.
            return Err(Error::InvalidFile);
        }
        let mut file_pos = bytes_read;
        if self.be16(0) != 0xffd8 {
            return Err(Error::InvalidFile);
        }
        let mut offset: i32 = 2;
        let mut marker: u16 = 0;
        let mut len: u16 = 0;
        while marker != 0xffda && offset < self.file_size {
            if offset >= FILE_BUF_I32 / 2 {
                // Too close to the end: read more.
                if offset >= FILE_BUF_I32 {
                    file_pos = file_pos.wrapping_add(offset - bytes_read);
                    offset = 0;
                    self.seek(file_pos);
                    bytes_read = 0;
                }
                if offset > bytes_read {
                    return Err(Error::Decode);
                }
                if offset != 0 {
                    let (from, to) = (at(offset), at(bytes_read));
                    self.arena.copy_within(from..to, 0);
                    bytes_read -= offset;
                    offset = 0;
                }
                let n = self.read(source, at(bytes_read), FILE_BUF_SIZE - at(bytes_read));
                // JPEGDEC's int, which a file looping on a marker runs round.
                file_pos = file_pos.wrapping_add(n);
                bytes_read += n;
            }
            marker = self.be16(at(offset));
            offset += 2;
            len = self.be16(at(offset));
            if marker < 0xffc0 || marker == 0xffff {
                // Not a marker: skip a byte and try to resync.
                offset += 1;
                continue;
            }
            match marker {
                // Extended and lossless.
                0xffc1 | 0xffc3 => return Err(Error::Unsupported),
                0xffc0 | 0xffc2 => {
                    #[allow(clippy::cast_possible_truncation, reason = "the marker's low byte")]
                    let mode = marker as u8;
                    self.mode = mode;
                    let o = at(offset);
                    self.height = i32::from(self.be16(o + 3));
                    self.width = i32::from(self.be16(o + 5));
                    self.num_components = self.byte(o + 7);
                    len = len.wrapping_sub(8);
                    offset += 8;
                    for i in 0..usize::from(self.num_components) {
                        let o = at(offset);
                        let (id, sampling, quant) =
                            (self.byte(o), self.byte(o + 1), self.byte(o + 2));
                        offset += 3;
                        if i == 0 {
                            self.sub_sample = sampling;
                        }
                        if let Some(c) = self.components.get_mut(i) {
                            c.id = id;
                            c.quant = quant;
                        }
                        if quant > 3 {
                            return Err(Error::Decode);
                        }
                        len = len.wrapping_sub(3);
                    }
                    if self.num_components == 1 {
                        // Tells grayscale from color at 1:1.
                        self.sub_sample = 0;
                    }
                }
                0xffdd => {
                    if len == 4 {
                        self.res_interval = i32::from(self.be16(at(offset) + 2));
                    }
                }
                0xffc4 => {
                    offset += 2;
                    len = len.wrapping_sub(2);
                    if !self.get_huff_tables(at(offset), i32::from(len)) {
                        return Err(Error::Decode);
                    }
                }
                0xffdb => {
                    offset += 2;
                    len = len.wrapping_sub(2);
                    while len > 0 {
                        let table = self.byte(at(offset));
                        offset += 1;
                        if table & 0xf > 3 {
                            return Err(Error::Decode);
                        }
                        let base = usize::from(table & 0xf) * DCTSIZE;
                        if table & 0xf0 != 0 {
                            for i in 0..DCTSIZE {
                                let value = self.be16(at(offset));
                                if let Some(q) = self.quant.get_mut(base + i) {
                                    *q = value;
                                }
                                offset += 2;
                            }
                            len = len.wrapping_sub(129);
                        } else {
                            for i in 0..DCTSIZE {
                                let value = u16::from(self.byte(at(offset)));
                                if let Some(q) = self.quant.get_mut(base + i) {
                                    *q = value;
                                }
                                offset += 1;
                            }
                            len = len.wrapping_sub(65);
                        }
                    }
                }
                _ => {}
            }
            offset += i32::from(len);
        }
        if marker == 0xffda {
            offset -= i32::from(len);
            // JPEGDEC went on whether or not the scan header was valid.
            self.get_sos(&mut offset);
            if !self.make_huff_tables() {
                return Err(Error::Unsupported);
            }
            let n = self.filter(at(offset), 0, bytes_read - offset);
            self.vlc_off = 0;
            self.vlc_size = n;
            self.get_more_data(source);
            return Ok(());
        }
        Err(Error::Decode)
    }

    /// Copies DHT tables (into the pixel buffer, as JPEGDEC did) for `make_huff_tables`; false
    /// for bogus counts.
    fn get_huff_tables(&mut self, src: usize, mut len: i32) -> bool {
        let mut off = src;
        while len > 17 {
            let mut table = self.byte(off);
            off += 1;
            if table & 0x10 != 0 {
                // AC tables 0x10.. go after the four DC ones.
                table ^= 0x14;
            }
            // JPEGDEC's `1 << table` in an int, as the hardware shifts it.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "into a u8, as JPEGDEC kept it"
            )]
            let bit = 1u32.wrapping_shl(u32::from(table)) as u8;
            self.huff_tables_used |= bit;
            if table <= 7 {
                let mut base = usize::from(table) * HUFF_TABLEN;
                let mut total = 0;
                for i in 0..16 {
                    let count = self.byte(off);
                    total += i32::from(count);
                    self.set_pixel(base + i, count);
                    off += 1;
                }
                len -= 17;
                if total == 0 || total > 256 || total > len {
                    return false;
                }
                base += 16;
                for i in 0..usize::try_from(total).unwrap_or(0) {
                    let value = self.byte(off);
                    self.set_pixel(base + i, value);
                    off += 1;
                }
                len -= total;
            }
        }
        true
    }

    /// Expands the copied tables into JPEGDEC's lookup tables: for DC a 6-bit short table (with
    /// small values precomputed) and a long one for codes starting with five 1s; for AC 10-bit
    /// short and long ones. False for codes it cannot hold.
    #[allow(
        clippy::too_many_lines,
        clippy::explicit_counter_loop,
        clippy::cast_sign_loss,
        reason = "JPEGDEC's JPEGMakeHuffTables in one piece, to read against it"
    )]
    fn make_huff_tables(&mut self) -> bool {
        const MAX_LENGTH: i32 = 12;
        const MAX_MASK: i32 = 0x7f;
        for table in 0..4usize {
            if self.huff_tables_used & (1 << table) == 0 {
                continue;
            }
            let short = DC_BASE + table * DC_TABLE_SIZE;
            let long = short + 128;
            let mut counts = table * HUFF_TABLEN;
            let mut values = counts + 16;
            let mut code: i32 = 0;
            for bits in 1..=16i32 {
                let mut n = self.pixel(counts);
                counts += 1;
                if bits > MAX_LENGTH && n > 0 {
                    return false;
                }
                while n > 0 {
                    // First five bits 1 (a code shorter than 5 bits never is: JPEGDEC's negative
                    // shift came out as a shift by 27 or more).
                    let (start, count) = if bits >= 5 && (code >> (bits - 5)) == 0x1f {
                        let count = MAX_LENGTH - bits;
                        (long + at((code << count) & MAX_MASK), count)
                    } else {
                        let count = 6 - bits;
                        if count < 0 {
                            return false;
                        }
                        (short + at(code << count), count)
                    };
                    let mut entry = self.pixel(values);
                    values += 1;
                    let magnitude = i32::from(entry);
                    if entry != 0 && magnitude + bits <= 6 && self.mode != 0xc2 {
                        // The extra bits fit in the lookup: store the DC difference itself.
                        #[allow(clippy::cast_possible_truncation, reason = "at most 6 << 4")]
                        let tag = ((bits + magnitude) << 4) as u8;
                        entry |= tag;
                        let mut d = start + 512;
                        for j in 0..(1i32 << magnitude) {
                            let value = if j & (1 << (magnitude - 1)) != 0 {
                                j
                            } else {
                                j - ((1 << magnitude) - 1)
                            };
                            #[allow(clippy::cast_possible_truncation, reason = "as JPEGDEC's u8")]
                            let value = value as u8;
                            for _ in 0..(1i32 << (count - magnitude)) {
                                self.set_byte(d, value);
                                d += 1;
                            }
                        }
                    } else {
                        #[allow(clippy::cast_possible_truncation, reason = "as JPEGDEC's u8")]
                        let tag = (bits << 4) as u8;
                        entry |= tag;
                    }
                    for i in 0..(1usize << count) {
                        self.set_byte(start + i, entry);
                    }
                    code += 1;
                    n -= 1;
                }
                code <<= 1;
            }
        }
        if self.mode == 0xc2 {
            // Progressive: only the DC tables.
            return true;
        }
        for table in 0..4usize {
            if self.huff_tables_used & (1 << (table + 4)) == 0 {
                continue;
            }
            let mut counts = (table + 4) * HUFF_TABLEN;
            let mut values = counts + 16;
            if table * HUFF11SIZE >= 2 * HUFF11SIZE {
                // Room for two AC tables only.
                return false;
            }
            let short = table * HUFF11SIZE;
            let long = short + 1024;
            let mut code: i32 = 0;
            for bits in 1..=16i32 {
                let mut n = self.pixel(counts);
                counts += 1;
                while n > 0 {
                    // First six bits 1 (never for a code shorter than 6 bits, as above).
                    let (start, count) = if bits >= 6 && (code >> (bits - 6)) == 0x3f {
                        let count = 16 - bits;
                        (long + at((code << count) & 0x3ff), count)
                    } else {
                        let count = 10 - bits;
                        if count < 0 {
                            return false;
                        }
                        (short + at(code << count), count)
                    };
                    #[allow(clippy::cast_possible_truncation, reason = "at most 16 << 8 | 255")]
                    let entry = (i32::from(self.pixel(values)) | (bits << 8)) as u16;
                    values += 1;
                    for i in 0..(1usize << count) {
                        self.set_ac(start + i, entry);
                    }
                    code += 1;
                    n -= 1;
                }
                code <<= 1;
            }
        }
        true
    }

    /// Reads the scan header at `offset` (its length); on success moves `offset` past it. False
    /// for a header JPEGDEC refused, leaving `offset` where it was.
    fn get_sos(&mut self, offset: &mut i32) -> bool {
        let mut o = at(*offset);
        #[allow(clippy::cast_possible_wrap, reason = "JPEGDEC's int16_t")]
        let mut len = self.be16(o) as i16;
        o += 2;
        for c in &mut self.components {
            c.needed = false;
        }
        let count = self.byte(o);
        o += 1;
        len = len.wrapping_sub(3);
        if !(1..=4).contains(&count) || i32::from(len) != i32::from(count) * 2 + 3 {
            return false;
        }
        for _ in 0..count {
            let (id, tables) = (self.byte(o), self.byte(o + 1));
            o += 2;
            let Some(c) = self.components.iter_mut().find(|c| c.id == id) else {
                return false;
            };
            if tables & 0xf > 3 || tables & 0xf0 > 0x30 {
                return false;
            }
            c.dc = tables >> 4;
            c.ac = tables & 0xf;
            c.needed = true;
        }
        self.scan_start = self.byte(o);
        self.scan_end = self.byte(o + 1);
        let approx = self.byte(o + 2);
        self.approx_low = approx & 0xf;
        self.approx_high = approx >> 4;
        *offset = i32::try_from(o + 3).unwrap_or(i32::MAX);
        true
    }

    /// Copies `len` bytes at `src` to `dst` (at or before it) without stuffed zeros and markers
    /// (restart markers included), as JPEGDEC's `JPEGFilter`; returns the bytes kept. A 0xFF at the
    /// end waits for the next call.
    fn filter(&mut self, src: usize, dst: usize, len: i32) -> i32 {
        let (mut s, mut d) = (src, dst);
        // The last byte, none for a length under 1 (`bytes_read - offset` can be negative).
        let end =
            i64::try_from(src).map_or(None, |src| usize::try_from(src + i64::from(len) - 1).ok());
        if self.ff {
            if self.byte(s) == 0 {
                self.set_byte(d, 0xff);
                d += 1;
            }
            s += 1;
            self.ff = false;
        }
        if let Some(end) = end {
            while s < end {
                let c = self.byte(s);
                self.set_byte(d, c);
                d += 1;
                s += 1;
                if c == 0xff {
                    if self.byte(s) != 0 {
                        // A marker: drop both bytes.
                        d -= 1;
                    }
                    s += 1;
                }
            }
            if s == end {
                let c = self.byte(s);
                if c == 0xff {
                    self.ff = true;
                } else {
                    self.set_byte(d, c);
                    d += 1;
                }
            }
        }
        i32::try_from(d - dst).unwrap_or(0)
    }

    /// Moves the unread data to the front of the read buffer and reads and filters more.
    fn get_more_data(&mut self, source: &mut impl Source) {
        let delta = self.vlc_size - self.vlc_off;
        if !(0..FILE_BUF_I32 - 64).contains(&delta) {
            return;
        }
        if self.vlc_off != 0 {
            let (from, to) = (at(self.vlc_off), at(self.vlc_size));
            self.arena.copy_within(from..to, 0);
            self.vlc_size -= self.vlc_off;
            self.vlc_off = 0;
            self.bit_buf = 0;
        }
        if self.file_pos < self.file_size && self.vlc_size < FILE_BUF_I32 - 64 {
            let to = at(self.vlc_size);
            let n = self.read(source, to, FILE_BUF_SIZE - to);
            self.vlc_size += self.filter(to, to, n);
        }
    }
}

/// An `i32` offset as an index (negative ones, which JPEGDEC never made, as 0).
fn at(offset: i32) -> usize {
    usize::try_from(offset).unwrap_or(0)
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "tests"
)]
mod tests;
