//! Paths inside an EPUB, as lib/FsHelpers resolved them: `%XX` escapes decoded, then `..`
//! components applied and empty ones dropped. Both work in place on bytes; a path is not
//! necessarily UTF-8.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Replaces each `%` followed by two hex digits with the byte they name, and returns the new
/// length. Other `%` stay.
pub fn decode_uri_escapes(path: &mut [u8]) -> usize {
    let mut read = 0;
    let mut write = 0;
    while let Some(&b) = path.get(read) {
        let escaped = match (b, path.get(read + 1), path.get(read + 2)) {
            (b'%', Some(&high), Some(&low)) => hex_value(high).zip(hex_value(low)),
            _ => None,
        };
        let (out, used) = match escaped {
            Some((high, low)) => (high << 4 | low, 3),
            None => (b, 1),
        };
        if let Some(slot) = path.get_mut(write) {
            *slot = out;
        }
        write += 1;
        read += used;
    }
    write
}

/// Joins the components between `/` with single `/`s, dropping empty ones and letting each `..`
/// remove the component before it (or nothing, at the start), and returns the new length. There is
/// no leading or trailing `/`, and `.` stays.
pub fn normalise(path: &mut [u8]) -> usize {
    let mut len = 0;
    let mut start = 0;
    while start < path.len() {
        let end = path
            .get(start..)
            .and_then(|rest| rest.iter().position(|&b| b == b'/'))
            .map_or(path.len(), |at| start + at);
        if end > start {
            if path.get(start..end) == Some(b"..".as_slice()) {
                // Back to the `/` before the last component, or to the start.
                len = path
                    .get(..len)
                    .and_then(|kept| kept.iter().rposition(|&b| b == b'/'))
                    .unwrap_or(0);
            } else {
                if len > 0 {
                    if let Some(slot) = path.get_mut(len) {
                        *slot = b'/';
                    }
                    len += 1;
                }
                // The kept text never reaches past `start`, so this only moves bytes back.
                for from in start..end {
                    if let Some(b) = path.get(from).copied()
                        && let Some(slot) = path.get_mut(len)
                    {
                        *slot = b;
                    }
                    len += 1;
                }
            }
        }
        start = end + 1;
    }
    len
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::{decode_uri_escapes, normalise};
    use std::vec::Vec;

    fn decoded(s: &str) -> Vec<u8> {
        let mut b = s.as_bytes().to_vec();
        let n = decode_uri_escapes(&mut b);
        b.truncate(n);
        b
    }

    fn normalised(s: &[u8]) -> Vec<u8> {
        let mut b = s.to_vec();
        let n = normalise(&mut b);
        b.truncate(n);
        b
    }

    #[test]
    fn decodes_escapes() {
        assert_eq!(decoded("a%20b%2Fc%2f"), b"a b/c/");
        assert_eq!(decoded("%zz%4%41%"), b"%zz%4A%");
        assert_eq!(decoded("%e2%80%94%00"), b"\xE2\x80\x94\0");
        assert_eq!(decoded(""), b"");
    }

    #[test]
    fn normalises() {
        assert_eq!(normalised(b"/Books/../.crosspoint/x"), b".crosspoint/x");
        assert_eq!(normalised(b"/../../etc"), b"etc");
        assert_eq!(normalised(b"OPS//a/./b/../c/"), b"OPS/a/./c");
        assert_eq!(normalised(b"a/b/../../.."), b"");
        assert_eq!(normalised(b"..a/b.."), b"..a/b..");
        assert_eq!(normalised(b"///"), b"");
    }
}
