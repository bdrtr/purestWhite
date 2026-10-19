//! The rule cache file: a version byte, a flags byte, the rule count, then each rule in source
//! order as a length-prefixed key and its style ([`WIRE_BYTES`] bytes). Integers are
//! little-endian. A partial cache (stylesheets cut by a cap or low memory) serves the current
//! session; the next book load retries the stylesheets.

use crate::rules::{Insert, MAX_RULES, MAX_STORED_KEY, Rules, SELECTOR_POOL_CAP, Stop};
use crate::style::{Style, WIRE_BYTES};

/// Bump when the format or the rules change: section caches are rebuilt with it. Also change
/// `BB_CSS_CACHE_VERSION` in rust/include/bembeyaz/css.h and the check in bembeyaz-ffi.
pub const VERSION: u8 = 18;
const FLAG_PARTIAL: u8 = 1;
const KNOWN_FLAGS: u8 = FLAG_PARTIAL;

/// Where the cache is read from.
pub trait Source {
    /// Fills `buf` entirely, or returns false.
    fn read(&mut self, buf: &mut [u8]) -> bool;
    /// Bytes left.
    fn remaining(&mut self) -> usize;
    /// Moves past `len` bytes, or returns false.
    fn skip(&mut self, len: usize) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Complete,
    Partial,
    Invalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    Complete,
    /// Retry later: the rules did not fit in memory.
    LowMemory,
    Invalid,
    /// Another version wrote the file (or it has no version byte): it should be removed.
    StaleVersion,
}

fn read_u8(source: &mut impl Source) -> Option<u8> {
    let mut b = [0];
    source.read(&mut b).then_some(b[0])
}

fn read_u16(source: &mut impl Source) -> Option<u16> {
    let mut b = [0; 2];
    source.read(&mut b).then(|| u16::from_le_bytes(b))
}

/// The header, and whether it says partial; `None` when it is not one this version wrote.
fn header(source: &mut impl Source) -> Option<(bool, usize)> {
    if read_u8(source)? != VERSION {
        return None;
    }
    let flags = read_u8(source).filter(|f| f & !KNOWN_FLAGS == 0)?;
    let count = usize::from(read_u16(source)?);
    (count <= MAX_RULES).then_some((flags & FLAG_PARTIAL != 0, count))
}

/// What the cache holds, without reading its rules in. A complete cache is checked as it is
/// loaded, so only a partial one is walked here.
pub fn inspect(source: &mut impl Source) -> Status {
    let Some((partial, count)) = header(source) else {
        return Status::Invalid;
    };
    if !partial {
        return Status::Complete;
    }
    let mut selector_bytes = 0;
    for _ in 0..count {
        let Some(len) = read_u16(source)
            .map(usize::from)
            .filter(|&l| l != 0 && l <= MAX_STORED_KEY)
        else {
            return Status::Invalid;
        };
        selector_bytes += len;
        let record = len + WIRE_BYTES;
        if selector_bytes > SELECTOR_POOL_CAP || source.remaining() < record || !source.skip(record)
        {
            return Status::Invalid;
        }
    }
    if source.remaining() == 0 {
        Status::Partial
    } else {
        Status::Invalid
    }
}

/// Replaces `rules` with the cache's; they are left empty unless it loads completely.
pub fn load(rules: &mut Rules, source: &mut impl Source) -> Load {
    rules.clear();
    let result = load_rules(rules, source);
    if result != Load::Complete {
        rules.clear();
    }
    result
}

fn load_rules(rules: &mut Rules, source: &mut impl Source) -> Load {
    if read_u8(source) != Some(VERSION) {
        return Load::StaleVersion;
    }
    if read_u8(source).is_none_or(|f| f & !KNOWN_FLAGS != 0) {
        return Load::Invalid;
    }
    let Some(count) = read_u16(source)
        .map(usize::from)
        .filter(|&c| c <= MAX_RULES)
    else {
        return Load::Invalid;
    };
    match rules.reserve_rules(count) {
        Ok(()) => {}
        Err(Stop::OutOfMemory) => return Load::LowMemory,
        Err(Stop::Limit) => return Load::Invalid,
    }
    let mut buffer = [0u8; MAX_STORED_KEY];
    let mut wire = [0u8; WIRE_BYTES];
    for _ in 0..count {
        let Some(len) = read_u16(source).map(usize::from).filter(|&l| l != 0) else {
            return Load::Invalid;
        };
        let Some(key) = buffer.get_mut(..len) else {
            return Load::Invalid;
        };
        if !source.read(key) {
            return Load::Invalid;
        }
        let Some(style) = source
            .read(&mut wire)
            .then(|| Style::from_wire(&wire))
            .flatten()
        else {
            return Load::Invalid;
        };
        match rules.insert_stored(key, &style) {
            Ok(Insert::Inserted) => {}
            Err(Stop::OutOfMemory) => return Load::LowMemory,
            // A key twice, or past a cap: not a file this code wrote.
            Ok(Insert::Merged) | Err(Stop::Limit) => return Load::Invalid,
        }
    }
    if source.remaining() == 0 {
        Load::Complete
    } else {
        Load::Invalid
    }
}

/// Writes the rules through `write`, which returns false when it fails. False on a failed write
/// or when sorting the rules into source order does not fit in memory.
pub fn save(rules: &Rules, complete: bool, mut write: impl FnMut(&[u8]) -> bool) -> bool {
    let Some(records) = rules.in_source_order() else {
        return false;
    };
    let Ok(count) = u16::try_from(rules.len()) else {
        return false;
    };
    let [c0, c1] = count.to_le_bytes();
    if !write(&[VERSION, if complete { 0 } else { FLAG_PARTIAL }, c0, c1]) {
        return false;
    }
    for (key, style) in records {
        let Ok(len) = u16::try_from(key.len()) else {
            return false;
        };
        if !(write(&len.to_le_bytes()) && write(key) && write(&style.to_wire())) {
            return false;
        }
    }
    true
}
