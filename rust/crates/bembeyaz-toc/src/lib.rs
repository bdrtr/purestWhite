//! An EPUB's table of contents, read from its NCX (EPUB 2) or nav (EPUB 3) document as lib/Epub's
//! `TocNcxParser` and `TocNavParser` read them up to e82f2461, which `test/toc_ncx_parser` and
//! `test/toc_nav_parser` pin: each entry's label, its target joined to the document's directory and
//! split at the first `#` into a path (`%XX` decoded, then `..` applied) and an anchor (`%XX`
//! decoded), and its depth.
//!
//! Both are loose state machines over element names as written (no namespace processing). An NCX
//! entry is a `navPoint` whose `navLabel`/`text` and `content` come before its nested points; a nav
//! entry is an `a` with an `href` inside `<nav epub:type="toc">` (or `type="toc"`) in the body,
//! and its depth counts the `ol` elements around it. Nav documents use HTML entities without
//! declaring them: those in labels are expanded, unknown ones kept as written. In an NCX an
//! undeclared entity stops the document (or is dropped behind an external DTD). Depths are 8-bit
//! and wrap, as the C++ readers did.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

use alloc::vec::Vec;
use bembeyaz_xml::{Error, Flow, Handler, Parser, StartTag, Str};

#[cfg(test)]
mod tests;

/// Which document is read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Ncx,
    Nav,
}

/// Where entries go.
pub trait Sink {
    /// One entry: a non-empty label, its resolved path and anchor (empty without `#`), its depth.
    fn entry(&mut self, label: &[u8], href: &[u8], anchor: &[u8], level: u8);
}

/// A table of contents document being read.
pub struct Toc {
    parser: Parser,
    reader: Reader,
}

impl Toc {
    /// A reader for a document in the directory `base` (with its trailing `/`, or empty).
    /// `limit` bounds the XML parser (see `Parser::new`).
    #[must_use]
    pub fn new(kind: Kind, base: Vec<u8>, limit: usize) -> Self {
        let mut parser = Parser::new(limit);
        if kind == Kind::Nav {
            parser.skip_undefined_entities();
        }
        Toc {
            parser,
            reader: Reader {
                state: match kind {
                    Kind::Ncx => State::Ncx(Ncx::Start),
                    Kind::Nav => State::Nav(Nav::Start),
                },
                base,
                depth: 0,
                label: Vec::new(),
                target: Vec::new(),
                href: Vec::new(),
                anchor: Vec::new(),
                out_of_memory: false,
            },
        }
    }

    /// Reads the next piece of the document; `last` when nothing follows.
    ///
    /// # Errors
    ///
    /// The parser's error; `Error::NoMemory` also when an entry could not be held.
    pub fn feed(&mut self, data: &[u8], last: bool, sink: &mut dyn Sink) -> Result<(), Error> {
        let mut events = Events {
            reader: &mut self.reader,
            sink,
        };
        match self.parser.feed(data, last, &mut events) {
            Err(Error::Aborted) if self.reader.out_of_memory => Err(Error::NoMemory),
            result => result,
        }
    }

    /// Where the token that failed starts in the input, in bytes.
    #[must_use]
    pub fn error_offset(&self) -> u64 {
        self.parser.error_offset()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ncx {
    Start,
    InRoot,
    InNavMap,
    InNavPoint,
    InNavLabel,
    InNavLabelText,
}

// Declared in nesting order: "at least InBody" compares.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Nav {
    Start,
    InHtml,
    InBody,
    InNavToc,
    InOl,
    InLi,
    InAnchor,
}

#[derive(Clone, Copy)]
enum State {
    Ncx(Ncx),
    Nav(Nav),
}

/// The allocation that failed.
struct Full;

struct Reader {
    state: State,
    base: Vec<u8>,
    /// Open `navPoint` (NCX) or `ol` (nav) elements.
    depth: u8,
    label: Vec<u8>,
    /// The entry's `src` (NCX) or `href` (nav).
    target: Vec<u8>,
    href: Vec<u8>,
    anchor: Vec<u8>,
    out_of_memory: bool,
}

fn push(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), Full> {
    out.try_reserve(bytes.len()).map_err(|_| Full)?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn set(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), Full> {
    out.clear();
    push(out, bytes)
}

/// The value of the first attribute named `name`, if any.
fn attribute<'a>(tag: &StartTag<'a>, name: &[u8]) -> Option<Str<'a>> {
    tag.attributes()
        .iter()
        .find(|(attribute, _)| attribute.bytes() == name)
        .map(|(_, value)| value)
}

impl Reader {
    fn flow(&mut self, result: &Result<(), Full>) -> Flow {
        if result.is_err() {
            self.out_of_memory = true;
            Flow::Break(())
        } else {
            Flow::Continue(())
        }
    }

    fn start(&mut self, tag: &StartTag<'_>) -> Result<(), Full> {
        let name = tag.name().bytes();
        match self.state {
            State::Ncx(state) => self.ncx_start(state, name, tag),
            State::Nav(state) => self.nav_start(state, name, tag),
        }
    }

    fn ncx_start(&mut self, state: Ncx, name: &[u8], tag: &StartTag<'_>) -> Result<(), Full> {
        match (state, name) {
            (Ncx::Start, b"ncx") => self.state = State::Ncx(Ncx::InRoot),
            (Ncx::InRoot, b"navMap") => self.state = State::Ncx(Ncx::InNavMap),
            (Ncx::InNavMap | Ncx::InNavPoint, b"navPoint") => {
                self.state = State::Ncx(Ncx::InNavPoint);
                self.depth = self.depth.wrapping_add(1);
                self.label.clear();
                self.target.clear();
            }
            (Ncx::InNavPoint, b"navLabel") => self.state = State::Ncx(Ncx::InNavLabel),
            (Ncx::InNavLabel, b"text") => self.state = State::Ncx(Ncx::InNavLabelText),
            (Ncx::InNavPoint, b"content") => {
                if let Some(src) = attribute(tag, b"src") {
                    set(&mut self.target, src.bytes())?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn nav_start(&mut self, state: Nav, name: &[u8], tag: &StartTag<'_>) -> Result<(), Full> {
        if name == b"html" {
            self.state = State::Nav(Nav::InHtml);
            return Ok(());
        }
        if state == Nav::InHtml && name == b"body" {
            self.state = State::Nav(Nav::InBody);
            return Ok(());
        }
        if state >= Nav::InBody && name == b"nav" {
            let toc = tag.attributes().iter().any(|(attribute, value)| {
                matches!(attribute.bytes(), b"epub:type" | b"type") && value.bytes() == b"toc"
            });
            if toc {
                self.state = State::Nav(Nav::InNavToc);
            }
            return Ok(());
        }
        // Lists and links count only inside the toc nav.
        if state < Nav::InNavToc {
            return Ok(());
        }
        if name == b"ol" {
            self.depth = self.depth.wrapping_add(1);
            self.state = State::Nav(Nav::InOl);
        } else if state == Nav::InOl && name == b"li" {
            self.state = State::Nav(Nav::InLi);
            self.label.clear();
            self.target.clear();
        } else if state == Nav::InLi && name == b"a" {
            self.state = State::Nav(Nav::InAnchor);
            if let Some(href) = attribute(tag, b"href") {
                set(&mut self.target, href.bytes())?;
            }
        }
        Ok(())
    }

    fn collecting(&self) -> bool {
        matches!(
            self.state,
            State::Ncx(Ncx::InNavLabelText) | State::Nav(Nav::InAnchor)
        )
    }

    fn text(&mut self, text: &[u8]) -> Result<(), Full> {
        if self.collecting() {
            push(&mut self.label, text)?;
        }
        Ok(())
    }

    /// An entity the document does not declare: in a nav label, an HTML one is expanded and any
    /// other kept as written.
    fn entity(&mut self, name: &[u8]) -> Result<(), Full> {
        if !matches!(self.state, State::Nav(Nav::InAnchor)) {
            return Ok(());
        }
        if let Some(value) = bembeyaz_html::entity(name) {
            push(&mut self.label, value.text())
        } else {
            push(&mut self.label, b"&")?;
            push(&mut self.label, name)?;
            push(&mut self.label, b";")
        }
    }

    fn end(&mut self, name: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        match self.state {
            State::Ncx(state) => self.ncx_end(state, name, sink),
            State::Nav(state) => self.nav_end(state, name, sink),
        }
    }

    fn ncx_end(&mut self, state: Ncx, name: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        match (state, name) {
            (Ncx::InNavLabelText, b"text") => self.state = State::Ncx(Ncx::InNavLabel),
            (Ncx::InNavLabel, b"navLabel") => self.state = State::Ncx(Ncx::InNavPoint),
            (Ncx::InNavPoint, b"navPoint") => {
                self.depth = self.depth.wrapping_sub(1);
                if self.depth == 0 {
                    self.state = State::Ncx(Ncx::InNavMap);
                }
            }
            (Ncx::InNavPoint, b"content") if !self.label.is_empty() && !self.target.is_empty() => {
                self.emit(sink)?;
                self.label.clear();
                self.target.clear();
            }
            _ => {}
        }
        Ok(())
    }

    fn nav_end(&mut self, state: Nav, name: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        if name == b"a" && state == Nav::InAnchor {
            if !self.label.is_empty() && !self.target.is_empty() {
                self.emit(sink)?;
                self.label.clear();
                self.target.clear();
            }
            self.state = State::Nav(Nav::InLi);
        } else if name == b"li" && matches!(state, Nav::InLi | Nav::InOl) {
            self.state = State::Nav(Nav::InOl);
        } else if name == b"ol" && state >= Nav::InNavToc {
            self.depth = self.depth.wrapping_sub(1);
            self.state = State::Nav(if self.depth == 0 {
                Nav::InNavToc
            } else {
                Nav::InLi
            });
        } else if name == b"nav" && state >= Nav::InNavToc {
            self.state = State::Nav(Nav::InBody);
        }
        Ok(())
    }

    /// Resolves the target against the base: the part before the first `#` decoded and
    /// normalised, the part after it decoded.
    fn emit(&mut self, sink: &mut dyn Sink) -> Result<(), Full> {
        set(&mut self.href, &self.base)?;
        push(&mut self.href, &self.target)?;
        let hash = self.href.iter().position(|&b| b == b'#');
        self.anchor.clear();
        if let Some(hash) = hash {
            push(
                &mut self.anchor,
                self.href.get(hash + 1..).unwrap_or_default(),
            )?;
            self.href.truncate(hash);
        }
        let len = bembeyaz_path::decode_uri_escapes(&mut self.href);
        self.href.truncate(len);
        let len = bembeyaz_path::normalise(&mut self.href);
        self.href.truncate(len);
        let len = bembeyaz_path::decode_uri_escapes(&mut self.anchor);
        self.anchor.truncate(len);
        sink.entry(&self.label, &self.href, &self.anchor, self.depth);
        Ok(())
    }
}

struct Events<'a> {
    reader: &'a mut Reader,
    sink: &'a mut dyn Sink,
}

impl Handler for Events<'_> {
    fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
        let result = self.reader.start(tag);
        self.reader.flow(&result)
    }

    fn end_element(&mut self, name: Str<'_>) -> Flow {
        let result = self.reader.end(name.bytes(), self.sink);
        self.reader.flow(&result)
    }

    fn text(&mut self, text: &[u8]) -> Flow {
        let result = self.reader.text(text);
        self.reader.flow(&result)
    }

    fn skipped_entity(&mut self, name: Str<'_>) -> Flow {
        let result = self.reader.entity(name.bytes());
        self.reader.flow(&result)
    }
}
