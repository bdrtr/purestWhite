//! Directories: their entries, with long names joined from their pieces, and paths.

use crate::upcase::upcase;
use crate::{BlockDevice, Error, Kind, Volume, byte_at, u16_at, u32_at};

pub(crate) const ENTRY: usize = 32;
pub(crate) const ENTRIES_PER_BLOCK: u32 = 16;
/// The most entries a directory may hold (fatgen103).
pub(crate) const MAX_ENTRIES: u32 = 65_536;
const ATTR_VOLUME: u8 = 0x08;
pub(crate) const ATTR_DIRECTORY: u8 = 0x10;
/// Read-only, hidden, system and volume together: a long-name piece.
pub(crate) const ATTR_LONG_NAME: u8 = 0x0F;
pub(crate) const DELETED: u8 = 0xE5;
/// A short name whose first byte is really 0xE5.
const KANJI_E5: u8 = 0x05;
/// The flags of a short name shown in lower case (Windows NT's, which Linux follows).
const LOWER_BASE: u8 = 0x08;
const LOWER_EXT: u8 = 0x10;
/// UTF-16 units in a long-name piece, and the most pieces a name has.
pub(crate) const PIECE_UNITS: usize = 13;
pub(crate) const MAX_PIECES: usize = 20;
/// Where a piece's units are.
pub(crate) const PIECE_OFFSETS: [usize; PIECE_UNITS] =
    [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
/// UTF-8 bytes of the longest name: 255 UTF-16 units, at most 3 bytes each.
const NAME_BYTES: usize = 768;

/// A directory: FAT16's fixed root (cluster 0), or a cluster chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dir {
    pub(crate) cluster: u32,
}

/// An entry's name in UTF-8: the long name, or the short one when there is none.
pub struct Name {
    bytes: [u8; NAME_BYTES],
    len: usize,
}

impl Default for Name {
    fn default() -> Self {
        Self::new()
    }
}

impl Name {
    #[must_use]
    pub const fn new() -> Self {
        Name {
            bytes: [0; NAME_BYTES],
            len: 0,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.bytes.get(..self.len).unwrap_or_default()).unwrap_or_default()
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn push(&mut self, c: char) {
        let mut utf8 = [0; 4];
        let encoded = c.encode_utf8(&mut utf8).as_bytes();
        if let Some(to) = self.bytes.get_mut(self.len..self.len + encoded.len()) {
            to.copy_from_slice(encoded);
            self.len += encoded.len();
        }
    }
}

/// A directory entry: a file or a directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub(crate) attributes: u8,
    /// The first cluster; 0 for an empty file.
    pub(crate) cluster: u32,
    pub(crate) size: u32,
    /// The short name as stored: 8 bytes of base, 3 of extension, space-padded.
    pub(crate) short_name: [u8; 11],
    /// Where it is: its directory, its short entry's index there, and the index of its first
    /// long-name piece (the short entry's own without a long name).
    pub(crate) parent: Dir,
    pub(crate) index: u32,
    pub(crate) first: u32,
}

impl Entry {
    /// The file or directory short entry `raw`, at `index` in `parent`, as if it had no long name.
    fn read(kind: Kind, raw: &[u8; ENTRY], parent: Dir, index: u32) -> Self {
        let mut short_name = [0u8; 11];
        short_name.copy_from_slice(raw.get(..11).unwrap_or(&[0; 11]));
        let high = match kind {
            Kind::Fat16 => 0,
            Kind::Fat32 => u32::from(u16_at(raw, 20)) << 16,
        };
        Entry {
            attributes: byte_at(raw, 11),
            cluster: high | u32::from(u16_at(raw, 26)),
            size: u32_at(raw, 28),
            short_name,
            parent,
            index,
            first: index,
        }
    }

    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.attributes & ATTR_DIRECTORY != 0
    }

    /// The file's size in bytes; 0 for a directory.
    #[must_use]
    pub fn size(&self) -> u32 {
        if self.is_dir() { 0 } else { self.size }
    }

    /// The directory this entry is; `None` for a file, or a directory entry without a cluster
    /// (damaged: cluster 0 is FAT16's root, which no entry but ".." names).
    #[must_use]
    pub fn dir(&self) -> Option<Dir> {
        (self.is_dir() && self.cluster >= 2).then_some(Dir {
            cluster: self.cluster,
        })
    }
}

/// A place in a directory's listing.
pub struct DirCursor {
    dir: Dir,
    /// The next entry's index.
    pub(crate) index: u32,
    /// The cluster holding entry `index`, and its place in the chain (cluster directories).
    cluster: u32,
    ordinal: u32,
    ended: bool,
}

/// A long name's pieces as they come, last piece first.
struct LongName {
    units: [u16; PIECE_UNITS * MAX_PIECES],
    /// Pieces in the name, and the next piece expected (0 when complete or not started).
    count: u8,
    next: u8,
    checksum: u8,
}

impl LongName {
    const fn new() -> Self {
        LongName {
            units: [0; PIECE_UNITS * MAX_PIECES],
            count: 0,
            next: 0,
            checksum: 0,
        }
    }

    fn reset(&mut self) {
        self.count = 0;
        self.next = 0;
    }

    /// Takes a piece; one out of order or with another checksum starts over.
    fn add(&mut self, raw: &[u8; ENTRY]) {
        let sequence = raw[0];
        let order = sequence & 0x1F;
        let checksum = raw[13];
        if sequence & 0x40 != 0 {
            if order == 0 || usize::from(order) > MAX_PIECES {
                self.reset();
                return;
            }
            self.count = order;
            self.next = order;
            self.checksum = checksum;
        } else if self.next == 0 || order != self.next || checksum != self.checksum {
            self.reset();
            return;
        }
        let start = usize::from(order - 1) * PIECE_UNITS;
        for (i, &at) in PIECE_OFFSETS.iter().enumerate() {
            if let Some(unit) = self.units.get_mut(start + i) {
                *unit = u16_at(raw, at);
            }
        }
        self.next = order - 1;
    }

    /// The name into `name`, if every piece came and they belong to the short entry of
    /// `checksum`.
    fn finish(&self, checksum: u8, name: &mut Name) -> bool {
        if self.count == 0 || self.next != 0 || self.checksum != checksum {
            return false;
        }
        let units = self
            .units
            .get(..usize::from(self.count) * PIECE_UNITS)
            .unwrap_or_default();
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        name.clear();
        for c in char::decode_utf16(units.iter().take(end).copied()) {
            name.push(c.unwrap_or(char::REPLACEMENT_CHARACTER));
        }
        name.len > 0
    }
}

/// A short name's checksum, which its long-name pieces carry.
pub(crate) fn checksum(short: &[u8]) -> u8 {
    short
        .iter()
        .fold(0u8, |sum, &b| sum.rotate_right(1).wrapping_add(b))
}

/// The short name's base and extension, without their padding.
fn short_parts(short: &[u8; 11]) -> (&[u8], &[u8]) {
    let trim = |s: &'_ [u8]| -> usize { s.iter().rposition(|&b| b != b' ').map_or(0, |i| i + 1) };
    let (base, ext) = short.split_at(8);
    (
        base.get(..trim(base)).unwrap_or_default(),
        ext.get(..trim(ext)).unwrap_or_default(),
    )
}

/// A short name as text: "BASE.EXT", lower case where the entry's flags say so.
fn short_name(short: &[u8; 11], flags: u8, name: &mut Name) {
    name.clear();
    let (base, ext) = short_parts(short);
    push_short(name, base, flags & LOWER_BASE != 0, true);
    if !ext.is_empty() {
        name.push('.');
        push_short(name, ext, flags & LOWER_EXT != 0, false);
    }
}

/// Code page 437's upper half: FAT's default OEM code page for short names, as Linux and US
/// Windows read them.
const CP437: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

/// A short name's byte as a character, in code page 437.
fn oem(b: u8) -> char {
    if b.is_ascii() {
        char::from(b)
    } else {
        CP437
            .get(usize::from(b - 0x80))
            .copied()
            .unwrap_or(char::REPLACEMENT_CHARACTER)
    }
}

/// A short name's part, in lower case when asked; a base's first byte 0x05 is 0xE5.
fn push_short(name: &mut Name, bytes: &[u8], lower: bool, base: bool) {
    for (i, &b) in bytes.iter().enumerate() {
        let c = oem(if base && i == 0 && b == KANJI_E5 {
            DELETED
        } else {
            b
        });
        if lower {
            for l in c.to_lowercase() {
                name.push(l);
            }
        } else {
            name.push(c);
        }
    }
}

/// Whether `wanted` is the short name, ignoring ASCII case.
pub(crate) fn short_matches(short: &[u8; 11], wanted: &str) -> bool {
    let (base, ext) = short_parts(short);
    let wanted = wanted.as_bytes();
    let (w_base, w_ext) = match wanted.iter().rposition(|&b| b == b'.') {
        Some(dot) => (
            wanted.get(..dot).unwrap_or_default(),
            wanted.get(dot + 1..).unwrap_or_default(),
        ),
        None => (wanted, &[][..]),
    };
    !base.is_empty() && base.eq_ignore_ascii_case(w_base) && ext.eq_ignore_ascii_case(w_ext)
}

/// "." and "..".
fn is_dot(raw: &[u8; ENTRY]) -> bool {
    raw.get(..11) == Some(b".          ") || raw.get(..11) == Some(b"..         ")
}

impl<D: BlockDevice> Volume<D> {
    /// The root directory.
    #[must_use]
    pub fn root(&self) -> Dir {
        Dir {
            cluster: match self.layout.kind {
                Kind::Fat16 => 0,
                Kind::Fat32 => self.layout.root_cluster,
            },
        }
    }

    /// A cursor at the start of `dir`'s listing.
    #[must_use]
    pub fn cursor(&self, dir: Dir) -> DirCursor {
        DirCursor {
            dir,
            index: 0,
            cluster: dir.cluster,
            ordinal: 0,
            ended: false,
        }
    }

    /// The next raw entry of the listing, or `None` past its last slot.
    pub(crate) async fn raw_entry(
        &mut self,
        cursor: &mut DirCursor,
    ) -> Result<Option<[u8; ENTRY]>, Error<D::Error>> {
        let l = self.layout;
        let index = cursor.index;
        if index >= MAX_ENTRIES {
            return Ok(None);
        }
        let block = if cursor.dir.cluster == 0 {
            if l.kind != Kind::Fat16 {
                return Err(Error::Corrupt);
            }
            if index >= l.root_entries {
                return Ok(None);
            }
            l.root_start + index / ENTRIES_PER_BLOCK
        } else {
            let per_cluster = l.cluster_blocks * ENTRIES_PER_BLOCK;
            let ordinal = index / per_cluster;
            while cursor.ordinal < ordinal {
                match self.next_cluster(cursor.cluster).await? {
                    Some(next) => {
                        cursor.cluster = next;
                        cursor.ordinal += 1;
                    }
                    None => return Ok(None),
                }
            }
            if !l.is_cluster(cursor.cluster) {
                return Err(Error::Corrupt);
            }
            l.cluster_block(cursor.cluster) + (index % per_cluster) / ENTRIES_PER_BLOCK
        };
        let data = self.data_block(block).await?;
        let at = (index % ENTRIES_PER_BLOCK) as usize * ENTRY;
        let raw = data
            .get(at..at + ENTRY)
            .and_then(|r| r.try_into().ok())
            .ok_or(Error::Corrupt)?;
        cursor.index += 1;
        Ok(Some(raw))
    }

    /// The next file or directory of the listing, with its name in `name`; `None` at its end.
    /// "." and "..", the volume label and deleted entries are skipped.
    ///
    /// # Errors
    /// When the device fails or the directory's chain is damaged.
    pub async fn next(
        &mut self,
        cursor: &mut DirCursor,
        name: &mut Name,
    ) -> Result<Option<Entry>, Error<D::Error>> {
        let mut long = LongName::new();
        // The index of the long name's first piece.
        let mut long_start = 0;
        while !cursor.ended {
            let index = cursor.index;
            let Some(raw) = self.raw_entry(cursor).await? else {
                cursor.ended = true;
                break;
            };
            match raw[0] {
                0 => {
                    cursor.ended = true;
                    break;
                }
                DELETED => {
                    long.reset();
                    continue;
                }
                _ => {}
            }
            let attributes = raw[11];
            if attributes & 0x3F == ATTR_LONG_NAME {
                if raw[0] & 0x40 != 0 {
                    long_start = index;
                }
                long.add(&raw);
                continue;
            }
            if attributes & ATTR_VOLUME != 0 || is_dot(&raw) {
                long.reset();
                continue;
            }
            let mut entry = Entry::read(self.layout.kind, &raw, cursor.dir, index);
            if long.finish(checksum(&entry.short_name), name) {
                entry.first = long_start;
            } else {
                short_name(&entry.short_name, byte_at(&raw, 12), name);
            }
            return Ok(Some(entry));
        }
        Ok(None)
    }

    /// The entry named `wanted` in `dir`: its name (see `same_name`), or its short name ignoring
    /// ASCII case.
    async fn find_in(&mut self, dir: Dir, wanted: &str) -> Result<Entry, Error<D::Error>> {
        let wanted = trim_name(wanted);
        let mut cursor = self.cursor(dir);
        let mut name = Name::new();
        while let Some(entry) = self.next(&mut cursor, &mut name).await? {
            if same_name(name.as_str(), wanted) || short_matches(&entry.short_name, wanted) {
                return Ok(entry);
            }
        }
        Err(Error::NotFound)
    }

    /// The entry at `path` from `dir`, its parts separated by '/'.
    ///
    /// # Errors
    /// When nothing is there (an empty path names `dir` itself, which has no entry here), the
    /// path goes through a file, a directory on the way is damaged, or the device fails.
    pub async fn find_from(&mut self, mut dir: Dir, path: &str) -> Result<Entry, Error<D::Error>> {
        let mut parts = path.split('/').filter(|p| !p.is_empty()).peekable();
        while let Some(part) = parts.next() {
            let entry = self.find_in(dir, part).await?;
            if parts.peek().is_none() {
                return Ok(entry);
            }
            dir = match entry.dir() {
                Some(dir) => dir,
                None if entry.is_dir() => return Err(Error::Corrupt),
                None => return Err(Error::NotADirectory),
            };
        }
        Err(Error::NotFound)
    }

    /// The entry at `path`, from the root.
    ///
    /// # Errors
    /// As `find_from`'s.
    pub async fn find(&mut self, path: &str) -> Result<Entry, Error<D::Error>> {
        self.find_from(self.root(), path).await
    }

    /// The directory at `path`; the root for an empty path or "/".
    ///
    /// # Errors
    /// As `find`'s, or when it is a file.
    pub async fn open_dir(&mut self, path: &str) -> Result<Dir, Error<D::Error>> {
        if path.split('/').all(str::is_empty) {
            return Ok(self.root());
        }
        let entry = self.find(path).await?;
        match entry.dir() {
            Some(dir) => Ok(dir),
            None if entry.is_dir() => Err(Error::Corrupt),
            None => Err(Error::NotADirectory),
        }
    }
}

/// A name as `SdFat` takes one to look up: without leading spaces, trailing dots and spaces.
pub(crate) fn trim_name(name: &str) -> &str {
    name.trim_start_matches(' ').trim_end_matches(['.', ' '])
}

/// Whether two names are the same, as `SdFat` compares long names: each UTF-16 unit in upper case
/// by its table (see `upcase`).
pub(crate) fn same_name(a: &str, b: &str) -> bool {
    a.encode_utf16()
        .map(upcase)
        .eq(b.encode_utf16().map(upcase))
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::{ENTRY, LongName, Name};

    pub(crate) fn checksum(short: &[u8; 11]) -> u8 {
        super::checksum(short)
    }

    /// A name of one long-name piece `raw`, for the short entry `short`.
    pub(crate) fn one_piece(raw: &[u8; ENTRY], short: &[u8; 11], name: &mut Name) -> bool {
        let mut long = LongName::new();
        long.add(raw);
        long.finish(super::checksum(short), name)
    }
}
