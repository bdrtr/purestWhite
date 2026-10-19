//! A streaming XML parser for EPUB files: containers, package documents, NCX and XHTML.
//!
//! It reports what expat 2.7.3 reports as the firmware built it (`XML_GE=0`, no `XML_DTD`;
//! lib/expat until the Rust port), and `test/xml_parser_utils` pins that: the same events for
//! the same document, and an error where expat has one. Like that build it reads the internal DTD
//! subset but no external one: a declared general entity reads as its own reference (`&name;`),
//! attribute defaults apply, and an entity nobody declares fails the document unless it has an
//! external subset (then the entity is skipped). Encodings: UTF-8, UTF-16 either way, ISO-8859-1
//! and US-ASCII. Names keep their prefixes (no namespace processing).
//!
//! Fed in the same pieces, it also calls the handlers in the same pieces: text up to the end of
//! the input expat would have scanned, with expat's buffer and its reparse deferral modelled, and
//! text from a Latin-1 or UTF-16 document in expat's 1 KB conversion pieces. The chapter parser
//! lays text out by callback, so pages come out the same.
//!
//! The documents come from books: nothing here panics, every allocation is fallible, and one token,
//! the open element names and the DTD each stay within a limit the caller sets.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

use core::ffi::CStr;

mod chars;
mod decode;
mod dtd;
mod names;
mod parser;
mod scan;

pub use parser::{Attributes, Parser, StartTag, Str};

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;

/// Whether to go on after an event.
pub type Flow = core::ops::ControlFlow<()>;

/// What a document reports, in order. Text may arrive in several pieces.
pub trait Handler {
    fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
        let _ = tag;
        Flow::Continue(())
    }
    fn end_element(&mut self, name: Str<'_>) -> Flow {
        let _ = name;
        Flow::Continue(())
    }
    /// Character data, line ends normalized to LF.
    fn text(&mut self, text: &[u8]) -> Flow {
        let _ = text;
        Flow::Continue(())
    }
    fn comment(&mut self, text: Str<'_>) -> Flow {
        let _ = text;
        Flow::Continue(())
    }
    fn processing_instruction(&mut self, target: Str<'_>, data: Str<'_>) -> Flow {
        let _ = (target, data);
        Flow::Continue(())
    }
    fn cdata_start(&mut self) -> Flow {
        Flow::Continue(())
    }
    fn cdata_end(&mut self) -> Flow {
        Flow::Continue(())
    }
    /// A reference to an entity the external DTD subset may declare, which is not read.
    fn skipped_entity(&mut self, name: Str<'_>) -> Flow {
        let _ = name;
        Flow::Continue(())
    }
}

/// Why a document failed, as expat names it (`code` is expat's `XML_Error`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    NoMemory,
    Syntax,
    NoElements,
    InvalidToken,
    UnclosedToken,
    PartialChar,
    TagMismatch,
    DuplicateAttribute,
    JunkAfterDocElement,
    ParamEntityRef,
    UndefinedEntity,
    BadCharRef,
    BinaryEntityRef,
    MisplacedXmlPi,
    UnknownEncoding,
    IncorrectEncoding,
    UnclosedCdataSection,
    XmlDecl,
    PublicId,
    /// A handler asked to stop.
    Aborted,
    /// The document was already complete.
    Finished,
}

impl Error {
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            Error::NoMemory => 1,
            Error::Syntax => 2,
            Error::NoElements => 3,
            Error::InvalidToken => 4,
            Error::UnclosedToken => 5,
            Error::PartialChar => 6,
            Error::TagMismatch => 7,
            Error::DuplicateAttribute => 8,
            Error::JunkAfterDocElement => 9,
            Error::ParamEntityRef => 10,
            Error::UndefinedEntity => 11,
            Error::BadCharRef => 14,
            Error::BinaryEntityRef => 15,
            Error::MisplacedXmlPi => 17,
            Error::UnknownEncoding => 18,
            Error::IncorrectEncoding => 19,
            Error::UnclosedCdataSection => 20,
            Error::XmlDecl => 30,
            Error::PublicId => 32,
            Error::Aborted => 35,
            Error::Finished => 36,
        }
    }

    /// The error with expat's code `code`.
    #[must_use]
    pub fn from_code(code: u32) -> Option<Error> {
        ALL.iter().copied().find(|e| e.code() == code)
    }

    #[must_use]
    pub fn message(self) -> &'static CStr {
        match self {
            Error::NoMemory => c"out of memory",
            Error::Syntax => c"syntax error",
            Error::NoElements => c"no element found",
            Error::InvalidToken => c"not well-formed (invalid token)",
            Error::UnclosedToken => c"unclosed token",
            Error::PartialChar => c"partial character",
            Error::TagMismatch => c"mismatched tag",
            Error::DuplicateAttribute => c"duplicate attribute",
            Error::JunkAfterDocElement => c"junk after document element",
            Error::ParamEntityRef => c"illegal parameter entity reference",
            Error::UndefinedEntity => c"undefined entity",
            Error::BadCharRef => c"reference to invalid character number",
            Error::BinaryEntityRef => c"reference to binary entity",
            Error::MisplacedXmlPi => c"XML or text declaration not at start of entity",
            Error::UnknownEncoding => c"unknown encoding",
            Error::IncorrectEncoding => c"encoding specified in XML declaration is incorrect",
            Error::UnclosedCdataSection => c"unclosed CDATA section",
            Error::XmlDecl => c"XML declaration not well-formed",
            Error::PublicId => c"illegal character(s) in public id",
            Error::Aborted => c"parsing aborted",
            Error::Finished => c"parsing finished",
        }
    }
}

const ALL: [Error; 21] = [
    Error::NoMemory,
    Error::Syntax,
    Error::NoElements,
    Error::InvalidToken,
    Error::UnclosedToken,
    Error::PartialChar,
    Error::TagMismatch,
    Error::DuplicateAttribute,
    Error::JunkAfterDocElement,
    Error::ParamEntityRef,
    Error::UndefinedEntity,
    Error::BadCharRef,
    Error::BinaryEntityRef,
    Error::MisplacedXmlPi,
    Error::UnknownEncoding,
    Error::IncorrectEncoding,
    Error::UnclosedCdataSection,
    Error::XmlDecl,
    Error::PublicId,
    Error::Aborted,
    Error::Finished,
];
