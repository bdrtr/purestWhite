//! The tokenizer: expat's `xmltok_impl.c` over UTF-8. A token that runs to the end of the input
//! waits for more (`Short`) unless the input is complete (`fin`), but text is reported up to
//! where the input ends, as expat does.

use crate::chars::{
    Char, char_at, is_ascii_name_char, is_ascii_name_start, is_space, is_wide_name_char,
    is_xml_ascii,
};

/// The bytes being tokenized.
#[derive(Clone, Copy)]
pub(crate) struct Input<'a> {
    pub(crate) b: &'a [u8],
    /// Nothing follows `b`.
    pub(crate) fin: bool,
    pub(crate) latin1: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Scan<T> {
    /// A token and where it ends.
    Tok(T, usize),
    /// The input ends inside a token; with `fin`, the token is unclosed.
    Short,
    /// The input ends inside a character.
    ShortChar,
    /// Not well-formed.
    Invalid,
}

use Scan::{Invalid, Short, ShortChar, Tok};

use Char as Next;

/// Which ASCII bytes stop a loop that skips plain characters: those a document may not contain,
/// plus `extra`. A table rather than a bit set, so the test is one load on 32-bit cores. Built at
/// compile time, where an index out of range would not compile.
#[allow(clippy::indexing_slicing)]
const fn stops(mut extra: &[u8]) -> [bool; 128] {
    let mut table = [false; 128];
    let mut b: u8 = 0;
    while b < 0x20 {
        table[b as usize] = !matches!(b, b'\t' | b'\n' | b'\r');
        b += 1;
    }
    while let [first, rest @ ..] = extra {
        table[(*first & 0x7F) as usize] = true;
        extra = rest;
    }
    table
}

/// ASCII bytes that are not name characters.
#[allow(clippy::indexing_slicing)]
const NOT_NAME: [bool; 128] = {
    let mut table = [true; 128];
    let mut b: u8 = 0;
    while b < 0x80 {
        table[b as usize] = !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'.' | b'-'));
        b += 1;
    }
    table
};

/// Where plain characters stop in content, a CDATA section, an attribute value in either quote,
/// a comment, a processing instruction and a literal.
const CONTENT_STOP: [bool; 128] = stops(b"<&]\r\n");
const CDATA_STOP: [bool; 128] = stops(b"]\r\n");
const DOUBLE_QUOTED_STOP: [bool; 128] = stops(b"\"&<");
const SINGLE_QUOTED_STOP: [bool; 128] = stops(b"'&<");
const COMMENT_STOP: [bool; 128] = stops(b"-");
const PI_STOP: [bool; 128] = stops(b"?");
const LITERAL_STOP: [bool; 128] = stops(b"\"'");

/// Skips ASCII bytes `stop` lets through from `at`.
fn skip_plain(b: &[u8], mut at: usize, stop: &[bool; 128]) -> usize {
    while let Some(&c) = b.get(at) {
        if stop.get(usize::from(c)).is_none_or(|&s| s) {
            break;
        }
        at += 1;
    }
    at
}

impl Input<'_> {
    fn byte(&self, at: usize) -> Option<u8> {
        self.b.get(at).copied()
    }

    fn next(&self, at: usize) -> Next {
        char_at(self.b, at)
    }

    fn name_char(&self, cp: u32) -> bool {
        is_wide_name_char(cp, false, self.latin1)
    }

    fn name_start_char(&self, cp: u32) -> bool {
        is_wide_name_char(cp, true, self.latin1)
    }

    /// Whether what follows `<` at `at` makes it the root element's start tag in the prolog, as
    /// expat sees it before checking the name.
    fn may_start_element(&self, at: usize) -> bool {
        match self.byte(at) {
            Some(b) if b < 0x80 => is_ascii_name_start(b),
            // UTF-8: any lead byte.
            Some(b) if !self.latin1 => (0xC0..=0xF4).contains(&b),
            // Latin-1 and UTF-16: a Latin-1 letter, or anything from U+0100 but U+FFFE and U+FFFF.
            _ => match char_at(self.b, at) {
                Char::Wide { cp, .. } => cp > 0xFF || is_wide_name_char(cp, true, true),
                Char::Partial => self.byte(at).is_some(),
                Char::Invalid | Char::Ascii(_) => false,
            },
        }
    }

    /// The first character of a name at `at`: its length, or how the scan fails. `None` when
    /// it cannot start a name and the caller decides.
    fn name_start(&self, at: usize) -> Result<usize, Option<Scan<()>>> {
        match self.next(at) {
            Next::Ascii(b) if is_ascii_name_start(b) => Ok(1),
            Next::Ascii(_) => Err(None),
            Next::Wide { cp, len } if self.name_start_char(cp) => Ok(len),
            Next::Wide { .. } | Next::Invalid => Err(Some(Invalid)),
            Next::Partial if at >= self.b.len() => Err(Some(Short)),
            Next::Partial => Err(Some(ShortChar)),
        }
    }

    /// Skips name characters from `at`: where they end (the end of the input included), or how
    /// the scan fails on a character that is not ASCII and not a name character.
    fn name_rest(&self, mut at: usize) -> Result<usize, Scan<()>> {
        loop {
            while let Some(&c) = self.b.get(at) {
                if NOT_NAME.get(usize::from(c)).is_none_or(|&s| s) {
                    break;
                }
                at += 1;
            }
            match self.next(at) {
                Next::Ascii(_) => return Ok(at),
                Next::Wide { cp, len } if self.name_char(cp) => at += len,
                Next::Wide { .. } | Next::Invalid => return Err(Invalid),
                Next::Partial if at >= self.b.len() => return Ok(at),
                Next::Partial => return Err(ShortChar),
            }
        }
    }

    /// Skips characters a comment, literal or processing instruction may hold, up to one of the
    /// bytes `stop` was made with.
    fn chars_until(&self, mut at: usize, stop: &[bool; 128]) -> Result<usize, Scan<()>> {
        loop {
            at = skip_plain(self.b, at, stop);
            match self.next(at) {
                Next::Ascii(b) if b >= 0x20 && stop.get(usize::from(b)) == Some(&true) => {
                    return Ok(at);
                }
                Next::Ascii(b) if !is_xml_ascii(b) => return Err(Invalid),
                Next::Ascii(_) => at += 1,
                Next::Wide { len, .. } => at += len,
                Next::Partial if at >= self.b.len() => return Err(Short),
                Next::Partial => return Err(ShortChar),
                Next::Invalid => return Err(Invalid),
            }
        }
    }
}

fn fail<T>(scan: Scan<()>) -> Scan<T> {
    match scan {
        Tok((), _) | Invalid => Invalid,
        Short => Short,
        ShortChar => ShortChar,
    }
}

/// `<!-` was read; `at` follows it.
pub(crate) fn comment(input: &Input, mut at: usize) -> Scan<()> {
    match input.byte(at) {
        None => return Short,
        Some(b'-') => at += 1,
        Some(_) => return Invalid,
    }
    loop {
        match input.chars_until(at, &COMMENT_STOP) {
            Ok(dash) => at = dash + 1,
            Err(e) => return e,
        }
        match input.byte(at) {
            None => return Short,
            Some(b'-') => {}
            Some(_) => continue,
        }
        return match input.byte(at + 1) {
            None => Short,
            Some(b'>') => Tok((), at + 2),
            Some(_) => Invalid,
        };
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Pi {
    /// A processing instruction; its target ends at `target_end`.
    Other { target_end: usize },
    /// `<?xml ...?>`.
    XmlDecl,
}

/// `<?` was read; `at` follows it.
pub(crate) fn pi(input: &Input, at: usize) -> Scan<Pi> {
    let target = at;
    let first = match input.name_start(at) {
        Ok(len) => len,
        Err(None) => return Invalid,
        Err(Some(e)) => return fail(e),
    };
    let end = match input.name_rest(at + first) {
        Ok(end) => end,
        Err(e) => return fail(e),
    };
    if input.byte(end).is_none() {
        return Short;
    }
    let kind = match input.b.get(target..end) {
        Some(b"xml") => Pi::XmlDecl,
        Some(name) if name.eq_ignore_ascii_case(b"xml") => return Invalid,
        _ => Pi::Other { target_end: end },
    };
    match input.byte(end) {
        None => Short,
        Some(b'?') => match input.byte(end + 1) {
            None => Short,
            Some(b'>') => Tok(kind, end + 2),
            Some(_) => Invalid,
        },
        Some(b) if is_space(b) => {
            let mut at = end + 1;
            loop {
                match input.chars_until(at, &PI_STOP) {
                    Ok(q) => at = q + 1,
                    Err(e) => return fail(e),
                }
                match input.byte(at) {
                    None => return Short,
                    Some(b'>') => return Tok(kind, at + 1),
                    Some(_) => {}
                }
            }
        }
        Some(_) => Invalid,
    }
}

/// `<![` was read in content; `at` follows it.
fn cdata_open(input: &Input, at: usize) -> Scan<Content> {
    let Some(word) = input.b.get(at..at + 6) else {
        return Short;
    };
    if word == b"CDATA[" {
        Tok(Content::CdataOpen, at + 6)
    } else {
        Invalid
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Cdata {
    Data,
    Newline,
    Close,
}

/// Inside a CDATA section.
pub(crate) fn cdata(input: &Input, start: usize) -> Scan<Cdata> {
    let mut at = start;
    match input.next(at) {
        Next::Ascii(b']') => match input.byte(at + 1) {
            None => return Short,
            Some(b']') => match input.byte(at + 2) {
                None => return Short,
                Some(b'>') => return Tok(Cdata::Close, at + 3),
                Some(_) => at += 1,
            },
            Some(_) => at += 1,
        },
        Next::Ascii(b'\r') => {
            return match input.byte(at + 1) {
                None => Short,
                Some(b'\n') => Tok(Cdata::Newline, at + 2),
                Some(_) => Tok(Cdata::Newline, at + 1),
            };
        }
        Next::Ascii(b'\n') => return Tok(Cdata::Newline, at + 1),
        Next::Ascii(b) if !is_xml_ascii(b) => return Invalid,
        Next::Ascii(_) => at += 1,
        Next::Wide { len, .. } => at += len,
        Next::Partial if at >= input.b.len() => return Short,
        Next::Partial => return ShortChar,
        Next::Invalid => return Invalid,
    }
    loop {
        at = skip_plain(input.b, at, &CDATA_STOP);
        match input.next(at) {
            Next::Ascii(b) if matches!(b, b']' | b'\r' | b'\n') || !is_xml_ascii(b) => {
                return Tok(Cdata::Data, at);
            }
            Next::Ascii(_) => at += 1,
            Next::Wide { len, .. } => at += len,
            Next::Invalid | Next::Partial => return Tok(Cdata::Data, at),
        }
    }
}

/// `&` was read; `at` follows it. A character reference or an entity reference.
fn reference(input: &Input, at: usize) -> Scan<Content> {
    match input.byte(at) {
        None => return Short,
        Some(b'#') => return char_reference(input, at + 1),
        Some(_) => {}
    }
    let first = match input.name_start(at) {
        Ok(len) => len,
        Err(None) => return Invalid,
        Err(Some(e)) => return fail(e),
    };
    let end = match input.name_rest(at + first) {
        Ok(end) => end,
        Err(e) => return fail(e),
    };
    match input.byte(end) {
        None => Short,
        Some(b';') => Tok(Content::EntityRef, end + 1),
        Some(_) => Invalid,
    }
}

/// `&#` was read; `at` follows it.
fn char_reference(input: &Input, mut at: usize) -> Scan<Content> {
    let hex = input.byte(at) == Some(b'x');
    if hex {
        at += 1;
    }
    let digit = |b: u8| {
        if hex {
            b.is_ascii_hexdigit()
        } else {
            b.is_ascii_digit()
        }
    };
    match input.byte(at) {
        None => return Short,
        Some(b) if digit(b) => at += 1,
        Some(_) => return Invalid,
    }
    loop {
        match input.byte(at) {
            None => return Short,
            Some(b) if digit(b) => at += 1,
            Some(b';') => return Tok(Content::CharRef, at + 1),
            Some(_) => return Invalid,
        }
    }
}

/// The value of a character reference `&#...;` that `reference` accepted, or None when it
/// names no character XML allows.
pub(crate) fn char_ref_value(token: &[u8]) -> Option<u32> {
    let digits = token
        .get(2..token.len().saturating_sub(1))
        .unwrap_or_default();
    let (digits, radix) = match digits.split_first() {
        Some((b'x', rest)) => (rest, 16),
        _ => (digits, 10),
    };
    let mut value: u32 = 0;
    for &b in digits {
        value = value * radix + char::from(b).to_digit(radix)?;
        if value >= 0x11_0000 {
            return None;
        }
    }
    let allowed = match value {
        0..=0x1F => matches!(value, 0x9 | 0xA | 0xD),
        0xD800..=0xDFFF | 0xFFFE | 0xFFFF => false,
        _ => true,
    };
    allowed.then_some(value)
}

/// What `&name;` stands for when it is one of the five predefined entities.
pub(crate) fn predefined(name: &[u8]) -> Option<u8> {
    match name {
        b"lt" => Some(b'<'),
        b"gt" => Some(b'>'),
        b"amp" => Some(b'&'),
        b"quot" => Some(b'"'),
        b"apos" => Some(b'\''),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Content {
    /// Text up to the token's end.
    Data,
    /// A line end: CR, LF or CR LF.
    Newline,
    CharRef,
    EntityRef,
    /// A start tag, its name ending at `name_end`; `empty` for `<name/>`.
    StartTag {
        name_end: usize,
        empty: bool,
    },
    /// An end tag, its name ending at `name_end`.
    EndTag {
        name_end: usize,
    },
    CdataOpen,
    Pi(Pi),
    Comment,
    /// The document ends after a CR.
    TrailingCr,
    /// The document ends after `]` or `]]`.
    TrailingRsqb,
}

/// One token of content. Attributes of a start tag go to `atts` as (name start, name end,
/// value start, value end), the value without its quotes.
pub(crate) fn content(input: &Input, start: usize, atts: &mut AttSpans) -> Scan<Content> {
    let mut at = start;
    match input.next(at) {
        Next::Ascii(b'<') => return markup(input, at + 1, atts),
        Next::Ascii(b'&') => return reference(input, at + 1),
        Next::Ascii(b'\r') => {
            return match input.byte(at + 1) {
                None if input.fin => Tok(Content::TrailingCr, at + 1),
                None => Short,
                Some(b'\n') => Tok(Content::Newline, at + 2),
                Some(_) => Tok(Content::Newline, at + 1),
            };
        }
        Next::Ascii(b'\n') => return Tok(Content::Newline, at + 1),
        Next::Ascii(b']') => match input.byte(at + 1) {
            None if input.fin => return Tok(Content::TrailingRsqb, at + 1),
            None => return Short,
            Some(b']') => match input.byte(at + 2) {
                None if input.fin => return Tok(Content::TrailingRsqb, at + 2),
                None => return Short,
                Some(b'>') => return Invalid,
                Some(_) => at += 1,
            },
            Some(_) => at += 1,
        },
        Next::Ascii(b) if !is_xml_ascii(b) => return Invalid,
        Next::Ascii(_) => at += 1,
        Next::Wide { len, .. } => at += len,
        Next::Partial if at >= input.b.len() => return Short,
        Next::Partial => return ShortChar,
        Next::Invalid => return Invalid,
    }
    // A run of text ends before markup, a line end, a character XML forbids, or where the input
    // ends (before a character it cuts, or a `]` it ends too soon after to tell whether `]]>`
    // follows). A `]]>` inside makes it invalid.
    let short = |at: usize| Tok(Content::Data, at);
    loop {
        at = skip_plain(input.b, at, &CONTENT_STOP);
        match input.next(at) {
            Next::Ascii(b']') => match (input.byte(at + 1), input.byte(at + 2)) {
                (Some(b']'), Some(b'>')) => return Invalid,
                (Some(b']'), None) | (None, _) => return short(at),
                (Some(_), _) => at += 1,
            },
            Next::Ascii(b) if matches!(b, b'<' | b'&' | b'\r' | b'\n') || !is_xml_ascii(b) => {
                return Tok(Content::Data, at);
            }
            Next::Ascii(_) => at += 1,
            Next::Wide { len, .. } => at += len,
            Next::Invalid => return Tok(Content::Data, at),
            Next::Partial => return short(at),
        }
    }
}

/// `<` was read in content; `at` follows it.
fn markup(input: &Input, at: usize, atts: &mut AttSpans) -> Scan<Content> {
    match input.byte(at) {
        None => return Short,
        Some(b'!') => {
            return match input.byte(at + 1) {
                None => Short,
                Some(b'-') => match comment(input, at + 2) {
                    Tok((), end) => Tok(Content::Comment, end),
                    e => fail(e),
                },
                Some(b'[') => cdata_open(input, at + 2),
                Some(_) => Invalid,
            };
        }
        Some(b'?') => {
            return match pi(input, at + 1) {
                Tok(kind, end) => Tok(Content::Pi(kind), end),
                Short => Short,
                ShortChar => ShortChar,
                Invalid => Invalid,
            };
        }
        Some(b'/') => return end_tag(input, at + 1),
        Some(_) => {}
    }
    start_tag(input, at, atts)
}

/// A start tag whose name starts at `at`.
fn start_tag(input: &Input, at: usize, atts: &mut AttSpans) -> Scan<Content> {
    let first = match input.name_start(at) {
        Ok(len) => len,
        Err(None) => return Invalid,
        Err(Some(e)) => return fail(e),
    };
    let name_end = match input.name_rest(at + first) {
        Ok(end) => end,
        Err(e) => return fail(e),
    };
    atts.clear();
    let mut at = name_end;
    match input.byte(at) {
        None => return Short,
        Some(b'>') => {
            return Tok(
                Content::StartTag {
                    name_end,
                    empty: false,
                },
                at + 1,
            );
        }
        Some(b'/') => return empty_close(input, at + 1, name_end),
        Some(b) if is_space(b) => at += 1,
        Some(_) => return Invalid,
    }
    // Attributes, each after white space.
    loop {
        while input.byte(at).is_some_and(is_space) {
            at += 1;
        }
        match input.byte(at) {
            None => return Short,
            Some(b'>') => {
                return Tok(
                    Content::StartTag {
                        name_end,
                        empty: false,
                    },
                    at + 1,
                );
            }
            Some(b'/') => return empty_close(input, at + 1, name_end),
            Some(_) => {}
        }
        let (name, name_stop, value, value_stop) = match attribute(input, at) {
            Ok(spans) => spans,
            Err(e) => return e,
        };
        at = value_stop;
        if !atts.push(name, name_stop, value, value_stop) {
            return Invalid;
        }
        at += 1;
        match input.byte(at) {
            None => return Short,
            Some(b) if is_space(b) => at += 1,
            Some(b'/') => return empty_close(input, at + 1, name_end),
            Some(b'>') => {
                return Tok(
                    Content::StartTag {
                        name_end,
                        empty: false,
                    },
                    at + 1,
                );
            }
            Some(_) => return Invalid,
        }
        // The next attribute must start with a name character, after white space.
        while input.byte(at).is_some_and(is_space) {
            at += 1;
        }
        match input.byte(at) {
            None => return Short,
            Some(b'>') => {
                return Tok(
                    Content::StartTag {
                        name_end,
                        empty: false,
                    },
                    at + 1,
                );
            }
            Some(b'/') => return empty_close(input, at + 1, name_end),
            Some(_) => match input.name_start(at) {
                Ok(_) => {}
                Err(None) => return Invalid,
                Err(Some(e)) => return fail(e),
            },
        }
    }
}

/// One attribute of a start tag, from its name at `at` to its closing quote: the name's start
/// and end, and the value's start and end.
fn attribute(input: &Input, mut at: usize) -> Result<(usize, usize, usize, usize), Scan<Content>> {
    let name = at;
    let first = match input.name_start(at) {
        Ok(len) => len,
        Err(None) => return Err(Invalid),
        Err(Some(e)) => return Err(fail(e)),
    };
    let name_stop = match input.name_rest(at + first) {
        Ok(end) => end,
        Err(e) => return Err(fail(e)),
    };
    at = name_stop;
    // S? '=' S? quote
    while input.byte(at).is_some_and(is_space) {
        at += 1;
    }
    match input.byte(at) {
        None => return Err(Short),
        Some(b'=') => at += 1,
        Some(_) => return Err(Invalid),
    }
    let quote = loop {
        match input.byte(at) {
            None => return Err(Short),
            Some(q @ (b'"' | b'\'')) => break q,
            Some(b) if is_space(b) => at += 1,
            Some(_) => return Err(Invalid),
        }
    };
    at += 1;
    let value = at;
    let stop = if quote == b'"' {
        &DOUBLE_QUOTED_STOP
    } else {
        &SINGLE_QUOTED_STOP
    };
    loop {
        at = skip_plain(input.b, at, stop);
        match input.next(at) {
            Next::Ascii(b) if b == quote => break,
            Next::Ascii(b'&') => match reference(input, at + 1) {
                Tok(_, end) => at = end,
                e => return Err(e),
            },
            Next::Ascii(b) if b == b'<' || !is_xml_ascii(b) => return Err(Invalid),
            Next::Ascii(_) => at += 1,
            Next::Wide { len, .. } => at += len,
            Next::Partial if at >= input.b.len() => return Err(Short),
            Next::Partial => return Err(ShortChar),
            Next::Invalid => return Err(Invalid),
        }
    }
    Ok((name, name_stop, value, at))
}

/// `/` closing a start tag was read; `at` follows it.
fn empty_close(input: &Input, at: usize, name_end: usize) -> Scan<Content> {
    match input.byte(at) {
        None => Short,
        Some(b'>') => Tok(
            Content::StartTag {
                name_end,
                empty: true,
            },
            at + 1,
        ),
        Some(_) => Invalid,
    }
}

/// `</` was read; `at` follows it.
fn end_tag(input: &Input, at: usize) -> Scan<Content> {
    let first = match input.name_start(at) {
        Ok(len) => len,
        Err(None) => return Invalid,
        Err(Some(e)) => return fail(e),
    };
    let name_end = match input.name_rest(at + first) {
        Ok(end) => end,
        Err(e) => return fail(e),
    };
    let mut at = name_end;
    while input.byte(at).is_some_and(is_space) {
        at += 1;
    }
    match input.byte(at) {
        None => Short,
        Some(b'>') => Tok(Content::EndTag { name_end }, at + 1),
        Some(_) => Invalid,
    }
}

/// Attribute positions of the start tag being read.
pub(crate) struct AttSpans {
    pub(crate) spans: alloc::vec::Vec<[usize; 4]>,
    /// The scan failed because `spans` could not grow, not because the tag is malformed.
    pub(crate) out_of_memory: bool,
}

impl AttSpans {
    pub(crate) const fn new() -> Self {
        AttSpans {
            spans: alloc::vec::Vec::new(),
            out_of_memory: false,
        }
    }

    fn clear(&mut self) {
        self.spans.clear();
        self.out_of_memory = false;
    }

    /// False when memory runs out.
    fn push(&mut self, name: usize, name_end: usize, value: usize, value_end: usize) -> bool {
        if self.spans.try_reserve(1).is_err() {
            self.out_of_memory = true;
            return false;
        }
        self.spans.push([name, name_end, value, value_end]);
        true
    }
}

/// Pieces of an attribute value, after the tokenizer checked the whole tag (or a literal).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Value {
    Data,
    /// A line end, which counts as one space.
    Newline,
    /// A space or a tab.
    Space,
    CharRef,
    EntityRef,
    /// `<`, or `&` that starts no reference: a default value in the DTD may hold them.
    Invalid,
}

/// The piece of an attribute value at `at` (before its end) and where it ends.
pub(crate) fn value_piece(value: &[u8], at: usize, latin1: bool) -> (Value, usize) {
    match value.get(at).copied() {
        Some(b'&') => {
            let input = Input {
                b: value,
                fin: true,
                latin1,
            };
            return match reference(&input, at + 1) {
                Tok(Content::CharRef, end) => (Value::CharRef, end),
                Tok(_, end) => (Value::EntityRef, end),
                _ => (Value::Invalid, value.len()),
            };
        }
        Some(b'\n') => return (Value::Newline, at + 1),
        Some(b'\r') => {
            let end = if value.get(at + 1) == Some(&b'\n') {
                at + 2
            } else {
                at + 1
            };
            return (Value::Newline, end);
        }
        Some(b' ' | b'\t') => return (Value::Space, at + 1),
        Some(b'<') | None => return (Value::Invalid, value.len()),
        Some(_) => {}
    }
    let rest = value.get(at..).unwrap_or_default();
    let len = rest
        .iter()
        .position(|&b| matches!(b, b'&' | b'<' | b'\n' | b'\r' | b' ' | b'\t'))
        .unwrap_or(rest.len());
    (Value::Data, at + len)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Prolog {
    S,
    XmlDecl,
    Pi {
        target_end: usize,
    },
    Comment,
    /// `<!NAME`, the name ending at `name_end`.
    DeclOpen {
        name_end: usize,
    },
    CondSectOpen,
    InstanceStart,
    ParamEntityRef,
    Percent,
    Literal,
    Name,
    Nmtoken,
    NameQuestion,
    NameAsterisk,
    NamePlus,
    PoundName,
    OpenParen,
    CloseParen,
    CloseParenAsterisk,
    CloseParenQuestion,
    CloseParenPlus,
    Or,
    Comma,
    OpenBracket,
    CloseBracket,
    CondSectClose,
    DeclClose,
}

/// One token of the prolog, the internal DTD subset or what follows the root element.
pub(crate) fn prolog(input: &Input, start: usize) -> Scan<Prolog> {
    let at = start;
    // A token that may continue at the end of the input: complete only at the end.
    let at_end = |tok: Prolog| {
        if input.fin {
            Tok(tok, input.b.len())
        } else {
            Short
        }
    };
    let Some(first) = input.byte(at) else {
        return Short;
    };
    match first {
        b'"' | b'\'' => return literal(input, at + 1, first),
        b'<' => {
            return match input.byte(at + 1) {
                None => Short,
                Some(b'!') => decl(input, at + 2),
                Some(b'?') => match pi(input, at + 2) {
                    Tok(Pi::XmlDecl, end) => Tok(Prolog::XmlDecl, end),
                    Tok(Pi::Other { target_end }, end) => Tok(Prolog::Pi { target_end }, end),
                    Short => Short,
                    ShortChar => ShortChar,
                    Invalid => Invalid,
                },
                Some(_) if input.may_start_element(at + 1) => Tok(Prolog::InstanceStart, at),
                Some(_) => Invalid,
            };
        }
        b'\r' if at + 1 == input.b.len() => return at_end(Prolog::S),
        b' ' | b'\t' | b'\n' | b'\r' => {
            let mut end = at + 1;
            while let Some(b) = input.byte(end) {
                match b {
                    b' ' | b'\t' | b'\n' => end += 1,
                    // A CR at the end may start a CR LF pair.
                    b'\r' if end + 1 != input.b.len() => end += 1,
                    _ => break,
                }
            }
            return Tok(Prolog::S, end);
        }
        b'%' => return percent(input, at + 1),
        b',' => return Tok(Prolog::Comma, at + 1),
        b'[' => return Tok(Prolog::OpenBracket, at + 1),
        b']' => {
            return match input.byte(at + 1) {
                None => at_end(Prolog::CloseBracket),
                Some(b']') => match input.byte(at + 2) {
                    None => Short,
                    Some(b'>') => Tok(Prolog::CondSectClose, at + 3),
                    Some(_) => Tok(Prolog::CloseBracket, at + 1),
                },
                Some(_) => Tok(Prolog::CloseBracket, at + 1),
            };
        }
        b'(' => return Tok(Prolog::OpenParen, at + 1),
        b')' => {
            return match input.byte(at + 1) {
                None => at_end(Prolog::CloseParen),
                Some(b'*') => Tok(Prolog::CloseParenAsterisk, at + 2),
                Some(b'?') => Tok(Prolog::CloseParenQuestion, at + 2),
                Some(b'+') => Tok(Prolog::CloseParenPlus, at + 2),
                Some(b'\r' | b'\n' | b' ' | b'\t' | b'>' | b',' | b'|' | b')') => {
                    Tok(Prolog::CloseParen, at + 1)
                }
                Some(_) => Invalid,
            };
        }
        b'|' => return Tok(Prolog::Or, at + 1),
        b'>' => return Tok(Prolog::DeclClose, at + 1),
        b'#' => return pound_name(input, at + 1),
        _ => {}
    }
    name_token(input, at)
}

/// A name or a name token in the prolog, which `+`, `*` or `?` may follow.
fn name_token(input: &Input, at: usize) -> Scan<Prolog> {
    let (mut tok, mut end) = match input.next(at) {
        Next::Ascii(b) if is_ascii_name_start(b) => (Prolog::Name, at + 1),
        Next::Ascii(b) if is_ascii_name_char(b) => (Prolog::Nmtoken, at + 1),
        Next::Wide { cp, len } if input.name_start_char(cp) => (Prolog::Name, at + len),
        Next::Wide { cp, len } if input.name_char(cp) => (Prolog::Nmtoken, at + len),
        Next::Partial => return ShortChar,
        _ => return Invalid,
    };
    loop {
        match input.next(end) {
            Next::Ascii(b) if is_ascii_name_char(b) => end += 1,
            Next::Ascii(b'>' | b')' | b',' | b'|' | b'[' | b'%' | b' ' | b'\t' | b'\r' | b'\n') => {
                return Tok(tok, end);
            }
            Next::Ascii(b @ (b'+' | b'*' | b'?')) => {
                if tok == Prolog::Nmtoken {
                    return Invalid;
                }
                tok = match b {
                    b'+' => Prolog::NamePlus,
                    b'*' => Prolog::NameAsterisk,
                    _ => Prolog::NameQuestion,
                };
                return Tok(tok, end + 1);
            }
            Next::Wide { cp, len } if input.name_char(cp) => end += len,
            Next::Ascii(_) | Next::Wide { .. } | Next::Invalid => return Invalid,
            Next::Partial if end >= input.b.len() => {
                return if input.fin { Tok(tok, end) } else { Short };
            }
            Next::Partial => return ShortChar,
        }
    }
}

/// `<!` was read in the prolog; `at` follows it.
fn decl(input: &Input, at: usize) -> Scan<Prolog> {
    match input.byte(at) {
        None => return Short,
        Some(b'-') => {
            return match comment(input, at + 1) {
                Tok((), end) => Tok(Prolog::Comment, end),
                e => fail(e),
            };
        }
        Some(b'[') => return Tok(Prolog::CondSectOpen, at + 1),
        Some(_) => {}
    }
    // Letters only; a Latin-1 or UTF-16 document also has those of U+0080..U+00FF.
    let letter = |at: usize| match char_at(input.b, at) {
        Char::Ascii(b) => is_ascii_name_start(b).then_some(1),
        Char::Wide { cp, len }
            if input.latin1 && cp <= 0xFF && is_wide_name_char(cp, true, true) =>
        {
            Some(len)
        }
        _ => None,
    };
    let Some(first) = letter(at) else {
        return Invalid;
    };
    let mut end = at + first;
    loop {
        if let Some(len) = letter(end) {
            end += len;
            continue;
        }
        match input.byte(end) {
            None => return Short,
            Some(b'%') => {
                // No `<!ENTITY% foo "whatever">`.
                return match input.byte(end + 1) {
                    None => Short,
                    Some(b' ' | b'\t' | b'\r' | b'\n' | b'%') => Invalid,
                    Some(_) => Tok(Prolog::DeclOpen { name_end: end }, end),
                };
            }
            Some(b' ' | b'\t' | b'\r' | b'\n') => {
                return Tok(Prolog::DeclOpen { name_end: end }, end);
            }
            Some(_) => return Invalid,
        }
    }
}

/// An opening quote was read; `at` follows it.
fn literal(input: &Input, mut at: usize, quote: u8) -> Scan<Prolog> {
    loop {
        match input.chars_until(at, &LITERAL_STOP) {
            Ok(q) => at = q + 1,
            Err(e) => return fail(e),
        }
        if input.b.get(at - 1) != Some(&quote) {
            continue;
        }
        return match input.byte(at) {
            None if input.fin => Tok(Prolog::Literal, at),
            None => Short,
            Some(b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'%' | b'[') => Tok(Prolog::Literal, at),
            Some(_) => Invalid,
        };
    }
}

/// `%` was read; `at` follows it.
fn percent(input: &Input, at: usize) -> Scan<Prolog> {
    let first = match input.next(at) {
        Next::Ascii(b' ' | b'\t' | b'\r' | b'\n' | b'%') => return Tok(Prolog::Percent, at),
        Next::Ascii(b) if is_ascii_name_start(b) => 1,
        Next::Wide { cp, len } if input.name_start_char(cp) => len,
        Next::Partial if at >= input.b.len() => return Short,
        Next::Partial => return ShortChar,
        _ => return Invalid,
    };
    let end = match input.name_rest(at + first) {
        Ok(end) => end,
        Err(e) => return fail(e),
    };
    match input.byte(end) {
        None => Short,
        Some(b';') => Tok(Prolog::ParamEntityRef, end + 1),
        Some(_) => Invalid,
    }
}

/// `#` was read; `at` follows it.
fn pound_name(input: &Input, at: usize) -> Scan<Prolog> {
    let first = match input.name_start(at) {
        Ok(len) => len,
        Err(None) => return Invalid,
        Err(Some(e)) => return fail(e),
    };
    let end = match input.name_rest(at + first) {
        Ok(end) => end,
        Err(e) => return fail(e),
    };
    match input.byte(end) {
        None if input.fin => Tok(Prolog::PoundName, end),
        None => Short,
        Some(b'\r' | b'\n' | b' ' | b'\t' | b')' | b'>' | b'%' | b'|') => {
            Tok(Prolog::PoundName, end)
        }
        Some(_) => Invalid,
    }
}

/// Whether a literal (without its quotes) is a valid public identifier.
pub(crate) fn is_public_id(text: &[u8]) -> bool {
    text.iter()
        .all(|&b| b.is_ascii_alphanumeric() || b" -'()+,./:=?;!*#@$_%\r\n".contains(&b))
}
