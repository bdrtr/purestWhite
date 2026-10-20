//! A streaming JSON reader, as the `FreeInk` SDK's `StreamingJsonParser` read JSON: bytes go in
//! as they arrive, and each key, string, number, literal and container edge comes out as an
//! event. Strings are decoded (escapes, `\uXXXX` with surrogate pairs) into a bounded buffer; a
//! token longer than the buffer is dropped, as the SDK did, unless
//! [`Parser::truncate_long_strings`] asked for string values to be cut instead. The reader is a
//! lenient scanner, not a validator: what it does not expect it skips (an unknown escape is kept
//! as written), and it stops only on a broken `\u` escape or literal, or on nesting deeper than
//! [`MAX_NESTING`].
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use core::mem::MaybeUninit;

/// The buffer for a decoded key, string or number, NUL included: at most 2047 bytes of text. A
/// longer token is dropped, unless [`Parser::truncate_long_strings`] is on, which cuts a string
/// value to fit. The SDK's size. `BB_JSON_TOKEN_BUF_SIZE` in json.h.
pub const TOKEN_BUF_SIZE: usize = 2048;
/// Open objects and arrays allowed at once. `BB_JSON_MAX_NESTING` in json.h.
pub const MAX_NESTING: usize = 32;

/// A decoded key, string or number. It is followed by a NUL in memory, for C callers.
#[derive(Clone, Copy)]
pub struct Token<'a>(&'a [u8]);

impl<'a> Token<'a> {
    /// The bytes, without the NUL.
    #[must_use]
    pub fn bytes(&self) -> &'a [u8] {
        self.0.split_last().map_or(&[], |(_, bytes)| bytes)
    }

    /// The bytes and the NUL after them.
    #[must_use]
    pub fn with_nul(&self) -> &'a [u8] {
        self.0
    }
}

/// What the reader found, in document order.
pub trait Events {
    fn key(&mut self, _key: Token<'_>) {}
    fn string(&mut self, _value: Token<'_>) {}
    fn number(&mut self, _value: Token<'_>) {}
    fn boolean(&mut self, _value: bool) {}
    fn null(&mut self) {}
    fn object_start(&mut self) {}
    fn object_end(&mut self) {}
    fn array_start(&mut self) {}
    fn array_end(&mut self) {}
}

// Every enum has a 0 variant: all-zero bytes are the start state (see `init`).
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum State {
    Scanning = 0,
    Key,
    Value,
    Number,
    Literal,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Container {
    Object = 0,
    Array,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Literal {
    True = 0,
    False,
    Null,
}

impl Literal {
    fn text(self) -> &'static [u8] {
        match self {
            Literal::True => b"true",
            Literal::False => b"false",
            Literal::Null => b"null",
        }
    }
}

#[allow(clippy::struct_excessive_bools, reason = "independent flags")]
pub struct Parser {
    token: [u8; TOKEN_BUF_SIZE],
    token_len: u16,
    state: State,
    expecting_value: bool,
    escaped: bool,
    token_overflow: bool,
    error: bool,
    stack: [Container; MAX_NESTING],
    depth: u8,
    literal: Literal,
    literal_pos: u8,
    // A \uXXXX escape being read, and the high half of a surrogate pair waiting for its low half.
    unicode_digits_left: u8,
    unicode_value: u16,
    pending_high_surrogate: u16,
    /// A string value longer than the buffer is reported cut, not dropped (kept by `reset`).
    truncate_long_strings: bool,
}

// All-zero bytes are the start state: `mem::zeroed` fails to compile here once a field has no
// valid zero. Storage is dropped without a call, so nothing in it may need dropping.
const _: Parser = unsafe { core::mem::zeroed() };
const _: () = assert!(!core::mem::needs_drop::<Parser>());
const _: () = assert!(TOKEN_BUF_SIZE <= u16::MAX as usize && MAX_NESTING <= u8::MAX as usize);

impl Parser {
    /// Sets up a reader in `slot`, zeroing it in place so its 2 KB never pass through a stack
    /// frame.
    pub fn init(slot: &mut MaybeUninit<Self>) -> &mut Self {
        // SAFETY: all-zero bytes are a valid Parser, checked above.
        unsafe {
            slot.as_mut_ptr().write_bytes(0, 1);
            slot.assume_init_mut()
        }
    }

    /// Reports a string value longer than the buffer by its first bytes (at most
    /// `TOKEN_BUF_SIZE - 1`, ending on a whole UTF-8 character) instead of dropping it. Keys and
    /// numbers that long are still dropped: a cut one would name or mean something else.
    pub fn truncate_long_strings(&mut self, truncate: bool) {
        self.truncate_long_strings = truncate;
    }

    /// Starts a new document.
    pub fn reset(&mut self) {
        self.token_len = 0;
        self.state = State::Scanning;
        self.expecting_value = false;
        self.escaped = false;
        self.token_overflow = false;
        self.error = false;
        self.depth = 0;
        self.literal = Literal::True;
        self.literal_pos = 0;
        self.unicode_digits_left = 0;
        self.unicode_value = 0;
        self.pending_high_surrogate = 0;
    }

    /// Whether the document stopped being read (a broken escape or literal, or nesting too
    /// deep). Later bytes are ignored until `reset`.
    #[must_use]
    pub fn has_error(&self) -> bool {
        self.error
    }

    /// Reads the next bytes of the document.
    pub fn feed(&mut self, data: &[u8], events: &mut impl Events) {
        for &c in data {
            if self.error {
                break;
            }
            match self.state {
                State::Scanning => self.scanning(c, events),
                State::Key | State::Value => self.string_byte(c, events),
                State::Number => self.number_byte(c, events),
                State::Literal => self.literal_byte(c, events),
            }
        }
    }

    fn in_array(&self) -> bool {
        usize::from(self.depth)
            .checked_sub(1)
            .and_then(|top| self.stack.get(top))
            .is_some_and(|&container| container == Container::Array)
    }

    fn value_expected(&self) -> bool {
        self.expecting_value || self.in_array()
    }

    fn open(&mut self, container: Container) -> bool {
        if let Some(slot) = self.stack.get_mut(usize::from(self.depth)) {
            *slot = container;
            self.depth += 1;
            true
        } else {
            self.error = true;
            false
        }
    }

    fn close(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        self.expecting_value = false;
    }

    fn start_token(&mut self) {
        self.token_len = 0;
        self.token_overflow = false;
    }

    fn start_literal(&mut self, literal: Literal) {
        if self.value_expected() {
            self.literal = literal;
            self.literal_pos = 1;
            self.state = State::Literal;
        }
    }

    fn scanning(&mut self, c: u8, events: &mut impl Events) {
        match c {
            b'"' => {
                self.start_token();
                self.state = if self.value_expected() {
                    State::Value
                } else {
                    State::Key
                };
            }
            b'{' => {
                if !self.open(Container::Object) {
                    return;
                }
                events.object_start();
                self.expecting_value = false;
            }
            b'}' => {
                events.object_end();
                self.close();
            }
            b'[' => {
                if !self.open(Container::Array) {
                    return;
                }
                events.array_start();
                self.expecting_value = false;
            }
            b']' => {
                events.array_end();
                self.close();
            }
            b':' => self.expecting_value = true,
            b',' => self.expecting_value = false,
            b't' => self.start_literal(Literal::True),
            b'f' => self.start_literal(Literal::False),
            b'n' => self.start_literal(Literal::Null),
            b'-' | b'0'..=b'9' if self.value_expected() => {
                self.start_token();
                self.push(c);
                self.state = State::Number;
            }
            _ => {}
        }
    }

    fn string_byte(&mut self, c: u8, events: &mut impl Events) {
        if self.unicode_digits_left > 0 {
            // A \u escape takes exactly four hex digits (RFC 8259); anything else, a closing
            // quote included, breaks the document.
            let Some(digit) = (c as char).to_digit(16) else {
                self.error = true;
                return;
            };
            #[allow(clippy::cast_possible_truncation)] // a hex digit
            let digit = digit as u16;
            self.unicode_value = (self.unicode_value << 4) | digit;
            self.unicode_digits_left -= 1;
            if self.unicode_digits_left == 0 {
                self.finish_unicode_escape();
            }
            return;
        }

        if self.escaped {
            self.escaped = false;
            if c != b'u' {
                self.flush_pending_surrogate();
            }
            match c {
                b'"' | b'\\' | b'/' => self.push(c),
                b'b' => self.push(0x08),
                b'f' => self.push(0x0C),
                b'n' => self.push(b'\n'),
                b'r' => self.push(b'\r'),
                b't' => self.push(b'\t'),
                b'u' => {
                    self.unicode_digits_left = 4;
                    self.unicode_value = 0;
                }
                _ => {
                    self.push(b'\\');
                    self.push(c);
                }
            }
            return;
        }

        if c == b'\\' {
            // A pending high surrogate stays: this may be the "\u" of its low half.
            self.escaped = true;
            return;
        }

        self.flush_pending_surrogate();
        if c == b'"' {
            self.end_string(events);
        } else {
            self.push(c);
        }
    }

    fn finish_unicode_escape(&mut self) {
        let unit = u32::from(self.unicode_value);
        if self.pending_high_surrogate != 0 {
            if (0xDC00..=0xDFFF).contains(&unit) {
                let high = u32::from(self.pending_high_surrogate);
                self.pending_high_surrogate = 0;
                self.push_utf8(0x10000 + ((high - 0xD800) << 10) + (unit - 0xDC00));
                return;
            }
            self.flush_pending_surrogate();
        }
        match unit {
            0xD800..=0xDBFF => self.pending_high_surrogate = self.unicode_value,
            0xDC00..=0xDFFF => self.push_utf8(0xFFFD), // a lone low surrogate
            _ => self.push_utf8(unit),
        }
    }

    fn flush_pending_surrogate(&mut self) {
        if self.pending_high_surrogate != 0 {
            self.pending_high_surrogate = 0;
            self.push_utf8(0xFFFD);
        }
    }

    #[allow(clippy::cast_possible_truncation)] // each byte is masked or shifted below 0x100
    fn push_utf8(&mut self, codepoint: u32) {
        if codepoint < 0x80 {
            self.push(codepoint as u8);
        } else if codepoint < 0x800 {
            self.push(0xC0 | (codepoint >> 6) as u8);
            self.push(0x80 | (codepoint & 0x3F) as u8);
        } else if codepoint < 0x1_0000 {
            self.push(0xE0 | (codepoint >> 12) as u8);
            self.push(0x80 | ((codepoint >> 6) & 0x3F) as u8);
            self.push(0x80 | (codepoint & 0x3F) as u8);
        } else {
            self.push(0xF0 | (codepoint >> 18) as u8);
            self.push(0x80 | ((codepoint >> 12) & 0x3F) as u8);
            self.push(0x80 | ((codepoint >> 6) & 0x3F) as u8);
            self.push(0x80 | (codepoint & 0x3F) as u8);
        }
    }

    fn number_byte(&mut self, c: u8, events: &mut impl Events) {
        if matches!(c, b'0'..=b'9' | b'.' | b'-' | b'+' | b'e' | b'E') {
            self.push(c);
            return;
        }
        if let Some(token) = self.token() {
            events.number(token);
        }
        self.state = State::Scanning;
        self.expecting_value = false;
        self.scanning(c, events);
    }

    fn literal_byte(&mut self, c: u8, events: &mut impl Events) {
        let text = self.literal.text();
        if text.get(usize::from(self.literal_pos)) != Some(&c) {
            self.error = true;
            return;
        }
        self.literal_pos += 1;
        if usize::from(self.literal_pos) == text.len() {
            match self.literal {
                Literal::True => events.boolean(true),
                Literal::False => events.boolean(false),
                Literal::Null => events.null(),
            }
            self.state = State::Scanning;
            self.expecting_value = false;
        }
    }

    fn end_string(&mut self, events: &mut impl Events) {
        let key = self.state == State::Key;
        if !key && self.token_overflow && self.truncate_long_strings {
            // The buffer holds the first bytes; a character cut at its end goes.
            let kept = self
                .token
                .get(..usize::from(self.token_len))
                .map_or(0, bembeyaz_utf8::safe_truncate_len);
            #[allow(clippy::cast_possible_truncation)] // at most the buffer's length
            {
                self.token_len = kept as u16;
            }
            self.token_overflow = false;
        }
        if let Some(token) = self.token() {
            if key {
                events.key(token);
            } else {
                events.string(token);
            }
        }
        self.state = State::Scanning;
        if !key {
            self.expecting_value = false;
        }
    }

    fn push(&mut self, c: u8) {
        let len = usize::from(self.token_len);
        match self.token.get_mut(len) {
            Some(slot) if len < TOKEN_BUF_SIZE - 1 => {
                *slot = c;
                self.token_len += 1;
            }
            _ => self.token_overflow = true,
        }
    }

    /// The token just read with a NUL after it, or `None` when it overflowed the buffer.
    fn token(&mut self) -> Option<Token<'_>> {
        if self.token_overflow {
            return None;
        }
        let len = usize::from(self.token_len);
        *self.token.get_mut(len)? = 0;
        self.token.get(..=len).map(Token)
    }
}

#[cfg(test)]
mod tests;
