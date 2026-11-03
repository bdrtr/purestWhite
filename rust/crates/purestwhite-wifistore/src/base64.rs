//! Standard base64 (RFC 4648, with padding), decoded as mbedtls 3.6's `mbedtls_base64_decode`
//! decodes it: line breaks (`\n`, `\r\n`) are skipped anywhere, spaces only before a line break or
//! at the end, and `=` only at the end. The whole text is checked before its length, so a text that
//! is not base64 is that, however long.

use crate::text::Text;

/// The value of a base64 digit.
fn digit(c: u8) -> Option<u32> {
    let value = match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    };
    Some(u32::from(value))
}

/// Why `encoded` could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not base64.
    Invalid,
    /// It decodes to more than the text holds.
    TooLong,
}

/// How many digits and `=` `encoded` holds, and how many of them are `=`: mbedtls's first pass.
fn symbols(encoded: &[u8]) -> Result<(usize, usize), DecodeError> {
    let (mut count, mut equals, mut at) = (0usize, 0usize, 0usize);
    while at < encoded.len() {
        let spaces_from = at;
        while encoded.get(at) == Some(&b' ') {
            at += 1;
        }
        let Some(&c) = encoded.get(at) else {
            break; // spaces at the end
        };
        at += 1;
        if c == b'\n' || (c == b'\r' && encoded.get(at) == Some(&b'\n')) {
            continue;
        }
        if at - 1 > spaces_from {
            return Err(DecodeError::Invalid); // a space inside a line
        }
        if c == b'=' {
            equals += 1;
            if equals > 2 {
                return Err(DecodeError::Invalid);
            }
        } else if equals != 0 || digit(c).is_none() {
            return Err(DecodeError::Invalid);
        }
        count += 1;
    }
    if (count - equals) % 4 == 1 || !count.is_multiple_of(4) {
        return Err(DecodeError::Invalid);
    }
    Ok((count, equals))
}

/// `encoded` decoded into text of up to `N` bytes.
///
/// # Errors
/// `encoded` is not base64, or decodes to more than `N` bytes.
pub fn decode<const N: usize>(encoded: &[u8]) -> Result<Text<N>, DecodeError> {
    let (count, equals) = symbols(encoded)?;
    let len = count / 4 * 3 - equals;
    if len > N {
        return Err(DecodeError::TooLong);
    }
    let mut out = [0u8; N];
    let mut written = 0;
    let mut put = |byte: u32| {
        if let Some(slot) = out.get_mut(written) {
            *slot = byte.to_le_bytes()[0]; // its low byte
            written += 1;
        }
    };
    let (mut value, mut digits) = (0u32, 0);
    for &c in encoded {
        if c == b'=' {
            break;
        }
        let Some(d) = digit(c) else {
            continue; // a line break or a space
        };
        value = value << 6 | d;
        digits += 1;
        if digits == 4 {
            digits = 0;
            put(value >> 16);
            put(value >> 8);
            put(value);
        }
    }
    match digits {
        3 => {
            put(value >> 10);
            put(value >> 2);
        }
        2 => put(value >> 4),
        _ => {}
    }
    Text::from_bytes(out.get(..len).unwrap_or_default()).ok_or(DecodeError::TooLong)
}
