//! What the chapter parser puts before a list item.

/// The most bytes a marker takes: a sign, ten digits and a dot.
pub const MAX_MARKER: usize = 12;

/// The UTF-8 bullet (U+2022) of unordered items, and of items outside any list.
const BULLET: [u8; 3] = [0xE2, 0x80, 0xA2];

/// An open list.
#[derive(Clone, Copy, Debug, Default)]
pub struct List {
    pub ordered: bool,
    /// `list-style-type: none`.
    pub unmarked: bool,
    /// The number of the last item, when ordered.
    pub counter: i32,
}

/// Writes the marker of an item of `list` (`None` outside any list) to `out` and returns its
/// length: nothing when unmarked, a bullet when unordered, else the list's next number and a dot.
pub fn marker(list: Option<&mut List>, out: &mut [u8; MAX_MARKER]) -> usize {
    let Some(list) = list else {
        return bullet(out);
    };
    if list.unmarked {
        return 0;
    }
    if !list.ordered {
        return bullet(out);
    }
    let counter = &mut list.counter;
    *counter = counter.wrapping_add(1);
    // Digits from the end of `out`, then moved to its start.
    let mut n = counter.unsigned_abs();
    let mut start = MAX_MARKER - 1;
    if let Some(dot) = out.get_mut(start) {
        *dot = b'.';
    }
    loop {
        start -= 1;
        if let (Some(d), Ok(digit)) = (out.get_mut(start), u8::try_from(n % 10)) {
            *d = b'0' + digit;
        }
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if *counter < 0 {
        start -= 1;
        if let Some(sign) = out.get_mut(start) {
            *sign = b'-';
        }
    }
    out.copy_within(start.., 0);
    MAX_MARKER - start
}

fn bullet(out: &mut [u8; MAX_MARKER]) -> usize {
    let [a, b, c, ..] = out;
    [*a, *b, *c] = BULLET;
    BULLET.len()
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{List, MAX_MARKER, marker};
    use std::vec::Vec;

    fn mark(list: Option<&mut List>) -> Vec<u8> {
        let mut out = [0xffu8; MAX_MARKER];
        let n = marker(list, &mut out);
        out.get(..n).unwrap_or_default().to_vec()
    }

    #[test]
    fn markers() {
        let mut ordered = List {
            ordered: true,
            ..List::default()
        };
        assert_eq!(mark(Some(&mut ordered)), b"1.");
        assert_eq!(ordered.counter, 1);
        ordered.counter = 9;
        assert_eq!(mark(Some(&mut ordered)), b"10.");
        ordered.counter = 9_999;
        assert_eq!(mark(Some(&mut ordered)), b"10000.");
        ordered.counter = 99_999;
        assert_eq!(mark(Some(&mut ordered)), b"100000.");
        ordered.counter = i32::MAX;
        assert_eq!(mark(Some(&mut ordered)), b"-2147483648.");
        assert_eq!(ordered.counter, i32::MIN);
        assert_eq!(mark(None), "\u{2022}".as_bytes());
        let mut unordered = List {
            counter: 4,
            ..List::default()
        };
        assert_eq!(mark(Some(&mut unordered)), "\u{2022}".as_bytes());
        let mut unmarked = List {
            ordered: true,
            unmarked: true,
            counter: 4,
        };
        assert_eq!(mark(Some(&mut unmarked)), b"");
        assert_eq!((unordered.counter, unmarked.counter), (4, 4));
    }
}
