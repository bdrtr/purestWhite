//! Standard base64 (RFC 4648, with padding). Decoded as mbedtls 3.6's `mbedtls_base64_decode`
//! decodes it: line breaks (`\n`, `\r\n`) are skipped anywhere, spaces only before a line break or
//! at the end, and `=` only at the end; the whole text is checked before its length is known, so
//! a text that is not base64 is that, however long. Encoded as Arduino's `base64::encode`
//! encodes it: padded, with no line breaks.

use alloc::vec::Vec;
use bembeyaz_jsondoc::Error;
use bembeyaz_store::password::Secret;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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

/// The text is not base64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Invalid;

/// How many digits and `=` `encoded` holds, and how many of them are `=`: mbedtls's first pass.
fn symbols(encoded: &[u8]) -> Result<(usize, usize), Invalid> {
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
            return Err(Invalid); // a space inside a line
        }
        if c == b'=' {
            equals += 1;
            if equals > 2 {
                return Err(Invalid);
            }
        } else if equals != 0 || digit(c).is_none() {
            return Err(Invalid);
        }
        count += 1;
    }
    if (count - equals) % 4 == 1 || !count.is_multiple_of(4) {
        return Err(Invalid);
    }
    Ok((count, equals))
}

/// How many bytes `encoded` decodes to.
///
/// # Errors
/// [`Invalid`] when it is not base64.
pub fn decoded_len(encoded: &[u8]) -> Result<usize, Invalid> {
    let (count, equals) = symbols(encoded)?;
    Ok(count / 4 * 3 - equals)
}

/// `encoded`, which [`decoded_len`] took, decoded onto the end of `out` (which has room for it, so
/// nothing is moved and no earlier copy is left behind).
pub fn decode_into(encoded: &[u8], out: &mut Vec<u8>) {
    let (mut value, mut digits) = (0u32, 0);
    let mut put = |byte: u32| out.push(byte.to_le_bytes()[0]); // its low byte
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
}

/// `bytes` encoded, as a secret (it may encode one).
///
/// # Errors
/// [`Error::NoMemory`] when memory runs out.
pub fn encode(bytes: &[u8]) -> Result<Secret, Error> {
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len().div_ceil(3) * 4)
        .map_err(|_| Error::NoMemory)?;
    for chunk in bytes.chunks(3) {
        let b = |i: usize| chunk.get(i).copied().map_or(0, u32::from);
        let value = b(0) << 16 | b(1) << 8 | b(2);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            let symbol = if i <= chunk.len() {
                let index = usize::try_from(value >> shift & 63).unwrap_or(0);
                ALPHABET.get(index).copied().unwrap_or(b'=')
            } else {
                b'='
            };
            out.push(symbol);
        }
    }
    Ok(Secret::new(out))
}
