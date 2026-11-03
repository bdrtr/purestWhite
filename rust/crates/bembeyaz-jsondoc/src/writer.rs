//! A JSON writer that builds the text straight into one buffer, as `serializeJson` of a document
//! would write it (the same escapes, numbers and floats as [`Value::to_json`](crate::Value)),
//! without a document in memory first, and that reports a failed allocation instead of aborting.

use crate::STRING_MOST;
use crate::write::{float, signed, string, string_len, unsigned};
use alloc::vec::Vec;

/// The most bytes one number or float takes.
const NUMBER_MOST: usize = 48;

/// JSON text written value by value: a container's items go inside the closure given to
/// [`Writer::object`] or [`Writer::array`], a member's value right after its [`Writer::key`].
pub struct Writer {
    out: Vec<u8>,
    failed: bool,
    /// The container being written has a value already: the next one takes a comma.
    comma: bool,
    /// A key was written: the next value belongs to it and takes no comma.
    after_key: bool,
}

impl Writer {
    /// A writer whose buffer starts with room for `capacity` bytes.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Writer {
        let mut out = Vec::new();
        let failed = out.try_reserve(capacity).is_err();
        Writer {
            out,
            failed,
            comma: false,
            after_key: false,
        }
    }

    /// The text written; none when an allocation failed.
    #[must_use]
    pub fn finish(self) -> Option<Vec<u8>> {
        (!self.failed).then_some(self.out)
    }

    /// Room for `more` bytes, or the writer marked failed.
    fn room(&mut self, more: usize) -> bool {
        if !self.failed && self.out.try_reserve(more).is_err() {
            self.failed = true;
        }
        !self.failed
    }

    /// The comma before a value, unless it follows its key or opens its container.
    fn separate(&mut self) {
        if self.after_key {
            self.after_key = false;
            return;
        }
        if self.comma && self.room(1) {
            self.out.push(b',');
        }
        self.comma = true;
    }

    fn container(&mut self, open: u8, close: u8, body: impl FnOnce(&mut Writer)) {
        self.separate();
        if self.room(1) {
            self.out.push(open);
        }
        let outer = self.comma;
        self.comma = false;
        body(self);
        self.comma = outer;
        self.after_key = false;
        if self.room(1) {
            self.out.push(close);
        }
    }

    /// An object, its members written by `body`.
    pub fn object(&mut self, body: impl FnOnce(&mut Writer)) {
        self.container(b'{', b'}', body);
    }

    /// An array, its items written by `body`.
    pub fn array(&mut self, body: impl FnOnce(&mut Writer)) {
        self.container(b'[', b']', body);
    }

    /// A member's key; its value is the next one written.
    pub fn key(&mut self, key: &[u8]) {
        self.separate();
        if self.room(string_len(key) + 1) {
            string(key, &mut self.out);
            self.out.push(b':');
        }
        self.after_key = true;
    }

    /// A string; `null` past 65,535 bytes, as ArduinoJson cannot store it.
    pub fn string(&mut self, text: &[u8]) {
        if text.len() > STRING_MOST {
            self.null();
            return;
        }
        self.separate();
        if self.room(string_len(text)) {
            string(text, &mut self.out);
        }
    }

    pub fn unsigned(&mut self, n: u64) {
        self.separate();
        if self.room(NUMBER_MOST) {
            unsigned(n, &mut self.out);
        }
    }

    pub fn signed(&mut self, n: i64) {
        self.separate();
        if self.room(NUMBER_MOST) {
            signed(n, &mut self.out);
        }
    }

    /// A `float` (6 decimal places at most, as ArduinoJson writes one).
    pub fn f32(&mut self, value: f32) {
        self.separate();
        if self.room(NUMBER_MOST) {
            float(f64::from(value), 6, &mut self.out);
        }
    }

    pub fn bool(&mut self, value: bool) {
        self.separate();
        let text: &[u8] = if value { b"true" } else { b"false" };
        if self.room(text.len()) {
            self.out.extend_from_slice(text);
        }
    }

    pub fn null(&mut self) {
        self.separate();
        if self.room(4) {
            self.out.extend_from_slice(b"null");
        }
    }
}
