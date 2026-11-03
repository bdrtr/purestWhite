//! A stored field whose presence and type the C++ store decides on: what it is, as
//! ArduinoJson's `isNull()` and `is<uint32_t>()` see it, and its value when it is a `u32`.

use bembeyaz_jsondoc::{Value, is_integer, or_integer};

/// What a number field holds; its value is store.h's code for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// Missing, or `null`.
    #[default]
    Missing = 0,
    /// An integer within `u32`.
    U32 = 1,
    /// Anything else: a float, a string, a bool, a container, an integer out of range.
    Other = 2,
}

/// A number field: its kind, and its value when the kind is [`Kind::U32`] (else 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Number {
    pub kind: Kind,
    pub value: u32,
}

impl Number {
    /// The kind and value of the field `value` (`Kind::Missing` when it is missing or null).
    pub(crate) fn of(value: Option<&Value>) -> Number {
        if Value::is_null(value) {
            Number::default()
        } else if is_integer::<u32>(value) {
            Number {
                kind: Kind::U32,
                value: or_integer(value, 0),
            }
        } else {
            Number {
                kind: Kind::Other,
                value: 0,
            }
        }
    }
}
