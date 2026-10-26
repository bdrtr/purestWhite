//! Short (8.3) names for new entries, from fatgen103's "Basis-Name Generation": the basis name
//! from a long name, and the numeric tails ("~1") that set it apart from the others in its
//! directory. As `SdFat` makes them, only ASCII goes in; every other character becomes `_`.

/// A short name's base length; its extension takes the other three bytes.
const BASE: usize = 8;

/// The basis of a long name's short name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Basis {
    /// Base and extension, space-padded, as stored.
    pub(crate) name: [u8; 11],
    base_len: usize,
    /// Whether the long name is this short name exactly; otherwise it is stored beside it.
    pub(crate) exact: bool,
    /// Whether the short name needs a numeric tail even without a clash: the long name lost
    /// characters, or did not fit 8.3.
    pub(crate) tail: bool,
}

/// Whether `c`, upper-cased ASCII, may stand in a short name as it is.
fn short_char(c: u8) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit() || b"!#$%&'()-@^_`{}~".contains(&c)
}

/// The basis of `long`, a valid long name (see `valid_long`).
pub(crate) fn basis(long: &str) -> Basis {
    let mut name = [b' '; 11];
    let (mut lossy, mut fits) = (false, true);
    // The extension is what follows the last dot, unless the name starts with its only dot.
    let body = long.trim_start_matches('.');
    if body.len() != long.len() {
        fits = false;
    }
    let (base, ext) = match body.rsplit_once('.') {
        Some((base, ext)) => (base, Some(ext)),
        None => (body, None),
    };
    let mut put = |part: &str, to: &mut [u8], drop_dots: bool| -> usize {
        let mut len = 0;
        for c in part.chars() {
            if c == ' ' || (drop_dots && c == '.') {
                fits = false;
                continue;
            }
            let b = if c.is_ascii() {
                let b = c.to_ascii_uppercase() as u8;
                if short_char(b) {
                    b
                } else {
                    lossy = true;
                    b'_'
                }
            } else {
                lossy = true;
                b'_'
            };
            match to.get_mut(len) {
                Some(slot) => {
                    *slot = b;
                    len += 1;
                }
                None => fits = false,
            }
        }
        len
    };
    let (base_part, ext_part) = name.split_at_mut(BASE);
    let mut base_len = put(base, base_part, true);
    if let Some(ext) = ext {
        put(ext, ext_part, false);
    }
    if base_len == 0 {
        // Nothing left of the base, as in "+.txt": one `_`.
        if let Some(first) = name.first_mut() {
            *first = b'_';
        }
        base_len = 1;
        lossy = true;
    }
    let exact = !lossy && fits && is_written(&name, long);
    Basis {
        name,
        base_len,
        exact,
        tail: lossy || !fits,
    }
}

/// Whether `long` is the short name `name` written as "BASE.EXT".
fn is_written(name: &[u8; 11], long: &str) -> bool {
    let (base, ext) = name.split_at(BASE);
    let (mut text, mut len) = ([0u8; 12], 0);
    let ext = ext.iter().filter(|&&b| b != b' ');
    let dot = ext.clone().next().map(|_| &b'.');
    for &b in base.iter().filter(|&&b| b != b' ').chain(dot).chain(ext) {
        if let Some(slot) = text.get_mut(len) {
            *slot = b;
            len += 1;
        }
    }
    text.get(..len) == Some(long.as_bytes())
}

/// Decimal digits in `n`.
fn digits(mut n: u32) -> usize {
    let mut count = 1;
    while n >= 10 {
        n /= 10;
        count += 1;
    }
    count
}

impl Basis {
    /// The short name with numeric tail `n` (1 to 999 999): as much of the base as leaves room
    /// for "~n", then "~n".
    pub(crate) fn with_tail(&self, n: u32) -> [u8; 11] {
        let mut name = self.name;
        let keep = self.base_len.min(BASE - 1 - digits(n));
        let mut at = keep;
        let mut tail = [0u8; 7];
        let len = 1 + digits(n);
        tail[0] = b'~';
        let mut rest = n;
        for i in (1..len).rev() {
            if let Some(slot) = tail.get_mut(i) {
                *slot = b'0' + (rest % 10) as u8;
            }
            rest /= 10;
        }
        for &b in tail.get(..len).unwrap_or_default() {
            if let Some(slot) = name.get_mut(at) {
                *slot = b;
            }
            at += 1;
        }
        for slot in name.get_mut(at..BASE).unwrap_or_default() {
            *slot = b' ';
        }
        name
    }

    /// The tail `short` carries, if it is this basis with one: the same extension, and a base
    /// of the basis' first characters, "~" and the number.
    pub(crate) fn tail_of(&self, short: &[u8; 11]) -> Option<u32> {
        if short.get(BASE..) != self.name.get(BASE..) {
            return None;
        }
        let base = short.get(..BASE)?;
        let end = base.iter().rposition(|&b| b != b' ')? + 1;
        let tilde = base.get(..end)?.iter().rposition(|&b| b == b'~')?;
        let number = base.get(tilde + 1..end)?;
        if number.is_empty() || number.first() == Some(&b'0') {
            return None;
        }
        let mut n = 0u32;
        for &b in number {
            if !b.is_ascii_digit() {
                return None;
            }
            n = n * 10 + u32::from(b - b'0');
        }
        (self.with_tail(n) == *short).then_some(n)
    }
}

/// Whether `name` (already without leading spaces and trailing dots and spaces) can be a long
/// name: not empty, "." or "..", at most 255 UTF-16 units, without control characters or
/// `\ / : * ? " < > |`.
pub(crate) fn valid_long(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.encode_utf16().count() <= 255
        && !name
            .chars()
            .any(|c| c < ' ' || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}
