//! ArduinoJson's `as<T>()` and `is<T>()` (`Variant/ConverterImpl.hpp`, `VariantData.hpp`,
//! `Numbers/convertNumber.hpp`), on a value or a missing member (`None`, which reads as a null). A
//! number converts to an integer type only within its range (else it is 0; to `f32` it always
//! converts), a string's text is read as a number; asked for as a string, any other value gives
//! its JSON (`null` for a missing one).

use crate::Value;
use crate::number::{Number, parse_number};
use alloc::vec::Vec;

/// A number type `as<T>()` converts to, with ArduinoJson's `canConvertNumber` limits.
pub trait Target: Sized + Default {
    fn from_unsigned(value: u64) -> Self;
    fn from_signed(value: i64) -> Self;
    fn from_f32(value: f32) -> Self;
    fn from_f64(value: f64) -> Self;
    fn from_bool(value: bool) -> Self;
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
impl Target for u16 {
    fn from_unsigned(value: u64) -> u16 {
        u16::try_from(value).unwrap_or(0)
    }
    fn from_signed(value: i64) -> u16 {
        u16::try_from(value).unwrap_or(0)
    }
    fn from_f32(value: f32) -> u16 {
        if (0.0..=65_535.0).contains(&value) {
            value as u16
        } else {
            0
        }
    }
    fn from_f64(value: f64) -> u16 {
        if (0.0..=65_535.0).contains(&value) {
            value as u16
        } else {
            0
        }
    }
    fn from_bool(value: bool) -> u16 {
        u16::from(value)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
impl Target for u32 {
    fn from_unsigned(value: u64) -> u32 {
        u32::try_from(value).unwrap_or(0)
    }
    fn from_signed(value: i64) -> u32 {
        u32::try_from(value).unwrap_or(0)
    }
    fn from_f32(value: f32) -> u32 {
        // The largest float below 2^32 (ArduinoJson's `highest_for<uint32_t>`).
        let highest = f32::from_bits(0x4F7F_FFFF);
        if value >= 0.0 && value <= highest {
            value as u32
        } else {
            0
        }
    }
    fn from_f64(value: f64) -> u32 {
        if (0.0..=4_294_967_295.0).contains(&value) {
            value as u32
        } else {
            0
        }
    }
    fn from_bool(value: bool) -> u32 {
        u32::from(value)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
impl Target for i64 {
    fn from_unsigned(value: u64) -> i64 {
        i64::try_from(value).unwrap_or(0)
    }
    fn from_signed(value: i64) -> i64 {
        value
    }
    fn from_f32(value: f32) -> i64 {
        let highest = f32::from_bits(0x5EFF_FFFF);
        if value >= i64::MIN as f32 && value <= highest {
            value as i64
        } else {
            0
        }
    }
    fn from_f64(value: f64) -> i64 {
        let highest = f64::from_bits(0x43DF_FFFF_FFFF_FFFF);
        if value >= i64::MIN as f64 && value <= highest {
            value as i64
        } else {
            0
        }
    }
    fn from_bool(value: bool) -> i64 {
        i64::from(value)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
impl Target for f32 {
    fn from_unsigned(value: u64) -> f32 {
        value as f32
    }
    fn from_signed(value: i64) -> f32 {
        value as f32
    }
    fn from_f32(value: f32) -> f32 {
        value
    }
    fn from_f64(value: f64) -> f32 {
        value as f32
    }
    fn from_bool(value: bool) -> f32 {
        if value { 1.0 } else { 0.0 }
    }
}

/// `value.as<T>()` for a number type.
#[must_use]
pub fn as_number<T: Target>(value: Option<&Value>) -> T {
    match value {
        Some(Value::Bool(b)) => T::from_bool(*b),
        Some(Value::U32(n)) => T::from_unsigned(u64::from(*n)),
        Some(Value::U64(n)) => T::from_unsigned(*n),
        Some(Value::I32(n)) => T::from_signed(i64::from(*n)),
        Some(Value::I64(n)) => T::from_signed(*n),
        Some(Value::F32(f)) => T::from_f32(*f),
        Some(Value::F64(f)) => T::from_f64(*f),
        Some(Value::Str(s)) => match parse_number(s) {
            Number::Unsigned(n) => T::from_unsigned(n),
            Number::Signed(n) => T::from_signed(n),
            Number::F32(f) => T::from_f32(f),
            Number::F64(f) => T::from_f64(f),
            Number::Invalid => T::default(),
        },
        _ => T::default(),
    }
}

/// `value.as<std::string>()`: a string's bytes, else the value's JSON (`null` for a missing one).
#[must_use]
pub fn as_string(value: Option<&Value>) -> Vec<u8> {
    match value {
        Some(Value::Str(s)) => s.clone(),
        Some(other) => other.to_json(),
        None => Value::Null.to_json(),
    }
}

/// `value.as<const char*>()`: a string's bytes, else none.
#[must_use]
pub fn as_str(value: Option<&Value>) -> Option<&[u8]> {
    match value {
        Some(Value::Str(s)) => Some(s),
        _ => None,
    }
}

/// `value.is<long long>()`: an integer within `i64`.
#[must_use]
pub fn is_i64(value: Option<&Value>) -> bool {
    match value {
        Some(Value::U32(_) | Value::I32(_) | Value::I64(_)) => true,
        Some(Value::U64(n)) => i64::try_from(*n).is_ok(),
        _ => false,
    }
}

/// `value.is<double>()`: any number, integers too.
#[must_use]
pub fn is_number(value: Option<&Value>) -> bool {
    matches!(
        value,
        Some(
            Value::U32(_)
                | Value::I32(_)
                | Value::U64(_)
                | Value::I64(_)
                | Value::F32(_)
                | Value::F64(_)
        )
    )
}
