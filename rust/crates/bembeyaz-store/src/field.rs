//! A stored field whose presence and type the store decides on: what it is, as
//! ArduinoJson's `isNull()` and `is<uint32_t>()` see it, its value when it is a `u32`, and what
//! `as<uint8_t>()` makes of it whatever it is.

use bembeyaz_jsondoc::{Value, as_number, is_integer, or_integer};

/// What a number field holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// Missing, or `null`.
    #[default]
    Missing,
    /// An integer within `u32`.
    U32,
    /// Anything else: a float, a string, a bool, a container, an integer out of range.
    Other,
}

/// A number field: its kind, its value when the kind is [`Kind::U32`] (else 0), and
/// ArduinoJson's `as<uint8_t>()` of it (a float, bool or string converted too, 0 when it cannot
/// be).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Number {
    pub kind: Kind,
    pub value: u32,
    pub byte: u8,
}

impl Number {
    /// The kind, value and `as<uint8_t>()` of the field `value` (`Kind::Missing` when it is missing
    /// or null).
    pub(crate) fn of(value: Option<&Value>) -> Number {
        let byte = as_number::<u8>(value);
        if Value::is_null(value) {
            Number {
                byte,
                ..Number::default()
            }
        } else if is_integer::<u32>(value) {
            Number {
                kind: Kind::U32,
                value: or_integer(value, 0),
                byte,
            }
        } else {
            Number {
                kind: Kind::Other,
                value: 0,
                byte,
            }
        }
    }

    /// The field read with `| fallback` for a `u8`: its value when it is an integer
    /// within `u8`, else `fallback`.
    #[must_use]
    pub fn byte_or(self, fallback: u8) -> u8 {
        match self.kind {
            Kind::U32 => u8::try_from(self.value).unwrap_or(fallback),
            _ => fallback,
        }
    }
}
