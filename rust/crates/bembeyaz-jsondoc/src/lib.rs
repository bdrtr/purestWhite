//! JSON documents read and written as ArduinoJson 7.4.2 reads and writes them, with the firmware's
//! configuration (64-bit integers and doubles on, comments, `NaN` and `Infinity` off, a nesting
//! limit of 10, strings up to 65,535 bytes): the C++ firmware keeps its settings and talks to its
//! servers through ArduinoJson, so what it accepts, refuses and writes is the contract a port keeps.
//!
//! - [`parse`]: `deserializeJson` over a C string (it ends at the first NUL), with ArduinoJson's
//!   leniencies (single quotes, unquoted keys, a duplicate key's value replacing the first in its
//!   place, anything after the root ignored unless the root is a number, numbers read by its own
//!   approximate decimal conversion).
//! - [`Value::to_json`]: `serializeJson`, floats written with up to 6 decimal places (a value
//!   stored as `float`) or 9 (as `double`) by ArduinoJson's own decomposition.
//! - The `as_*`, `is_*` and `or_*` functions (on a value or a missing member) and
//!   [`Value::is_null`]: ArduinoJson's `as<T>()`, `is<T>()`, `variant | fallback` and `isNull()`;
//!   [`items`] and [`members`]: a `JsonArrayConst` or `JsonObjectConst` of a value.
//! - [`Writer`]: the same text as [`Value::to_json`], written value by value into one buffer
//!   with no document first; a failed allocation makes [`Writer::finish`] return `None` instead
//!   of aborting.
//!
//! The golden corpora are ArduinoJson's own output on seeded scenarios: testdata/kosync-corpus.txt
//! (`test/kosync_messages/make_corpus.py`) and testdata/conversions-corpus.txt
//! (`test/json_conversions/make_corpus.py`, the conversions and `|` the settings stores use).
//!
//! The input is untrusted (a sync server's answer, a book's sidecar metadata): reading never panics, and an allocation that
//! fails is [`Error::NoMemory`], as in ArduinoJson, not an abort. Writing allocates as `Vec` does.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

#[cfg(test)]
mod conversions_tests;
mod convert;
mod float;
mod number;
mod parse;
#[cfg(test)]
mod tests;
mod write;
mod writer;

use alloc::vec::Vec;

/// The longest string or key ArduinoJson keeps (`ARDUINOJSON_STRING_LENGTH_SIZE` 2).
pub(crate) const STRING_MOST: usize = 65_535;

pub use convert::{
    Integer, Target, as_number, as_str, as_string, is_i64, is_integer, is_number, items, members,
    or_bool, or_f32, or_integer, or_str,
};
pub use parse::{Error, parse};
pub use writer::Writer;

/// A value as ArduinoJson stores it: integers in 32 bits when they fit, floats as `float` when
/// that loses nothing, strings and keys as bytes (they may hold a NUL).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    F32(f32),
    F64(f64),
    Str(Vec<u8>),
    Array(Vec<Value>),
    /// Members in the order they came; keys unique (a repeated key replaced the value in place).
    Object(Vec<(Vec<u8>, Value)>),
}

impl Value {
    /// An unsigned integer, as ArduinoJson's `setInteger` stores one.
    #[must_use]
    pub fn unsigned(value: u64) -> Value {
        u32::try_from(value).map_or(Value::U64(value), Value::U32)
    }

    /// A signed integer, as ArduinoJson's `setInteger` stores one.
    #[must_use]
    pub fn signed(value: i64) -> Value {
        i32::try_from(value).map_or(Value::I64(value), Value::I32)
    }

    /// A double, as ArduinoJson's `setFloat(double)` stores one: as a `float` when that loses
    /// nothing.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::float_cmp)]
    pub fn double(value: f64) -> Value {
        let narrow = value as f32;
        if f64::from(narrow) == value {
            Value::F32(narrow)
        } else {
            Value::F64(value)
        }
    }

    /// A string of `bytes`; a null past 65,535 bytes, as ArduinoJson cannot store it.
    #[must_use]
    pub fn string(bytes: &[u8]) -> Value {
        if bytes.len() > STRING_MOST {
            return Value::Null;
        }
        Value::Str(bytes.to_vec())
    }

    /// The member `key` of an object (the first so named); none for another value or a missing
    /// key, as `doc[key]` gives a null.
    #[must_use]
    pub fn get(&self, key: &[u8]) -> Option<&Value> {
        match self {
            Value::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Sets the member `key` of an object (in place when it is there, else at the end), as
    /// `obj[key] = value` does; nothing for another value.
    pub fn set(&mut self, key: &[u8], value: Value) {
        let Value::Object(members) = self else {
            return;
        };
        match members.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => members.push((key.to_vec(), value)),
        }
    }

    /// Whether this is a null, or a missing member (`isNull()`).
    #[must_use]
    pub fn is_null(value: Option<&Value>) -> bool {
        matches!(value, None | Some(Value::Null))
    }
}
