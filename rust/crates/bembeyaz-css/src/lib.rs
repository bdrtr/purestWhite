//! EPUB stylesheets for the reader: the declarations an e-ink page can show, rules keyed by
//! simple selectors, and the lookup that merges them for one element.
//!
//! Behaviour matches the C++ parser this replaces (`lib/Epub/Epub/css/CssParser.cpp` up to
//! 09112bb): `test/css_parser` pins it, including the cache file, which keeps its format.
//!
//! Selectors: compounds of an optional element, an `#id` and up to four classes, joined by
//! descendant and child combinators, in comma-separated groups, with an optional
//! `::first-letter`. Everything else (sibling combinators, attributes, other pseudo-classes,
//! at-rules) is skipped. Rules apply in specificity order, then in source order.
//!
//! The text comes from books: nothing here panics, and memory grows only up to fixed caps,
//! through fallible allocation.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

mod ascii;
pub mod cache;
mod declaration;
mod number;
mod rules;
mod selector;
mod style;
mod tokenizer;

pub use declaration::parse_inline;
pub use rules::{Query, Rules, Stop};
pub use selector::Ancestor;
pub use style::{
    BorderSide, BorderStyle, Decoration, Defined, Direction, Display, FontStyle, FontWeight,
    Length, LengthField, ListStyle, Style, TextAlign, Unit, VerticalAlign, WIRE_BYTES,
};
pub use tokenizer::{Parse, Tokenizer};

#[cfg(test)]
extern crate std;
#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::format_push_string,
    clippy::same_item_push,
    reason = "tests"
)]
mod tests;
