//! ArduinoJson's `serializeJson` (`Json/JsonSerializer.hpp`, `TextFormatter.hpp`).

use crate::Value;
use crate::float::decompose;
use alloc::vec::Vec;

impl Value {
    /// The JSON ArduinoJson writes for this value: no spaces, members in their order, a float
    /// with up to 6 decimal places when stored as `float` and 9 as `double` (trailing zeros
    /// dropped), `null` for a NaN or an infinity.
    #[must_use]
    pub fn to_json(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write(self, &mut out);
        out
    }
}

fn write(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::U32(n) => unsigned(u64::from(*n), out),
        Value::U64(n) => unsigned(*n, out),
        Value::I32(n) => signed(i64::from(*n), out),
        Value::I64(n) => signed(*n, out),
        Value::F32(f) => float(f64::from(*f), 6, out),
        Value::F64(f) => float(*f, 9, out),
        Value::Str(s) => string(s, out),
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write(item, out);
            }
            out.push(b']');
        }
        Value::Object(members) => {
            out.push(b'{');
            for (i, (key, item)) in members.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                string(key, out);
                out.push(b':');
                write(item, out);
            }
            out.push(b'}');
        }
    }
}

fn unsigned(n: u64, out: &mut Vec<u8>) {
    let mut digits = [0u8; 20];
    let mut at = digits.len();
    let mut n = n;
    loop {
        at -= 1;
        if let Some(d) = digits.get_mut(at) {
            *d = b'0' + u8::try_from(n % 10).unwrap_or(0);
        }
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(digits.get(at..).unwrap_or_default());
}

fn signed(n: i64, out: &mut Vec<u8>) {
    if n < 0 {
        out.push(b'-');
    }
    unsigned(n.unsigned_abs(), out);
}

/// A float as `TextFormatter::writeFloat`: `null` for a NaN or an infinity, a sign, the integral
/// digits, the decimals and an exponent.
fn float(value: f64, places: u32, out: &mut Vec<u8>) {
    if value.is_nan() || value.is_infinite() {
        out.extend_from_slice(b"null");
        return;
    }
    let value = if value < 0.0 {
        out.push(b'-');
        -value
    } else {
        value
    };
    let parts = decompose(value, places);
    unsigned(u64::from(parts.integral), out);
    if parts.decimal_places > 0 {
        out.push(b'.');
        let mut digits = [b'0'; 10];
        let mut decimal = parts.decimal;
        let width = usize::try_from(parts.decimal_places)
            .unwrap_or(0)
            .min(digits.len());
        for d in digits.iter_mut().take(width).rev() {
            *d = b'0' + u8::try_from(decimal % 10).unwrap_or(0);
            decimal /= 10;
        }
        out.extend_from_slice(digits.get(..width).unwrap_or_default());
    }
    if parts.exponent != 0 {
        out.push(b'e');
        signed(i64::from(parts.exponent), out);
    }
}

/// A string as `TextFormatter::writeString`: `"` and `\` and five controls escaped, a NUL as
/// `\u0000`, every other byte as it is.
fn string(s: &[u8], out: &mut Vec<u8>) {
    out.push(b'"');
    for &c in s {
        let escape = match c {
            b'"' => Some(b'"'),
            b'\\' => Some(b'\\'),
            0x08 => Some(b'b'),
            0x0C => Some(b'f'),
            b'\n' => Some(b'n'),
            b'\r' => Some(b'r'),
            b'\t' => Some(b't'),
            _ => None,
        };
        match (escape, c) {
            (Some(e), _) => out.extend_from_slice(&[b'\\', e]),
            (None, 0) => out.extend_from_slice(b"\\u0000"),
            (None, c) => out.push(c),
        }
    }
    out.push(b'"');
}
