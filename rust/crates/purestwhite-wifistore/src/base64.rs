//! Standard base64 (RFC 4648, with padding), decoded as mbedtls's `mbedtls_base64_decode` takes
//! what the C++ firmware writes: `=` only at the end. mbedtls also skips line breaks, which the C++
//! firmware never writes; here they make the text not base64. The whole text is checked before its
//! length, so a text that is not base64 is that, however long.

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

/// `encoded` decoded into text of up to `N` bytes.
///
/// # Errors
/// `encoded` is not base64, or decodes to more than `N` bytes.
pub fn decode<const N: usize>(encoded: &[u8]) -> Result<Text<N>, DecodeError> {
    if !encoded.len().is_multiple_of(4) {
        return Err(DecodeError::Invalid);
    }
    let padding = encoded.iter().rev().take_while(|&&c| c == b'=').count();
    let digits = encoded.get(..encoded.len() - padding).unwrap_or_default();
    if padding > 2 || digits.iter().any(|&c| digit(c).is_none()) {
        return Err(DecodeError::Invalid);
    }
    if (encoded.len() / 4 * 3).saturating_sub(padding) > N {
        return Err(DecodeError::TooLong);
    }
    let mut out = [0u8; N];
    let mut len = 0;
    for quad in encoded.chunks(4) {
        let mut value = 0u32;
        let mut digits = 0;
        for &c in quad.iter().filter(|&&c| c != b'=') {
            value = value << 6 | digit(c).ok_or(DecodeError::Invalid)?;
            digits += 1;
        }
        let bytes: usize = match digits {
            4 => 3,
            3 => 2,
            2 => 1,
            _ => return Err(DecodeError::Invalid),
        };
        value <<= 6 * (4 - digits);
        for shift in [16, 8, 0].into_iter().take(bytes) {
            *out.get_mut(len).ok_or(DecodeError::TooLong)? = (value >> shift).to_le_bytes()[0]; // its low byte
            len += 1;
        }
    }
    // `=` only in the last quad, after its digits.
    if encoded
        .iter()
        .take(encoded.len() - padding)
        .any(|&c| c == b'=')
    {
        return Err(DecodeError::Invalid);
    }
    Text::from_bytes(out.get(..len).unwrap_or_default()).ok_or(DecodeError::TooLong)
}
