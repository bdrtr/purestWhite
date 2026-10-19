//! The parser: input in pieces, tokens, and the events they make.

use alloc::vec::Vec;

use crate::chars::{is_space, push_utf8};
use crate::decode::{Decoder, Encoding};
use crate::dtd::{Dtd, Grammar, Reference, Role};
use crate::scan::{self, AttSpans, Cdata, Content, Input, Pi, Prolog, Scan, Value};
use crate::{Error, Flow, Handler};

/// A string the parser reports, followed by a NUL in memory for C callers.
#[derive(Clone, Copy, Debug)]
pub struct Str<'a>(&'a [u8]);

impl<'a> Str<'a> {
    /// The string without its NUL.
    #[must_use]
    pub fn bytes(&self) -> &'a [u8] {
        self.0.split_last().map_or(&[], |(_, rest)| rest)
    }

    /// The string with its NUL.
    #[must_use]
    pub fn with_nul(&self) -> &'a [u8] {
        self.0
    }
}

/// A start tag's attributes: those it has, in order, then the defaults the DTD adds.
#[derive(Clone, Copy)]
pub struct Attributes<'a> {
    strings: &'a [u8],
    /// Where each name and value starts in `strings`; each ends at a NUL.
    spans: &'a [[u32; 2]],
}

impl<'a> Attributes<'a> {
    fn at(&self, start: u32) -> Str<'a> {
        let rest = self.strings.get(start as usize..).unwrap_or_default();
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .map_or(rest.len(), |n| n + 1);
        Str(rest.get(..len).unwrap_or_default())
    }

    pub fn iter(&self) -> impl Iterator<Item = (Str<'a>, Str<'a>)> + '_ {
        self.spans
            .iter()
            .map(|&[name, value]| (self.at(name), self.at(value)))
    }

    #[must_use]
    pub fn get(&self, name: &[u8]) -> Option<Str<'a>> {
        self.iter().find(|(n, _)| n.bytes() == name).map(|(_, v)| v)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
}

pub struct StartTag<'a> {
    name: Str<'a>,
    attributes: Attributes<'a>,
    offset: u64,
}

impl<'a> StartTag<'a> {
    #[must_use]
    pub fn name(&self) -> Str<'a> {
        self.name
    }

    #[must_use]
    pub fn attributes(&self) -> Attributes<'a> {
        self.attributes
    }

    /// Where the tag starts in the input, in bytes.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Prolog,
    Content,
    Cdata,
    Epilog,
    Finished,
}

/// Expat's input buffer, as sizes and positions only. Expat 2.6+ puts off scanning a token
/// again until enough input has come (its reparse deferral), and a text run is reported up to
/// the end of what was scanned: so where its text callbacks split depends on this buffer, and
/// the chapter parser lays out text by callback. All in input bytes.
struct Window {
    size: usize,
    /// Where expat's next token starts, and where its input ends.
    ptr: usize,
    end: usize,
    /// Bytes the last scan left whole because the token at their start needed more.
    partial_before: usize,
    /// The last `XML_GetBuffer` size.
    request: usize,
    /// The input offset of `ptr`.
    consumed: u64,
}

/// `XML_CONTEXT_BYTES` and `INIT_BUFFER_SIZE` as the firmware built expat.
const CONTEXT: usize = 1024;
const INIT_BUFFER: usize = 1024;

impl Window {
    const fn new() -> Self {
        Window {
            size: 0,
            ptr: 0,
            end: 0,
            partial_before: 0,
            request: 0,
            consumed: 0,
        }
    }

    /// `XML_GetBuffer(len)`: room for `len` more bytes, keeping up to `CONTEXT` consumed ones.
    fn get_buffer(&mut self, len: usize) {
        self.request = len;
        if self.size != 0 && len <= self.size - self.end {
            return;
        }
        let keep = self.ptr.min(CONTEXT);
        let needed = len.saturating_add(self.end - self.ptr).saturating_add(keep);
        if self.size == 0 || needed > self.size {
            let mut size = self.size.max(INIT_BUFFER);
            loop {
                size = size.saturating_mul(2);
                if size >= needed || size == usize::MAX {
                    break;
                }
            }
            self.size = size;
        }
        // The bytes from `ptr - keep` move to the start.
        let offset = self.ptr - keep;
        self.ptr -= offset;
        self.end -= offset;
    }

    /// Whether expat scans what it holds now, or waits for more (never with the last input).
    fn scans(&self) -> bool {
        let have = self.end - self.ptr;
        let available = self.ptr - self.ptr.min(CONTEXT) + (self.size - self.end);
        have >= 2 * self.partial_before || self.request > available
    }
}

/// Expat converts text from Latin-1 or UTF-16 through a buffer of this many bytes, one callback
/// per buffer.
const CONVERT_BUFFER: usize = 1024;

pub struct Parser {
    /// Decoded input not consumed yet, from `pos`.
    buf: Vec<u8>,
    pos: usize,
    decoder: Decoder,
    part: Part,
    grammar: Grammar,
    dtd: Dtd,
    /// Names of the open elements, each followed by a NUL, and where each starts.
    tags: Vec<u8>,
    tag_starts: Vec<u32>,
    spans: AttSpans,
    /// The strings of the event being reported, each followed by a NUL.
    strings: Vec<u8>,
    attributes: Vec<[u32; 2]>,
    order: Vec<u32>,
    limit: usize,
    window: Window,
    error: Option<Error>,
    error_offset: u64,
    /// The input offset of `buf[cursor_at]`.
    cursor_at: usize,
    cursor_raw: u64,
    stopped: bool,
    /// The last input ended with a CR after the root element, which expat took without looking
    /// further (see `feed`).
    final_cr: bool,
}

impl Parser {
    /// A parser that fails with `Error::NoMemory` rather than hold more than `limit` bytes for
    /// one token, the open elements, or the DTD.
    #[must_use]
    pub const fn new(limit: usize) -> Self {
        Parser {
            buf: Vec::new(),
            pos: 0,
            decoder: Decoder::new(),
            part: Part::Prolog,
            grammar: Grammar::new(),
            dtd: Dtd::new(limit),
            tags: Vec::new(),
            tag_starts: Vec::new(),
            spans: AttSpans::new(),
            strings: Vec::new(),
            attributes: Vec::new(),
            order: Vec::new(),
            limit,
            window: Window::new(),
            error: None,
            error_offset: 0,
            cursor_at: 0,
            cursor_raw: 0,
            stopped: false,
            final_cr: false,
        }
    }

    /// Parses the next piece of the document; `last` when nothing follows. Events come out when
    /// expat's would: each token when it is complete and expat scans again, text up to the end
    /// of the input scanned.
    ///
    /// # Errors
    ///
    /// Why the document is not well-formed, or `Error::Aborted` when a handler stopped. Every
    /// later call returns the same error.
    pub fn feed(
        &mut self,
        data: &[u8],
        last: bool,
        handler: &mut dyn Handler,
    ) -> Result<(), Error> {
        if let Some(e) = self.error {
            return Err(e);
        }
        if self.part == Part::Finished {
            return Err(Error::Finished);
        }
        // Refuse input that would hold more than `limit` bytes, before copying it.
        if self.buf.len() - self.pos + data.len() > self.limit {
            return self.fail(Error::NoMemory);
        }
        let detected = self.decoder.encoding.is_some();
        self.window.get_buffer(data.len());
        self.window.end += data.len();
        if !self.decoder.decode(data, last, &mut self.buf) {
            return self.fail(Error::NoMemory);
        }
        if !detected && self.decoder.encoding.is_some() {
            self.cursor_raw = self.decoder.bom_len as u64;
            if self.decoder.too_short && data.len() + self.buf.len() > 0 {
                return self.fail(Error::UnclosedToken);
            }
        }
        if !last && !self.window.scans() {
            return self.check_pending();
        }
        let have = self.window.end - self.window.ptr;
        let result = self.run(last, handler);
        if let Err(e) = result {
            return self.fail(e);
        }
        let consumed = self.offset_at(self.pos);
        let used = usize::try_from(consumed - self.window.consumed)
            .unwrap_or(have)
            .min(have);
        self.window.consumed = consumed;
        self.window.ptr += used;
        self.window.partial_before = if used == 0 { have } else { 0 };
        self.compact();
        if last {
            // A UTF-16 document with an odd byte at its end passes in expat only when the last
            // input ends with a CR after the root element, a token expat takes without looking
            // further.
            if self.decoder.odd_byte && !self.final_cr {
                return self.fail(Error::UnclosedToken);
            }
            self.part = Part::Finished;
            return Ok(());
        }
        self.check_pending()
    }

    /// Reports a reference to an entity nobody declared as skipped (in an attribute value, it
    /// reads as nothing) instead of failing, even in a standalone document: XHTML uses HTML
    /// entities like `&nbsp;` with or without an external DTD. Call before the first `feed`.
    pub fn skip_undefined_entities(&mut self) {
        self.dtd.skip_undefined = true;
    }

    /// Where in the input the token that failed starts, in bytes.
    #[must_use]
    pub fn error_offset(&self) -> u64 {
        self.error_offset
    }

    fn check_pending(&mut self) -> Result<(), Error> {
        if self.buf.len() - self.pos > self.limit {
            return self.fail(Error::NoMemory);
        }
        Ok(())
    }

    fn fail(&mut self, e: Error) -> Result<(), Error> {
        self.error_offset = self.offset_at(self.pos);
        self.error = Some(e);
        Err(e)
    }

    fn encoding(&self) -> Encoding {
        self.decoder.encoding.unwrap_or(Encoding::Utf8)
    }

    /// The input offset of `buf[at]`, for `at` at or after the last one asked.
    fn offset_at(&mut self, at: usize) -> u64 {
        if at > self.cursor_at {
            let decoded = self.buf.get(self.cursor_at..at).unwrap_or_default();
            self.cursor_raw += self.encoding().raw_len(decoded);
            self.cursor_at = at;
        }
        self.cursor_raw
    }

    fn compact(&mut self) {
        let done = self.pos;
        if done == 0 {
            return;
        }
        self.offset_at(done);
        self.buf.drain(..done);
        self.pos = 0;
        self.cursor_at -= done;
    }

    fn run(&mut self, last: bool, h: &mut dyn Handler) -> Result<(), Error> {
        loop {
            let more = match self.part {
                Part::Prolog => self.prolog_step(last, h)?,
                Part::Content => self.content_step(last, h)?,
                Part::Cdata => self.cdata_step(last, h)?,
                Part::Epilog => self.epilog_step(last, h)?,
                Part::Finished => true,
            };
            if self.stopped {
                return Err(Error::Aborted);
            }
            if more {
                return Ok(());
            }
        }
    }

    fn input(&self, last: bool) -> Input<'_> {
        Input {
            b: &self.buf,
            fin: last,
            latin1: self.encoding().latin1_names(),
        }
    }

    /// What a token that needs more input means: wait, or fail at the end.
    fn short(last: bool, unclosed: Error) -> Result<bool, Error> {
        if last { Err(unclosed) } else { Ok(true) }
    }

    fn flow(&mut self, flow: Flow) {
        if flow.is_break() {
            self.stopped = true;
        }
    }

    /// What the end of the input means where a token could start.
    fn end(&self, last: bool, unclosed: Error) -> Result<bool, Error> {
        // A UTF-16 document's odd byte is the start of a token that never ends.
        let unclosed = if self.decoder.odd_byte {
            Error::UnclosedToken
        } else {
            unclosed
        };
        Self::short(last, unclosed)
    }

    fn prolog_step(&mut self, last: bool, h: &mut dyn Handler) -> Result<bool, Error> {
        if self.pos == self.buf.len() {
            return self.end(last, Error::NoElements);
        }
        let start = self.pos;
        match scan::prolog(&self.input(last), start) {
            Scan::Tok(tok, end) => {
                self.prolog_token(tok, start, end, last, h)?;
                Ok(false)
            }
            Scan::Short => Self::short(last, Error::UnclosedToken),
            Scan::ShortChar => Self::short(last, Error::PartialChar),
            Scan::Invalid => Err(Error::InvalidToken),
        }
    }

    fn prolog_token(
        &mut self,
        tok: Prolog,
        start: usize,
        end: usize,
        last: bool,
        h: &mut dyn Handler,
    ) -> Result<(), Error> {
        let role = self.grammar.role(tok, &self.buf, start, end);
        let name = self.buf.get(start..end).unwrap_or_default();
        match role {
            Role::InstanceStart => {
                self.part = Part::Content;
                return Ok(());
            }
            Role::XmlDecl => {
                self.pos = end;
                return self.xml_decl(start, end, last);
            }
            Role::DoctypePublicId | Role::EntityPublicId | Role::NotationPublicId => {
                let literal = self
                    .buf
                    .get(start + 1..end.saturating_sub(1))
                    .unwrap_or_default();
                if !scan::is_public_id(literal) {
                    return Err(Error::PublicId);
                }
                if role == Role::DoctypePublicId {
                    self.dtd.has_external = true;
                }
            }
            Role::DoctypeSystemId => self.dtd.has_external = true,
            Role::Pi => {
                if let Prolog::Pi { target_end } = tok {
                    self.report_pi(start, target_end, end, h)?;
                }
            }
            Role::Comment => self.report_comment(start, end, h)?,
            Role::GeneralEntityName => self.dtd.declare_entity(name)?,
            Role::ParamEntityName => self.dtd.declare_parameter_entity(),
            Role::EntityNotationName => self.dtd.unparsed_entity(),
            Role::AttlistElementName => self.dtd.attlist_element(name)?,
            Role::AttributeName => self.dtd.attribute_name(name)?,
            Role::AttributeType { cdata, id } => self.dtd.attribute_type(cdata, id),
            Role::NoDefault => self.dtd.define_attribute(None)?,
            Role::DefaultValue => {
                let cdata = self.dtd.declaring_cdata();
                let latin1 = self.encoding().latin1_names();
                let mut value = Vec::new();
                let literal = self
                    .buf
                    .get(start + 1..end.saturating_sub(1))
                    .unwrap_or_default();
                normalize_value(literal, cdata, latin1, &self.dtd, self.limit, &mut value)?;
                self.dtd.define_attribute(Some(&value))?;
            }
            Role::GroupOpen => self.dtd.group_open(self.grammar.level())?,
            Role::GroupSequence => self.dtd.connector(self.grammar.level(), b',')?,
            Role::GroupChoice => self.dtd.connector(self.grammar.level(), b'|')?,
            Role::Error => {
                return Err(match tok {
                    Prolog::ParamEntityRef => Error::ParamEntityRef,
                    Prolog::XmlDecl => Error::MisplacedXmlPi,
                    _ => Error::Syntax,
                });
            }
            Role::None | Role::EntityValue | Role::EntityComplete => {}
        }
        self.pos = end;
        Ok(())
    }

    /// `<?xml version="..." encoding="..." standalone="..."?>`, as expat reads it.
    fn xml_decl(&mut self, start: usize, end: usize, last: bool) -> Result<(), Error> {
        let body = self
            .buf
            .get(start + 5..end.saturating_sub(2))
            .unwrap_or_default();
        let decl = parse_xml_decl(body).ok_or(Error::XmlDecl)?;
        if decl.standalone {
            self.dtd.standalone = true;
        }
        let Some(name) = decl.encoding else {
            return Ok(());
        };
        let current = self.encoding();
        let named = if name.len() < 128 {
            Encoding::named(name, current)
        } else {
            None
        };
        let new = named.ok_or(Error::UnknownEncoding)?;
        if new.is_utf16() != current.is_utf16() || (new.is_utf16() && new != current) {
            return Err(Error::IncorrectEncoding);
        }
        if new != current {
            // Until now the bytes passed through as UTF-8; decode what follows the declaration.
            self.offset_at(self.pos);
            let mut tail = Vec::new();
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            if tail.try_reserve_exact(rest.len()).is_err() {
                return Err(Error::NoMemory);
            }
            tail.extend_from_slice(rest);
            self.buf.truncate(self.pos);
            self.decoder.encoding = Some(new);
            if !self.decoder.decode(&tail, last, &mut self.buf) {
                return Err(Error::NoMemory);
            }
        }
        Ok(())
    }

    fn content_step(&mut self, last: bool, h: &mut dyn Handler) -> Result<bool, Error> {
        if self.pos == self.buf.len() {
            return self.end(last, Error::NoElements);
        }
        let start = self.pos;
        let latin1 = self.encoding().latin1_names();
        let input = Input {
            b: &self.buf,
            fin: last,
            latin1,
        };
        match scan::content(&input, start, &mut self.spans) {
            Scan::Tok(tok, end) => {
                self.content_token(tok, start, end, h)?;
                Ok(false)
            }
            Scan::Short => Self::short(last, Error::UnclosedToken),
            Scan::ShortChar => Self::short(last, Error::PartialChar),
            Scan::Invalid if self.spans.out_of_memory => Err(Error::NoMemory),
            Scan::Invalid => Err(Error::InvalidToken),
        }
    }

    fn content_token(
        &mut self,
        tok: Content,
        start: usize,
        end: usize,
        h: &mut dyn Handler,
    ) -> Result<(), Error> {
        let token = self.buf.get(start..end).unwrap_or_default();
        match tok {
            Content::Data => {
                let flow = self.text(start, end, h);
                self.flow(flow);
            }
            Content::Newline => {
                let flow = h.text(b"\n");
                self.flow(flow);
            }
            Content::CharRef => {
                let cp = scan::char_ref_value(token).ok_or(Error::BadCharRef)?;
                let mut buf = [0; 4];
                let flow = h.text(crate::chars::encode_utf8(cp, &mut buf));
                self.flow(flow);
            }
            Content::EntityRef => {
                let name = token
                    .get(1..token.len().saturating_sub(1))
                    .unwrap_or_default();
                if let Some(c) = scan::predefined(name) {
                    let flow = h.text(&[c]);
                    self.flow(flow);
                } else {
                    match self.dtd.reference(name)? {
                        Reference::Text => {
                            // Expat expands it to `&amp;name;`: `&`, then the rest.
                            let (amp, rest) = token.split_at(1);
                            let mut flow = h.text(amp);
                            if flow.is_continue() {
                                flow = h.text(rest);
                            }
                            self.flow(flow);
                        }
                        Reference::Unparsed => return Err(Error::BinaryEntityRef),
                        Reference::Skipped => {
                            self.strings.clear();
                            push_str(&mut self.strings, name, self.limit)?;
                            let flow = h.skipped_entity(Str(&self.strings));
                            self.flow(flow);
                        }
                    }
                }
            }
            Content::StartTag { name_end, empty } => self.start_tag(start, name_end, empty, h)?,
            Content::EndTag { name_end } => {
                let name = self.buf.get(start + 2..name_end).unwrap_or_default();
                let top = self.tag_starts.last().map_or(0, |&s| s as usize);
                let open = self.tags.get(top..).unwrap_or_default();
                if open.split_last().map(|(_, n)| n) != Some(name) {
                    return Err(Error::TagMismatch);
                }
                let flow = h.end_element(Str(open));
                self.flow(flow);
                self.tags.truncate(top);
                self.tag_starts.pop();
                if self.tag_starts.is_empty() {
                    self.part = Part::Epilog;
                }
            }
            Content::CdataOpen => {
                let flow = h.cdata_start();
                self.flow(flow);
                self.part = Part::Cdata;
            }
            Content::Pi(Pi::XmlDecl) => return Err(Error::MisplacedXmlPi),
            Content::Pi(Pi::Other { target_end }) => self.report_pi(start, target_end, end, h)?,
            Content::Comment => self.report_comment(start, end, h)?,
            Content::TrailingCr => {
                let flow = h.text(b"\n");
                self.flow(flow);
                return Err(Error::NoElements);
            }
            Content::TrailingRsqb => {
                let flow = h.text(token);
                self.flow(flow);
                return Err(Error::NoElements);
            }
        }
        self.pos = end;
        Ok(())
    }

    fn start_tag(
        &mut self,
        start: usize,
        name_end: usize,
        empty: bool,
        h: &mut dyn Handler,
    ) -> Result<(), Error> {
        let latin1 = self.encoding().latin1_names();
        let offset = self.offset_at(start);
        let Parser {
            buf,
            strings,
            attributes,
            order,
            spans,
            dtd,
            limit,
            ..
        } = self;
        let name = buf.get(start + 1..name_end).unwrap_or_default();
        strings.clear();
        attributes.clear();
        push_str(strings, name, *limit)?;
        let atts = &spans.spans;
        let first_dup = first_duplicate(buf, atts, order)?;
        for (i, &[name_start, name_stop, value_start, value_stop]) in atts.iter().enumerate() {
            if i == first_dup {
                return Err(Error::DuplicateAttribute);
            }
            let att = buf.get(name_start..name_stop).unwrap_or_default();
            let cdata = dtd.is_cdata(name, att);
            let name_at = offset32(strings.len())?;
            push_str(strings, att, *limit)?;
            let value_at = offset32(strings.len())?;
            let value = buf.get(value_start..value_stop).unwrap_or_default();
            normalize_value(value, cdata, latin1, dtd, *limit, strings)?;
            push_str(strings, &[], *limit)?;
            if attributes.try_reserve(1).is_err() {
                return Err(Error::NoMemory);
            }
            attributes.push([name_at, value_at]);
        }
        if dtd.has_attlists() {
            let specified = attributes.len();
            for (att, value) in dtd.defaults(name) {
                let given =
                    attributes
                        .get(..specified)
                        .unwrap_or_default()
                        .iter()
                        .any(|&[n, _]| {
                            let s = strings
                                .get(n as usize..n as usize + att.len() + 1)
                                .unwrap_or_default();
                            s.split_last() == Some((&0, att))
                        });
                if given {
                    continue;
                }
                let name_at = offset32(strings.len())?;
                push_str(strings, att, *limit)?;
                let value_at = offset32(strings.len())?;
                push_str(strings, value, *limit)?;
                if attributes.try_reserve(1).is_err() {
                    return Err(Error::NoMemory);
                }
                attributes.push([name_at, value_at]);
            }
        }
        let name_len = name.len() + 1;
        let tag_name = Str(strings.get(..name_len).unwrap_or_default());
        let tag = StartTag {
            name: tag_name,
            attributes: Attributes {
                strings,
                spans: attributes,
            },
            offset,
        };
        let mut flow = h.start_element(&tag);
        if empty {
            // As in expat, the end follows the start even when the start asked to stop.
            let end_flow = h.end_element(tag_name);
            if flow.is_continue() {
                flow = end_flow;
            }
        } else {
            let at = offset32(self.tags.len())?;
            if self.tags.len() + name_len > self.limit
                || self.tags.try_reserve(name_len).is_err()
                || self.tag_starts.try_reserve(1).is_err()
            {
                return Err(Error::NoMemory);
            }
            self.tags
                .extend_from_slice(self.strings.get(..name_len).unwrap_or_default());
            self.tag_starts.push(at);
        }
        self.flow(flow);
        if empty && self.tag_starts.is_empty() {
            self.part = Part::Epilog;
        }
        Ok(())
    }

    fn cdata_step(&mut self, last: bool, h: &mut dyn Handler) -> Result<bool, Error> {
        if self.pos == self.buf.len() {
            return Self::short(last, Error::UnclosedCdataSection);
        }
        let start = self.pos;
        match scan::cdata(&self.input(last), start) {
            Scan::Tok(tok, end) => {
                let flow = match tok {
                    Cdata::Data => self.text(start, end, h),
                    Cdata::Newline => h.text(b"\n"),
                    Cdata::Close => {
                        self.part = Part::Content;
                        h.cdata_end()
                    }
                };
                self.flow(flow);
                self.pos = end;
                Ok(false)
            }
            Scan::Short => Self::short(last, Error::UnclosedCdataSection),
            Scan::ShortChar => Self::short(last, Error::PartialChar),
            Scan::Invalid => Err(Error::InvalidToken),
        }
    }

    fn epilog_step(&mut self, last: bool, h: &mut dyn Handler) -> Result<bool, Error> {
        if self.pos == self.buf.len() {
            return Ok(true);
        }
        let start = self.pos;
        match scan::prolog(&self.input(last), start) {
            Scan::Tok(Prolog::S, end) => {
                self.final_cr = last && end == self.buf.len() && self.buf.last() == Some(&b'\r');
                self.pos = end;
            }
            // A CR that ends the input may start a CR LF pair: expat takes it anyway here.
            Scan::Short if self.buf.get(start..) == Some(b"\r") => {
                self.pos += 1;
                return Ok(true);
            }
            Scan::Tok(Prolog::Pi { target_end }, end) => {
                self.report_pi(start, target_end, end, h)?;
                self.pos = end;
            }
            Scan::Tok(Prolog::Comment, end) => {
                self.report_comment(start, end, h)?;
                self.pos = end;
            }
            Scan::Tok(..) => return Err(Error::JunkAfterDocElement),
            Scan::Short => return Self::short(last, Error::UnclosedToken),
            Scan::ShortChar => return Self::short(last, Error::PartialChar),
            Scan::Invalid => return Err(Error::InvalidToken),
        }
        Ok(false)
    }

    /// Reports `buf[start..end]` as text: at once, or for a document expat converted, in pieces of
    /// whole characters that fill its conversion buffer.
    fn text(&self, start: usize, end: usize, h: &mut dyn Handler) -> Flow {
        let mut text = self.buf.get(start..end).unwrap_or_default();
        if self.encoding().latin1_names() && self.encoding() != Encoding::Ascii {
            while text.len() > CONVERT_BUFFER {
                let mut cut = 0;
                while let Some(&lead) = text.get(cut) {
                    let len = match lead {
                        0xF0..=0xF7 => 4,
                        0xE0..=0xEF => 3,
                        0xC0..=0xDF => 2,
                        _ => 1,
                    };
                    if cut + len > CONVERT_BUFFER {
                        break;
                    }
                    cut += len;
                }
                let (piece, rest) = text.split_at(cut);
                if h.text(piece).is_break() {
                    return Flow::Break(());
                }
                text = rest;
            }
        }
        h.text(text)
    }

    fn report_comment(
        &mut self,
        start: usize,
        end: usize,
        h: &mut dyn Handler,
    ) -> Result<(), Error> {
        let text = self
            .buf
            .get(start + 4..end.saturating_sub(3))
            .unwrap_or_default();
        self.strings.clear();
        push_lines(&mut self.strings, text, self.limit)?;
        let flow = h.comment(Str(&self.strings));
        self.flow(flow);
        Ok(())
    }

    fn report_pi(
        &mut self,
        start: usize,
        target_end: usize,
        end: usize,
        h: &mut dyn Handler,
    ) -> Result<(), Error> {
        let target = self.buf.get(start + 2..target_end).unwrap_or_default();
        let mut data_start = target_end;
        while self.buf.get(data_start).is_some_and(|&b| is_space(b))
            && data_start < end.saturating_sub(2)
        {
            data_start += 1;
        }
        let data = self
            .buf
            .get(data_start..end.saturating_sub(2))
            .unwrap_or_default();
        self.strings.clear();
        push_lines(&mut self.strings, target, self.limit)?;
        push_lines(&mut self.strings, data, self.limit)?;
        let split = target.len() + 1;
        let (t, d) = self.strings.split_at(split.min(self.strings.len()));
        let flow = h.processing_instruction(Str(t), Str(d));
        self.flow(flow);
        Ok(())
    }
}

fn offset32(n: usize) -> Result<u32, Error> {
    u32::try_from(n).map_err(|_| Error::NoMemory)
}

/// Appends `s` and a NUL.
fn push_str(out: &mut Vec<u8>, s: &[u8], limit: usize) -> Result<(), Error> {
    if out.len() + s.len() + 1 > limit || out.try_reserve(s.len() + 1).is_err() {
        return Err(Error::NoMemory);
    }
    out.extend_from_slice(s);
    out.push(0);
    Ok(())
}

/// Appends `s` with CR LF and CR turned into LF, and a NUL.
fn push_lines(out: &mut Vec<u8>, s: &[u8], limit: usize) -> Result<(), Error> {
    if out.len() + s.len() + 1 > limit || out.try_reserve(s.len() + 1).is_err() {
        return Err(Error::NoMemory);
    }
    let mut bytes = s.iter().copied().peekable();
    while let Some(b) = bytes.next() {
        if b == b'\r' {
            out.push(b'\n');
            if bytes.peek() == Some(&b'\n') {
                bytes.next();
            }
        } else {
            out.push(b);
        }
    }
    out.push(0);
    Ok(())
}

/// The index of the first attribute whose name an earlier one has, or the count.
fn first_duplicate(buf: &[u8], atts: &[[usize; 4]], order: &mut Vec<u32>) -> Result<usize, Error> {
    let name = |i: u32| {
        atts.get(i as usize)
            .and_then(|a| buf.get(a[0]..a[1]))
            .unwrap_or_default()
    };
    let n = u32::try_from(atts.len()).map_err(|_| Error::NoMemory)?;
    if n <= 8 {
        return Ok((1..n)
            .find(|&i| (0..i).any(|j| name(j) == name(i)))
            .map_or(atts.len(), |i| i as usize));
    }
    // Sort the indices by (name, index) and find, among equal names, the smallest second index.
    order.clear();
    if order.try_reserve(atts.len()).is_err() {
        return Err(Error::NoMemory);
    }
    order.extend(0..n);
    heapsort(order, |a, b| (name(a), a) < (name(b), b));
    let mut first = atts.len();
    for pair in order.windows(2) {
        if let [a, b] = *pair
            && name(a) == name(b)
        {
            first = first.min(b as usize);
        }
    }
    Ok(first)
}

/// Sorts in place with `less`: O(n log n) on any input, in little code.
fn heapsort(v: &mut [u32], less: impl Fn(u32, u32) -> bool) {
    let sift = |v: &mut [u32], mut root: usize, end: usize| {
        loop {
            let mut child = 2 * root + 1;
            if child >= end {
                break;
            }
            if child + 1 < end
                && less(
                    v.get(child).copied().unwrap_or(0),
                    v.get(child + 1).copied().unwrap_or(0),
                )
            {
                child += 1;
            }
            if !less(
                v.get(root).copied().unwrap_or(0),
                v.get(child).copied().unwrap_or(0),
            ) {
                break;
            }
            v.swap(root, child);
            root = child;
        }
    };
    for root in (0..v.len() / 2).rev() {
        sift(v, root, v.len());
    }
    for end in (1..v.len()).rev() {
        v.swap(0, end);
        sift(v, 0, end);
    }
}

/// Appends an attribute value as expat normalizes it: references resolved, each white space
/// character a space, and for a value that is not CDATA, spaces collapsed and trimmed.
fn normalize_value(
    value: &[u8],
    cdata: bool,
    latin1: bool,
    dtd: &Dtd,
    limit: usize,
    out: &mut Vec<u8>,
) -> Result<(), Error> {
    let start = out.len();
    if out.len() + value.len() > limit || out.try_reserve(value.len()).is_err() {
        return Err(Error::NoMemory);
    }
    let space_ends = |out: &Vec<u8>| out.len() == start || out.last() == Some(&b' ');
    let mut at = 0;
    while at < value.len() {
        let (piece, next) = scan::value_piece(value, at, latin1);
        let text = value.get(at..next).unwrap_or_default();
        match piece {
            Value::Data => out.extend_from_slice(text),
            Value::Newline | Value::Space => {
                if cdata || !space_ends(out) {
                    out.push(b' ');
                }
            }
            Value::CharRef => {
                let cp = scan::char_ref_value(text).ok_or(Error::BadCharRef)?;
                if cdata || cp != 0x20 || !space_ends(out) {
                    if out.try_reserve(4).is_err() {
                        return Err(Error::NoMemory);
                    }
                    push_utf8(out, cp);
                }
            }
            Value::EntityRef => {
                let name = text
                    .get(1..text.len().saturating_sub(1))
                    .unwrap_or_default();
                if let Some(c) = scan::predefined(name) {
                    out.push(c);
                } else {
                    match dtd.reference(name)? {
                        Reference::Text => out.extend_from_slice(text),
                        Reference::Unparsed => return Err(Error::BinaryEntityRef),
                        Reference::Skipped => {}
                    }
                }
            }
            Value::Invalid => return Err(Error::InvalidToken),
        }
        at = next;
    }
    if !cdata && out.len() > start && out.last() == Some(&b' ') {
        out.pop();
    }
    Ok(())
}

pub(crate) struct XmlDecl<'a> {
    pub(crate) encoding: Option<&'a [u8]>,
    pub(crate) standalone: bool,
}

/// The pseudo-attributes of an XML declaration (between `<?xml` and `?>`): `version`, then
/// optionally `encoding`, then optionally `standalone`, as expat's doParseXmlDecl reads them.
pub(crate) fn parse_xml_decl(body: &[u8]) -> Option<XmlDecl<'_>> {
    let mut at = 0;
    let (name, _) = pseudo_attribute(body, &mut at)??;
    if name != b"version" {
        return None;
    }
    let mut decl = XmlDecl {
        encoding: None,
        standalone: false,
    };
    let Some((mut name, mut value)) = pseudo_attribute(body, &mut at)? else {
        return Some(decl);
    };
    if name == b"encoding" {
        if !value.first().is_some_and(u8::is_ascii_alphabetic) {
            return None;
        }
        decl.encoding = Some(value);
        match pseudo_attribute(body, &mut at)? {
            None => return Some(decl),
            Some((n, v)) => (name, value) = (n, v),
        }
    }
    if name != b"standalone" {
        return None;
    }
    decl.standalone = match value {
        b"yes" => true,
        b"no" => false,
        _ => return None,
    };
    while body.get(at).is_some_and(|&b| is_space(b)) {
        at += 1;
    }
    (at == body.len()).then_some(decl)
}

/// One `S name = "value"`: None when malformed, Some(None) at the end.
#[allow(clippy::option_option)]
fn pseudo_attribute<'a>(body: &'a [u8], at: &mut usize) -> Option<Option<(&'a [u8], &'a [u8])>> {
    let byte = |i: usize| body.get(i).copied();
    if *at == body.len() {
        return Some(None);
    }
    if !byte(*at).is_some_and(is_space) {
        return None;
    }
    while byte(*at).is_some_and(is_space) {
        *at += 1;
    }
    if *at == body.len() {
        return Some(None);
    }
    let name_start = *at;
    let name_end;
    loop {
        let c = byte(*at).filter(u8::is_ascii)?;
        if c == b'=' {
            name_end = *at;
            break;
        }
        if is_space(c) {
            name_end = *at;
            while byte(*at).is_some_and(is_space) {
                *at += 1;
            }
            if byte(*at) != Some(b'=') {
                return None;
            }
            break;
        }
        *at += 1;
    }
    if name_end == name_start {
        return None;
    }
    *at += 1;
    while byte(*at).is_some_and(is_space) {
        *at += 1;
    }
    let quote = byte(*at).filter(|&c| c == b'"' || c == b'\'')?;
    *at += 1;
    let value_start = *at;
    loop {
        let c = byte(*at)?;
        if c == quote {
            break;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_')) {
            return None;
        }
        *at += 1;
    }
    let value = body.get(value_start..*at)?;
    *at += 1;
    Some(Some((body.get(name_start..name_end)?, value)))
}
