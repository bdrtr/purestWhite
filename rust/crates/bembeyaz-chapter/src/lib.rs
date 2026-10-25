//! The chapter parser's (lib/Epub's `ChapterHtmlSlimParser`) state that outlives a paragraph:
//! [`anchors`] maps the chapter's element ids to the pages they land on, and [`footnotes`] keeps
//! the note references until the lines that show them are on a page.
//!
//! Names and references are kept back to back in one buffer each, with a NUL after every one,
//! because the parser hands them on as C strings. Every allocation is fallible: what cannot be
//! stored is reported, and nothing here panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

pub mod anchors;
pub mod footnotes;

#[cfg(test)]
mod tests;
