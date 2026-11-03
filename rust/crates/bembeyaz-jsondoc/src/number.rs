//! ArduinoJson's `parseNumber` (`Numbers/parseNumber.hpp`): a number's text read into an integer
//! when it is one that fits, else a float (a `float` when its mantissa and exponent allow, else a
//! `double`), by its own approximate decimal conversion.

use crate::float::{make_f32, make_f64};

/// What a number's text reads as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Number {
    Invalid,
    Unsigned(u64),
    Signed(i64),
    F32(f32),
    F64(f64),
}

/// `double`'s and `float`'s largest mantissa and exponent, as ArduinoJson's `FloatTraits`.
const MANTISSA_MAX_64: u64 = (1 << 52) - 1;
const EXPONENT_MAX_64: i32 = 308;
const MANTISSA_MAX_32: u64 = (1 << 23) - 1;
const EXPONENT_MAX_32: i32 = 38;

/// Reads digits from `s` at `*at`, while `each` takes them; the position moves past each one.
fn digits(s: &[u8], at: &mut usize, mut each: impl FnMut(u8) -> bool) {
    while let Some(&c) = s.get(*at) {
        if !c.is_ascii_digit() || !each(c - b'0') {
            return;
        }
        *at += 1;
    }
}

/// `s` (up to its first NUL) read as ArduinoJson reads a number.
pub(crate) fn parse_number(s: &[u8]) -> Number {
    let s = s.split(|b| *b == 0).next().unwrap_or_default();
    let mut at = 0;
    let negative = s.first() == Some(&b'-');
    if matches!(s.first(), Some(b'-' | b'+')) {
        at = 1;
    }
    match s.get(at) {
        Some(c) if c.is_ascii_digit() || *c == b'.' => {}
        _ => return Number::Invalid,
    }
    let mut mantissa: u64 = 0;
    // As ArduinoJson: when adding the digit is what overflows `u64`, the mantissa keeps the
    // multiplication by ten and the digit it could not add still counts as one more place, so a
    // number such as 18446744073709551616 reads ten times too large (1.844674e20); kept.
    digits(s, &mut at, |d| {
        if mantissa > u64::MAX / 10 {
            return false;
        }
        mantissa *= 10;
        if mantissa > u64::MAX - u64::from(d) {
            return false;
        }
        mantissa += u64::from(d);
        true
    });
    if at == s.len() {
        if !negative {
            return Number::Unsigned(mantissa);
        }
        if mantissa <= 1 << 63 {
            return Number::Signed((!mantissa).wrapping_add(1).cast_signed());
        }
    }
    fraction(s, at, mantissa, negative)
}

/// The rest of a number that is not an integer that fits: the digits past the mantissa's, the
/// fraction and the exponent.
fn fraction(s: &[u8], mut at: usize, mut mantissa: u64, negative: bool) -> Number {
    let mut offset: i16 = 0;
    while mantissa > MANTISSA_MAX_64 {
        mantissa /= 10;
        offset = offset.wrapping_add(1);
    }
    digits(s, &mut at, |_| {
        offset = offset.wrapping_add(1);
        true
    });
    if s.get(at) == Some(&b'.') {
        at += 1;
        digits(s, &mut at, |d| {
            if mantissa < MANTISSA_MAX_64 / 10 {
                mantissa = mantissa * 10 + u64::from(d);
                offset = offset.wrapping_sub(1);
            }
            true
        });
    }
    let exponent = match exponent(s, &mut at, i32::from(offset)) {
        Ok(exponent) => exponent,
        Err(overflow) => return overflow.signed(negative),
    };
    if at != s.len() {
        return Number::Invalid;
    }
    finish(mantissa, exponent.wrapping_add(i32::from(offset)), negative)
}

/// A number whose exponent ran past `double`'s: zero or infinity.
enum Overflow {
    Zero,
    Infinity,
}

impl Overflow {
    fn signed(self, negative: bool) -> Number {
        match (self, negative) {
            (Overflow::Zero, false) => Number::F32(0.0),
            (Overflow::Zero, true) => Number::F32(-0.0),
            (Overflow::Infinity, false) => Number::F64(f64::INFINITY),
            (Overflow::Infinity, true) => Number::F64(f64::NEG_INFINITY),
        }
    }
}

/// The written exponent (0 without one), or the overflow it makes with `offset`.
fn exponent(s: &[u8], at: &mut usize, offset: i32) -> Result<i32, Overflow> {
    if !matches!(s.get(*at), Some(b'e' | b'E')) {
        return Ok(0);
    }
    *at += 1;
    let negative = s.get(*at) == Some(&b'-');
    if matches!(s.get(*at), Some(b'-' | b'+')) {
        *at += 1;
    }
    let mut exponent: i32 = 0;
    let mut overflow = None;
    digits(s, at, |d| {
        exponent = exponent * 10 + i32::from(d);
        if exponent + offset > EXPONENT_MAX_64 {
            overflow = Some(if negative {
                Overflow::Zero
            } else {
                Overflow::Infinity
            });
            return false;
        }
        true
    });
    match overflow {
        Some(overflow) => Err(overflow),
        None => Ok(if negative { -exponent } else { exponent }),
    }
}

/// The float of `mantissa` × 10^`exponent`: a `double` when a `float` cannot hold the mantissa or
/// the exponent, else a `float`.
#[allow(
    clippy::cast_precision_loss,
    reason = "ArduinoJson converts the mantissa so"
)]
fn finish(mantissa: u64, exponent: i32, negative: bool) -> Number {
    let wide =
        !(-EXPONENT_MAX_32..=EXPONENT_MAX_32).contains(&exponent) || mantissa > MANTISSA_MAX_32;
    if wide {
        let value = make_f64(mantissa as f64, exponent);
        Number::F64(if negative { -value } else { value })
    } else {
        let value = make_f32(mantissa as f32, exponent);
        Number::F32(if negative { -value } else { value })
    }
}
