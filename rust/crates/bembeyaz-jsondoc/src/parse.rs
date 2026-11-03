//! ArduinoJson's `deserializeJson` (`Json/JsonDeserializer.hpp`) over a C string.

use crate::number::{Number, parse_number};
use crate::{STRING_MOST, Value};
use alloc::vec::Vec;

/// Why a document was not read (`DeserializationError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Nothing but spaces.
    EmptyInput,
    /// The input ended inside a value.
    IncompleteInput,
    InvalidInput,
    /// A string or key longer than 65,535 bytes, or an allocation that failed.
    NoMemory,
    /// Nesting deeper than 10.
    TooDeep,
}

impl Error {
    /// ArduinoJson's name for it (`DeserializationError::c_str`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Error::EmptyInput => "EmptyInput",
            Error::IncompleteInput => "IncompleteInput",
            Error::InvalidInput => "InvalidInput",
            Error::NoMemory => "NoMemory",
            Error::TooDeep => "TooDeep",
        }
    }
}

/// Containers ArduinoJson opens at most (`ARDUINOJSON_DEFAULT_NESTING_LIMIT`).
const NESTING: u8 = 10;
/// The longest number token it reads; the rest of it is left in the input.
const NUMBER_MOST: usize = 63;

/// `input` read as a JSON document, as ArduinoJson reads a C string: it ends at the first NUL,
/// and what follows the root value is ignored, unless the root is a number: then anything after
/// it is an invalid input.
///
/// # Errors
/// As ArduinoJson's `deserializeJson`.
pub fn parse(input: &[u8]) -> Result<Value, Error> {
    let input = input.split(|b| *b == 0).next().unwrap_or_default();
    let mut reader = Reader {
        input,
        at: 0,
        found_something: false,
    };
    let value = reader.value(NESTING)?;
    // ArduinoJson's `isFloat()` here is any number: a number root must end the input.
    if reader.current() != 0 && crate::is_number(Some(&value)) {
        return Err(Error::InvalidInput);
    }
    Ok(value)
}

struct Reader<'a> {
    input: &'a [u8],
    at: usize,
    /// Something other than spaces was seen: an end after it is an incomplete input.
    found_something: bool,
}

impl Reader<'_> {
    /// The byte at the reading point; 0 at the end.
    fn current(&self) -> u8 {
        self.input.get(self.at).copied().unwrap_or(0)
    }

    fn advance(&mut self) {
        self.at = (self.at + 1).min(self.input.len());
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.current() != c {
            return false;
        }
        self.advance();
        true
    }

    fn skip_spaces(&mut self) -> Result<(), Error> {
        loop {
            match self.current() {
                0 if self.found_something => return Err(Error::IncompleteInput),
                0 => return Err(Error::EmptyInput),
                b' ' | b'\t' | b'\r' | b'\n' => self.advance(),
                _ => {
                    self.found_something = true;
                    return Ok(());
                }
            }
        }
    }

    /// A value, `nesting` more containers allowed inside it and itself.
    fn value(&mut self, nesting: u8) -> Result<Value, Error> {
        self.skip_spaces()?;
        match self.current() {
            b'[' => self.array(nesting),
            b'{' => self.object(nesting),
            b'"' | b'\'' => self.string().map(Value::Str),
            b't' => self.keyword(b"true").map(|()| Value::Bool(true)),
            b'f' => self.keyword(b"false").map(|()| Value::Bool(false)),
            b'n' => self.keyword(b"null").map(|()| Value::Null),
            _ => self.number(),
        }
    }

    fn array(&mut self, nesting: u8) -> Result<Value, Error> {
        let inner = nesting.checked_sub(1).ok_or(Error::TooDeep)?;
        self.advance();
        self.skip_spaces()?;
        let mut items = Vec::new();
        if self.eat(b']') {
            return Ok(Value::Array(items));
        }
        loop {
            let item = self.value(inner)?;
            items.try_reserve(1).map_err(|_| Error::NoMemory)?;
            items.push(item);
            self.skip_spaces()?;
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            if !self.eat(b',') {
                return Err(Error::InvalidInput);
            }
        }
    }

    fn object(&mut self, nesting: u8) -> Result<Value, Error> {
        let inner = nesting.checked_sub(1).ok_or(Error::TooDeep)?;
        self.advance();
        self.skip_spaces()?;
        let mut members = Vec::new();
        if self.eat(b'}') {
            return Ok(Value::Object(members));
        }
        loop {
            let key = self.key()?;
            self.skip_spaces()?;
            if !self.eat(b':') {
                return Err(Error::InvalidInput);
            }
            let value = self.value(inner)?;
            set(&mut members, key, value)?;
            self.skip_spaces()?;
            if self.eat(b'}') {
                return Ok(Value::Object(members));
            }
            if !self.eat(b',') {
                return Err(Error::InvalidInput);
            }
            self.skip_spaces()?;
        }
    }

    fn key(&mut self) -> Result<Vec<u8>, Error> {
        if matches!(self.current(), b'"' | b'\'') {
            return self.string();
        }
        let start = self.at;
        while is_unquoted(self.current()) {
            self.advance();
        }
        let key = self.input.get(start..self.at).unwrap_or_default();
        if key.is_empty() {
            return Err(Error::InvalidInput);
        }
        if key.len() > STRING_MOST {
            return Err(Error::NoMemory);
        }
        let mut out = Vec::new();
        keep(&mut out, key)?;
        Ok(out)
    }

    fn keyword(&mut self, word: &[u8]) -> Result<(), Error> {
        for &c in word {
            match self.current() {
                0 => return Err(Error::IncompleteInput),
                got if got != c => return Err(Error::InvalidInput),
                _ => self.advance(),
            }
        }
        Ok(())
    }

    /// A number token: the bytes a number may hold, 63 at most, read by `parse_number`.
    fn number(&mut self) -> Result<Value, Error> {
        let start = self.at;
        while self.at - start < NUMBER_MOST && is_number(self.current()) {
            self.advance();
        }
        let token = self.input.get(start..self.at).unwrap_or_default();
        match parse_number(token) {
            Number::Unsigned(n) => Ok(Value::unsigned(n)),
            Number::Signed(n) => Ok(Value::signed(n)),
            Number::F32(f) => Ok(Value::F32(f)),
            Number::F64(f) => Ok(Value::double(f)),
            Number::Invalid => Err(Error::InvalidInput),
        }
    }
}

/// A member of an object being read: a repeated key keeps its place, its value is the last one.
fn set(members: &mut Vec<(Vec<u8>, Value)>, key: Vec<u8>, value: Value) -> Result<(), Error> {
    if let Some((_, v)) = members.iter_mut().find(|(k, _)| *k == key) {
        *v = value;
        return Ok(());
    }
    members.try_reserve(1).map_err(|_| Error::NoMemory)?;
    members.push((key, value));
    Ok(())
}

/// `bytes` added to a string being read, while it is within ArduinoJson's limit: past it the
/// string is refused once it ends, so the rest is only scanned.
fn keep(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), Error> {
    if out.len() > STRING_MOST {
        return Ok(());
    }
    out.try_reserve(bytes.len()).map_err(|_| Error::NoMemory)?;
    out.extend_from_slice(bytes);
    Ok(())
}

/// A byte of an unquoted key: a digit, a letter, or one of `_` to `z`.
fn is_unquoted(c: u8) -> bool {
    c.is_ascii_digit() || (b'_'..=b'z').contains(&c) || c.is_ascii_uppercase()
}

/// A byte a number token may hold.
fn is_number(c: u8) -> bool {
    c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.' | b'e' | b'E')
}

impl Reader<'_> {
    /// A quoted string (in `"` or `'`), its escapes decoded.
    fn string(&mut self) -> Result<Vec<u8>, Error> {
        let stop = self.current();
        self.advance();
        let mut out = Vec::new();
        let mut high: u16 = 0;
        loop {
            let c = self.current();
            self.advance();
            match c {
                c if c == stop => break,
                0 => return Err(Error::IncompleteInput),
                b'\\' => self.escape(&mut out, &mut high)?,
                c => keep(&mut out, &[c])?,
            }
        }
        if out.len() > STRING_MOST {
            return Err(Error::NoMemory);
        }
        Ok(out)
    }

    /// The escape after a `\`.
    fn escape(&mut self, out: &mut Vec<u8>, high: &mut u16) -> Result<(), Error> {
        let c = self.current();
        if c == 0 {
            return Err(Error::IncompleteInput);
        }
        if c == b'u' {
            self.advance();
            let unit = self.hex4()?;
            if let Some(codepoint) = codepoint(high, unit) {
                let (bytes, n) = encode_utf8(codepoint);
                keep(out, bytes.get(..n).unwrap_or_default())?;
            }
            return Ok(());
        }
        let plain = match c {
            b'"' | b'\'' | b'\\' | b'/' => c,
            b'b' => 0x08,
            b'f' => 0x0C,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            _ => return Err(Error::InvalidInput),
        };
        self.advance();
        keep(out, &[plain])
    }

    fn hex4(&mut self) -> Result<u16, Error> {
        let mut unit: u16 = 0;
        for _ in 0..4 {
            let c = self.current();
            if c == 0 {
                return Err(Error::IncompleteInput);
            }
            // ArduinoJson's decodeHex: below 'A' a digit, else a letter of either case.
            let value = if c < b'A' {
                c.wrapping_sub(b'0')
            } else {
                (c & !0x20).wrapping_sub(b'A').wrapping_add(10)
            };
            if value > 0x0F {
                return Err(Error::InvalidInput);
            }
            unit = (unit << 4) | u16::from(value);
            self.advance();
        }
        Ok(unit)
    }
}

/// A UTF-16 unit taken as ArduinoJson's `Utf16::Codepoint` takes it: a high surrogate waits (and
/// yields nothing), a low one joins whatever high one came last.
fn codepoint(high: &mut u16, unit: u16) -> Option<u32> {
    if (0xD800..0xDC00).contains(&unit) {
        *high = unit & 0x3FF;
        return None;
    }
    if (0xDC00..0xE000).contains(&unit) {
        return Some(0x10000 + ((u32::from(*high) << 10) | u32::from(unit & 0x3FF)));
    }
    Some(u32::from(unit))
}

/// `codepoint` in UTF-8, as ArduinoJson's `Utf8::encodeCodepoint`: the bytes and how many.
#[allow(
    clippy::cast_possible_truncation,
    reason = "each byte is masked to fit"
)]
fn encode_utf8(codepoint: u32) -> ([u8; 4], usize) {
    if codepoint < 0x80 {
        return ([codepoint as u8, 0, 0, 0], 1);
    }
    let mut bytes = [0u8; 4];
    let mut n = 0;
    let mut push = |b: u8| {
        if let Some(slot) = bytes.get_mut(n) {
            *slot = b;
            n += 1;
        }
    };
    push(((codepoint | 0x80) & 0xBF) as u8);
    let mut rest = (codepoint >> 6) as u16;
    if rest < 0x20 {
        push((rest | 0xC0) as u8);
    } else {
        push(((rest | 0x80) & 0xBF) as u8);
        rest >>= 6;
        if rest < 0x10 {
            push((rest | 0xE0) as u8);
        } else {
            push(((rest | 0x80) & 0xBF) as u8);
            rest >>= 6;
            push((rest | 0xF0) as u8);
        }
    }
    let mut ordered = [0u8; 4];
    for (slot, b) in ordered.iter_mut().zip(bytes.iter().take(n).rev()) {
        *slot = *b;
    }
    (ordered, n)
}
