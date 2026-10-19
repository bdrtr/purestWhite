//! Reads an EPUB package document (content.opf) as lib/Epub's `ContentOpfParser` did, which
//! `test/content_opf_parser` pins: the book's title, creators, language, ISBN, ASIN and series
//! (calibre metas first, then an EPUB 3 collection of type series), the cover image (a cover meta
//! naming an image item, else the first `cover-image` item), the NCX, the EPUB 3 nav document, the
//! stylesheets, the guide's start and cover pages, and the spine's hrefs in order. Names are
//! matched by their local part (after the first `:`), except the manifest's, spine's and guide's
//! attributes, which are matched whole.
//!
//! Metadata text has XML whitespace collapsed and is kept to about 512 bytes. Hrefs are resolved
//! against the package's directory as `bembeyaz-path` does. The manifest is kept in memory, within
//! a limit the caller sets; a repeated id resolves to its first item, and a second manifest
//! replaces the first.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

mod manifest;
#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;

use bembeyaz_xml::{Error, Flow, Handler, Parser, StartTag, Str};
use manifest::Manifest;

/// Metadata text stops growing at this many bytes; the last addition can overshoot it by a `, ` or
/// a space and one byte.
pub const MAX_METADATA_TEXT: usize = 512;
/// EPUB 3 collections remembered while reading the metadata.
const MAX_COLLECTIONS: usize = 8;

/// What the reader hands out while it reads.
pub trait Sink {
    /// The next spine item's href.
    fn spine_item(&mut self, href: &[u8]);
    /// A manifest item of type `text/css`.
    fn stylesheet(&mut self, href: &[u8]);
    /// `text` as a finite number, the way the C library's `strtof` reads all of it, or `None`.
    fn parse_float(&mut self, text: &[u8]) -> Option<f32>;
}

/// What the package says, as far as it has been read.
pub struct Fields {
    pub title: Vec<u8>,
    pub author: Vec<u8>,
    pub language: Vec<u8>,
    pub isbn: Vec<u8>,
    pub asin: Vec<u8>,
    pub series: Vec<u8>,
    pub series_index: Option<f32>,
    pub toc_ncx: Vec<u8>,
    pub toc_nav: Vec<u8>,
    pub cover: Vec<u8>,
    pub guide_cover: Vec<u8>,
    pub text_reference: Vec<u8>,
}

/// Out of memory, or a manifest past the caller's limit.
struct Full;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Start,
    Package,
    Metadata,
    Title,
    Author,
    Language,
    Identifier,
    MetaText,
    Manifest,
    Spine,
    Guide,
}

struct Collection {
    id: Vec<u8>,
    title: Vec<u8>,
    index: Option<f32>,
    is_series: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct Reader {
    /// Resolve the spine through the manifest.
    spine: bool,
    /// Stop at the end of the metadata (or at a manifest, spine or guide).
    metadata_only: bool,
    base: Vec<u8>,
    manifest_limit: usize,
    state: State,
    metadata_complete: bool,
    stopped: bool,
    out_of_memory: bool,
    cover_item_id: Vec<u8>,
    has_explicit_start: bool,
    // Character data can come in several pieces for one text node, so whitespace and the
    // separator between creators are element state.
    space_pending: bool,
    author_separator_pending: bool,
    identifier_text: Vec<u8>,
    identifier_scheme: Vec<u8>,
    meta_text: Vec<u8>,
    meta_property: Vec<u8>,
    meta_refines: Vec<u8>,
    meta_id: Vec<u8>,
    collections: Vec<Collection>,
    calibre_series: Vec<u8>,
    calibre_series_index: Option<f32>,
    manifest: Manifest,
    /// The href being resolved, kept between items.
    href: Vec<u8>,
    fields: Fields,
}

/// A package document being read.
pub struct Package {
    parser: Parser,
    reader: Reader,
}

impl Package {
    /// A reader for a package in the directory `base` (with its trailing `/`). With `spine`, the
    /// manifest is kept to resolve the spine, its ids, hrefs and index within `manifest_limit`
    /// bytes. `limit` bounds the parser (see `Parser::new`).
    #[must_use]
    pub const fn new(
        base: Vec<u8>,
        spine: bool,
        metadata_only: bool,
        limit: usize,
        manifest_limit: usize,
    ) -> Self {
        Package {
            parser: Parser::new(limit),
            reader: Reader {
                spine,
                metadata_only,
                base,
                manifest_limit,
                state: State::Start,
                metadata_complete: false,
                stopped: false,
                out_of_memory: false,
                cover_item_id: Vec::new(),
                has_explicit_start: false,
                space_pending: false,
                author_separator_pending: false,
                identifier_text: Vec::new(),
                identifier_scheme: Vec::new(),
                meta_text: Vec::new(),
                meta_property: Vec::new(),
                meta_refines: Vec::new(),
                meta_id: Vec::new(),
                collections: Vec::new(),
                calibre_series: Vec::new(),
                calibre_series_index: None,
                manifest: Manifest::new(),
                href: Vec::new(),
                fields: Fields {
                    title: Vec::new(),
                    author: Vec::new(),
                    language: Vec::new(),
                    isbn: Vec::new(),
                    asin: Vec::new(),
                    series: Vec::new(),
                    series_index: None,
                    toc_ncx: Vec::new(),
                    toc_nav: Vec::new(),
                    cover: Vec::new(),
                    guide_cover: Vec::new(),
                    text_reference: Vec::new(),
                },
            },
        }
    }

    /// Reads the next piece of the package; `last` when nothing follows.
    ///
    /// # Errors
    ///
    /// The parser's error. `Error::Aborted` means a metadata-only read has all the metadata
    /// (every later call says so too); `Error::NoMemory` also covers a manifest past its limit.
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

    #[must_use]
    pub fn fields(&self) -> &Fields {
        &self.reader.fields
    }

    /// Where the token that failed starts in the input, in bytes.
    #[must_use]
    pub fn error_offset(&self) -> u64 {
        self.parser.error_offset()
    }
}

struct Events<'a> {
    reader: &'a mut Reader,
    sink: &'a mut dyn Sink,
}

// After running out of memory, the end of an empty element still comes: nothing more is read.
impl Handler for Events<'_> {
    fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
        if self.reader.out_of_memory {
            return Flow::Break(());
        }
        let result = self.reader.start(tag, self.sink);
        self.reader.flow(&result)
    }

    fn end_element(&mut self, name: Str<'_>) -> Flow {
        if self.reader.out_of_memory {
            return Flow::Break(());
        }
        let result = self.reader.end(name.bytes(), self.sink);
        self.reader.flow(&result)
    }

    fn text(&mut self, text: &[u8]) -> Flow {
        if self.reader.out_of_memory {
            return Flow::Break(());
        }
        let result = self.reader.text(text);
        self.reader.flow(&result)
    }
}

/// The part of a qualified name after its first `:`.
fn local(name: &[u8]) -> &[u8] {
    name.iter()
        .position(|&b| b == b':')
        .and_then(|at| name.get(at + 1..))
        .unwrap_or(name)
}

fn is_xml_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
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

/// At most `MAX_METADATA_TEXT` bytes of an attribute value.
fn bounded(value: &[u8]) -> &[u8] {
    value.get(..MAX_METADATA_TEXT).unwrap_or(value)
}

/// Adds metadata text with XML whitespace runs collapsed to one space (none at the start), stopping
/// at `MAX_METADATA_TEXT` bytes. With a pending separator, `, ` goes before the next character.
fn append_text(
    out: &mut Vec<u8>,
    text: &[u8],
    space_pending: &mut bool,
    mut separator_pending: Option<&mut bool>,
) -> Result<(), Full> {
    if out.len() >= MAX_METADATA_TEXT {
        return Ok(());
    }
    for &c in text {
        if is_xml_space(c) {
            *space_pending = true;
            continue;
        }
        if out.len() >= MAX_METADATA_TEXT {
            return Ok(());
        }
        match separator_pending.as_deref_mut() {
            Some(separator @ true) => {
                push(out, b", ")?;
                *separator = false;
            }
            _ if *space_pending && !out.is_empty() => push(out, b" ")?,
            _ => {}
        }
        *space_pending = false;
        push(out, &[c])?;
    }
    Ok(())
}

fn starts_with_ignore_case(value: &[u8], prefix: &[u8]) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

fn contains_ignore_case(value: &[u8], part: &[u8]) -> bool {
    part.is_empty()
        || value
            .windows(part.len())
            .any(|window| window.eq_ignore_ascii_case(part))
}

fn contains(value: &[u8], part: &[u8]) -> bool {
    value.windows(part.len()).any(|window| window == part)
}

/// `value` starts with `prefix` or `urn:<prefix>:`, ignoring case. `prefix` is lowercase.
fn has_metadata_prefix(value: &[u8], prefix: &[u8], urn: &[u8]) -> bool {
    starts_with_ignore_case(value, prefix) || starts_with_ignore_case(value, urn)
}

/// `value` without `urn:<prefix>:` or `prefix` and the `:`, spaces and tabs after it.
fn strip_metadata_prefix<'a>(value: &'a [u8], prefix: &[u8], urn: &[u8]) -> &'a [u8] {
    let offset = if starts_with_ignore_case(value, urn) {
        urn.len()
    } else if starts_with_ignore_case(value, prefix) {
        prefix.len()
    } else {
        return value;
    };
    let rest = value.get(offset..).unwrap_or_default();
    let skip = rest
        .iter()
        .position(|&b| !matches!(b, b':' | b' ' | b'\t'))
        .unwrap_or(rest.len());
    rest.get(skip..).unwrap_or_default()
}

/// A space-separated property list `properties` that names `word`: all of it, its start, or after
/// a space anywhere.
fn has_property(properties: &[u8], word: &[u8], word_space: &[u8], space_word: &[u8]) -> bool {
    properties == word || properties.starts_with(word_space) || contains(properties, space_word)
}

impl Reader {
    fn flow(&mut self, result: &Result<(), Full>) -> Flow {
        if result.is_err() {
            self.out_of_memory = true;
            self.stopped = true;
        }
        if self.stopped {
            Flow::Break(())
        } else {
            Flow::Continue(())
        }
    }

    fn stop(&mut self) {
        self.metadata_complete = true;
        if self.metadata_only {
            self.stopped = true;
        }
    }

    /// `base` + `href` in `self.href`, escapes decoded and the path normalised.
    fn resolve(&mut self, href: &[u8]) -> Result<(), Full> {
        let out = &mut self.href;
        set(out, &self.base)?;
        push(out, href)?;
        let len = bembeyaz_path::decode_uri_escapes(out);
        out.truncate(len);
        let len = bembeyaz_path::normalise(out);
        out.truncate(len);
        Ok(())
    }

    fn start(&mut self, tag: &StartTag<'_>, sink: &mut dyn Sink) -> Result<(), Full> {
        if self.metadata_only && self.metadata_complete {
            return Ok(());
        }
        let name = local(tag.name().bytes());
        if self.metadata_only && matches!(name, b"manifest" | b"spine" | b"guide") {
            self.stop();
            return Ok(());
        }
        match (self.state, name) {
            (State::Start, b"package") => self.state = State::Package,
            (State::Package, b"metadata") => self.state = State::Metadata,
            (State::Metadata, b"title") => {
                // Only the first title: later ones are subtitles.
                if self.fields.title.is_empty() {
                    self.state = State::Title;
                    self.space_pending = false;
                }
            }
            (State::Metadata, b"creator") => {
                self.state = State::Author;
                self.space_pending = false;
                self.author_separator_pending = !self.fields.author.is_empty();
            }
            (State::Metadata, b"language") => {
                self.state = State::Language;
                self.space_pending = false;
            }
            (State::Metadata, b"identifier") => {
                self.state = State::Identifier;
                self.identifier_text.clear();
                self.identifier_scheme.clear();
                self.space_pending = false;
                for (attribute, value) in tag.attributes().iter() {
                    if local(attribute.bytes()) == b"scheme" {
                        set(&mut self.identifier_scheme, bounded(value.bytes()))?;
                    }
                }
            }
            (State::Package, b"manifest") => {
                self.state = State::Manifest;
                // The items start over.
                self.manifest.clear();
            }
            (State::Package, b"spine") => self.state = State::Spine,
            (State::Package, b"guide") => self.state = State::Guide,
            (State::Metadata, b"meta") => self.meta(tag, sink)?,
            (State::Manifest, b"item") => self.item(tag, sink)?,
            (State::Spine, b"itemref") if self.spine => {
                for (attribute, value) in tag.attributes().iter() {
                    if attribute.bytes() == b"idref"
                        && let Some(href) = self.manifest.find(value.bytes())
                    {
                        sink.spine_item(href);
                    }
                }
            }
            (State::Guide, b"reference") => self.reference(tag)?,
            _ => {}
        }
        Ok(())
    }

    fn meta(&mut self, tag: &StartTag<'_>, sink: &mut dyn Sink) -> Result<(), Full> {
        let mut meta_name: &[u8] = &[];
        let mut content: &[u8] = &[];
        self.meta_property.clear();
        self.meta_refines.clear();
        self.meta_id.clear();
        self.meta_text.clear();
        for (attribute, value) in tag.attributes().iter() {
            let value = bounded(value.bytes());
            match local(attribute.bytes()) {
                b"name" => meta_name = value,
                b"content" => content = value,
                b"property" => set(&mut self.meta_property, value)?,
                b"refines" => set(
                    &mut self.meta_refines,
                    value.strip_prefix(b"#").unwrap_or(value),
                )?,
                b"id" => set(&mut self.meta_id, value)?,
                _ => {}
            }
        }
        if meta_name.eq_ignore_ascii_case(b"cover") {
            set(&mut self.cover_item_id, content)?;
        }
        if meta_name.eq_ignore_ascii_case(b"calibre:series") && self.calibre_series.is_empty() {
            set(&mut self.calibre_series, content)?;
        }
        if meta_name.eq_ignore_ascii_case(b"calibre:series_index")
            && self.calibre_series_index.is_none()
        {
            self.calibre_series_index = parse_float(sink, content);
        }
        let property = &self.meta_property;
        if property.eq_ignore_ascii_case(b"belongs-to-collection")
            || property.eq_ignore_ascii_case(b"collection-type")
            || property.eq_ignore_ascii_case(b"group-position")
        {
            self.state = State::MetaText;
            self.space_pending = false;
        }
        Ok(())
    }

    fn item(&mut self, tag: &StartTag<'_>, sink: &mut dyn Sink) -> Result<(), Full> {
        let mut id: &[u8] = &[];
        let mut media_type: &[u8] = &[];
        let mut properties: &[u8] = &[];
        self.href.clear();
        for (attribute, value) in tag.attributes().iter() {
            match attribute.bytes() {
                b"id" => id = value.bytes(),
                b"href" => self.resolve(value.bytes())?,
                b"media-type" => media_type = value.bytes(),
                b"properties" => properties = value.bytes(),
                _ => {}
            }
        }
        let href = &self.href;
        if self.spine {
            self.manifest.add(id, href, self.manifest_limit)?;
        }
        // A cover meta may name an XHTML wrapper: only an image counts.
        if id == self.cover_item_id.as_slice() && starts_with_ignore_case(media_type, b"image/") {
            set(&mut self.fields.cover, href)?;
        }
        if media_type == b"application/x-dtbncx+xml" && self.fields.toc_ncx.is_empty() {
            set(&mut self.fields.toc_ncx, href)?;
        }
        if media_type == b"text/css" {
            sink.stylesheet(href);
        }
        if !properties.is_empty()
            && self.fields.toc_nav.is_empty()
            && has_property(properties, b"nav", b"nav ", b" nav")
        {
            set(&mut self.fields.toc_nav, href)?;
        }
        if !properties.is_empty()
            && self.fields.cover.is_empty()
            && has_property(properties, b"cover-image", b"cover-image ", b" cover-image")
        {
            set(&mut self.fields.cover, href)?;
        }
        Ok(())
    }

    fn reference(&mut self, tag: &StartTag<'_>) -> Result<(), Full> {
        let mut kind: &[u8] = &[];
        self.href.clear();
        for (attribute, value) in tag.attributes().iter() {
            match attribute.bytes() {
                b"type" => kind = value.bytes(),
                b"href" => self.resolve(value.bytes())?,
                _ => {}
            }
        }
        let href = &self.href;
        if href.is_empty() {
            return Ok(());
        }
        // EPUB 2 guides often mark every content file as "text", so only "start" names where
        // reading begins.
        if kind == b"start" && !self.has_explicit_start {
            set(&mut self.fields.text_reference, href)?;
            self.has_explicit_start = true;
        } else if matches!(kind, b"cover" | b"cover-page") && self.fields.guide_cover.is_empty() {
            set(&mut self.fields.guide_cover, href)?;
        }
        Ok(())
    }

    fn text(&mut self, text: &[u8]) -> Result<(), Full> {
        if self.metadata_only && self.metadata_complete {
            return Ok(());
        }
        let pending = &mut self.space_pending;
        match self.state {
            State::Title => append_text(&mut self.fields.title, text, pending, None),
            State::Author => append_text(
                &mut self.fields.author,
                text,
                pending,
                Some(&mut self.author_separator_pending),
            ),
            State::Language => append_text(&mut self.fields.language, text, pending, None),
            State::Identifier => append_text(&mut self.identifier_text, text, pending, None),
            State::MetaText => append_text(&mut self.meta_text, text, pending, None),
            _ => Ok(()),
        }
    }

    fn end(&mut self, name: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        if self.metadata_only && self.metadata_complete {
            return Ok(());
        }
        let name = local(name);
        match (self.state, name) {
            (State::Spine, b"spine")
            | (State::Guide, b"guide")
            | (State::Manifest, b"manifest") => self.state = State::Package,
            (State::Title, b"title")
            | (State::Author, b"creator")
            | (State::Language, b"language") => self.state = State::Metadata,
            (State::Identifier, b"identifier") => {
                self.identifier_end()?;
                self.state = State::Metadata;
            }
            (State::MetaText, b"meta") => {
                self.meta_end(sink)?;
                self.state = State::Metadata;
            }
            (State::Metadata, b"metadata") => {
                self.metadata_end()?;
                self.state = State::Package;
                self.stop();
            }
            (State::Package, b"package") => self.state = State::Start,
            _ => {}
        }
        Ok(())
    }

    fn identifier_end(&mut self) -> Result<(), Full> {
        let scheme = &self.identifier_scheme;
        let text = &self.identifier_text;
        if self.fields.isbn.is_empty()
            && (contains_ignore_case(scheme, b"isbn")
                || has_metadata_prefix(text, b"isbn", b"urn:isbn:"))
        {
            set(
                &mut self.fields.isbn,
                strip_metadata_prefix(text, b"isbn", b"urn:isbn:"),
            )?;
        }
        if self.fields.asin.is_empty()
            && (contains_ignore_case(scheme, b"asin")
                || scheme.eq_ignore_ascii_case(b"amazon")
                || has_metadata_prefix(text, b"asin", b"urn:asin:"))
        {
            set(
                &mut self.fields.asin,
                strip_metadata_prefix(text, b"asin", b"urn:asin:"),
            )?;
        }
        Ok(())
    }

    fn meta_end(&mut self, sink: &mut dyn Sink) -> Result<(), Full> {
        let property = &self.meta_property;
        if property.eq_ignore_ascii_case(b"belongs-to-collection") && !self.meta_id.is_empty() {
            if let Some(collection) = self.collections.iter_mut().find(|c| c.id == self.meta_id) {
                set(&mut collection.title, &self.meta_text)?;
            } else if self.collections.len() < MAX_COLLECTIONS {
                let collection = Collection {
                    id: copy(&self.meta_id)?,
                    title: copy(&self.meta_text)?,
                    index: None,
                    is_series: false,
                };
                self.collections.try_reserve(1).map_err(|_| Full)?;
                self.collections.push(collection);
            }
        } else if !self.meta_refines.is_empty()
            && (property.eq_ignore_ascii_case(b"collection-type")
                || property.eq_ignore_ascii_case(b"group-position"))
        {
            let found = self
                .collections
                .iter()
                .position(|c| c.id == self.meta_refines);
            let at = match found {
                Some(at) => Some(at),
                None if self.collections.len() < MAX_COLLECTIONS => {
                    let collection = Collection {
                        id: copy(&self.meta_refines)?,
                        title: Vec::new(),
                        index: None,
                        is_series: false,
                    };
                    self.collections.try_reserve(1).map_err(|_| Full)?;
                    self.collections.push(collection);
                    Some(self.collections.len() - 1)
                }
                None => None,
            };
            if let Some(collection) = at.and_then(|at| self.collections.get_mut(at)) {
                if property.eq_ignore_ascii_case(b"collection-type") {
                    if self.meta_text.eq_ignore_ascii_case(b"series") {
                        collection.is_series = true;
                    }
                } else {
                    collection.index = parse_float(sink, &self.meta_text);
                }
            }
        }
        Ok(())
    }

    fn metadata_end(&mut self) -> Result<(), Full> {
        if !self.calibre_series.is_empty() {
            set(&mut self.fields.series, &self.calibre_series)?;
            self.fields.series_index = self.calibre_series_index;
        } else if let Some(collection) = self
            .collections
            .iter()
            .find(|c| c.is_series && !c.title.is_empty())
        {
            set(&mut self.fields.series, &collection.title)?;
            self.fields.series_index = collection.index;
        }
        Ok(())
    }
}

fn copy(bytes: &[u8]) -> Result<Vec<u8>, Full> {
    let mut out = Vec::new();
    push(&mut out, bytes)?;
    Ok(out)
}

fn parse_float(sink: &mut dyn Sink, text: &[u8]) -> Option<f32> {
    if text.is_empty() {
        None
    } else {
        sink.parse_float(text)
    }
}
