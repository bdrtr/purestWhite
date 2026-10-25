//! The chapter parser: [`chapter::Chapter`] reads a chapter's XHTML, styles and builds its
//! paragraphs and lays them out onto pages; the pages, their elements and the file are the
//! [`host::Host`]'s. [`anchors`] maps the chapter's element ids to the pages they land on, and
//! [`footnotes`] keeps the note references until the lines that show them are on a page.
//!
//! Names, references and the strings handed to the host end with a NUL, because the host passes
//! them on as C strings. Every allocation is fallible: what cannot be stored is reported, and
//! nothing here panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
// `(pager, pages)`: the page cursor and the pages it fills, side by side throughout.
#![allow(clippy::similar_names)]

extern crate alloc;

pub mod anchors;
mod blocks;
mod boxed;
pub mod chapter;
pub mod flow;
pub mod footnotes;
pub mod host;
mod place;
mod table;
pub mod text;

#[cfg(test)]
mod tests;
