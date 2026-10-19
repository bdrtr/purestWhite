//! Reads a stylesheet in chunks: comments, at-rules (skipped with their blocks), selectors and
//! declaration blocks, which go to the rule store as each block closes.

use crate::ascii::is_space;
use crate::declaration::apply_declaration;
use crate::rules::Rules;
use crate::style::Style;
use core::mem::MaybeUninit;

/// Longest selector group or declaration read: a longer selector skips its whole rule, and a
/// longer declaration is not applied.
const BUFFER: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parse {
    Complete,
    /// Something was cut: a store cap, a buffer, or the text ended inside a rule or comment.
    Partial,
}

#[repr(C)]
struct Buffer {
    bytes: [u8; BUFFER],
    len: u16,
}

impl Buffer {
    fn push(&mut self, b: u8) -> bool {
        let Some(slot) = self.bytes.get_mut(usize::from(self.len)) else {
            return false;
        };
        *slot = b;
        self.len += 1;
        true
    }

    fn as_slice(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or_default()
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn clear(&mut self) {
        self.len = 0;
    }
}

/// The state between chunks of one stylesheet, about 2.2 KB: callers keep it outside the stack
/// frames of this crate and set it up with [`Tokenizer::init`].
#[repr(C)]
pub struct Tokenizer {
    selector: Buffer,
    declaration: Buffer,
    style: Style,
    at_depth: u32,
    body_depth: u32,
    in_comment: bool,
    maybe_slash: bool,
    prev_star: bool,
    in_at_rule: bool,
    skipping_rule: bool,
    selector_truncated: bool,
    declaration_truncated: bool,
    input_truncated: bool,
}

// All-zero bytes are the start state: `mem::zeroed` fails to compile here once a field has no
// valid zero (a reference, a `NonZero`, an enum without a 0 variant). Storage is dropped
// without a call, so nothing in it may need dropping.
const _: Tokenizer = unsafe { core::mem::zeroed() };
const _: () = assert!(!core::mem::needs_drop::<Tokenizer>());

impl Tokenizer {
    /// Starts a stylesheet in `slot`, zeroing it in place so the 2.2 KB never pass through a
    /// stack frame.
    pub fn init(slot: &mut MaybeUninit<Self>) -> &mut Self {
        // SAFETY: all-zero bytes are a valid Tokenizer, checked above.
        unsafe {
            slot.as_mut_ptr().write_bytes(0, 1);
            slot.assume_init_mut()
        }
    }

    /// Reads the next bytes of the stylesheet.
    pub fn feed(&mut self, rules: &mut Rules, bytes: &[u8]) {
        for &c in bytes {
            if self.in_comment {
                if self.prev_star && c == b'/' {
                    self.in_comment = false;
                    self.prev_star = false;
                } else {
                    self.prev_star = c == b'*';
                }
                continue;
            }
            if self.maybe_slash {
                self.maybe_slash = false;
                if c == b'*' {
                    self.in_comment = true;
                    self.prev_star = false;
                    continue;
                }
                self.byte(rules, b'/');
            }
            if c == b'/' {
                self.maybe_slash = true;
            } else {
                self.byte(rules, c);
            }
        }
    }

    /// Ends the stylesheet.
    pub fn finish(&mut self, rules: &mut Rules) -> Parse {
        if self.maybe_slash {
            self.maybe_slash = false;
            self.byte(rules, b'/');
        }
        let incomplete =
            self.body_depth > 0 || self.in_at_rule || self.in_comment || !self.selector.is_empty();
        if rules.stopped().is_some() || self.input_truncated || incomplete {
            Parse::Partial
        } else {
            Parse::Complete
        }
    }

    /// True when a selector or declaration was longer than the buffer.
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.input_truncated
    }

    fn byte(&mut self, rules: &mut Rules, c: u8) {
        if self.in_at_rule {
            match c {
                b'{' => self.at_depth = self.at_depth.saturating_add(1),
                b'}' => {
                    self.at_depth = self.at_depth.saturating_sub(1);
                    self.in_at_rule = self.at_depth > 0;
                }
                b';' if self.at_depth == 0 => self.in_at_rule = false,
                _ => {}
            }
            return;
        }

        if self.body_depth == 0 {
            if self.selector.is_empty() && (is_space(c) || c == b'@') {
                if c == b'@' {
                    self.in_at_rule = true;
                    self.at_depth = 0;
                }
                return;
            }
            if c == b'{' {
                self.body_depth = 1;
                self.style = Style::DEFAULT;
                self.declaration.clear();
                self.skipping_rule = self.selector_truncated;
                return;
            }
            if !self.selector.push(c) {
                self.selector_truncated = true;
                self.input_truncated = true;
            }
            return;
        }

        match c {
            b'{' => self.body_depth = self.body_depth.saturating_add(1),
            b'}' => {
                self.body_depth -= 1;
                if self.body_depth == 0 {
                    if !self.skipping_rule {
                        self.end_declaration();
                        rules.add_rule(self.selector.as_slice(), &self.style);
                    }
                    self.selector.clear();
                    self.declaration.clear();
                    self.skipping_rule = false;
                    self.selector_truncated = false;
                    self.declaration_truncated = false;
                }
            }
            _ if self.body_depth > 1 || self.skipping_rule => {}
            b';' => {
                self.end_declaration();
                self.declaration.clear();
                self.declaration_truncated = false;
            }
            _ => {
                if !self.declaration.push(c) {
                    self.declaration_truncated = true;
                    self.input_truncated = true;
                }
            }
        }
    }

    /// Applies the declaration read so far, unless it was cut.
    fn end_declaration(&mut self) {
        if !self.declaration_truncated && !self.declaration.is_empty() {
            apply_declaration(self.declaration.as_slice(), &mut self.style);
        }
    }
}
