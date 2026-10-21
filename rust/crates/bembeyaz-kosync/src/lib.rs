//! `KOReader` positions in a chapter's XHTML, as `test/koreader_xpath_resolver` pins them: the path of the Nth paragraph, the path and
//! text offset of a visible codepoint, and the number of visible codepoints.
//!
//! Visible text is every codepoint inside `<body>` outside `head`, `style`, `script`, `title` and
//! `rp` (any case), with HTML entities as the chapter parser shows them: lib/Epub's
//! `ChapterHtmlSlimParser::characterData` counts the same, so offsets from the page table line up. Paths read like
//! `/body/DocFragment[8]/body/div[2]/p[4]/text()[1].6`: element names without their prefix, each
//! with its 1-based index among same-named siblings; `text()[N]` counts the non-empty text runs of
//! the element, a run ending at every tag, comment, processing instruction and CDATA edge. Text
//! arrives in the XML parser's pieces, and a piece of only spaces, tabs and line ends never holds
//! a position: a target inside one moves to the start of the next piece that is not. crengine also drops a
//! whitespace-only run that is the first child of a block, but which elements are blocks comes from
//! the style, which this does not see, so such a run counts here.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

use alloc::vec::Vec;
use bembeyaz_xml::{Error, Flow, Handler, Parser, StartTag, Str};
use core::fmt::Write;

/// What to look for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// The Nth (1-based) `<p>` or `<li>`, as the chapter parser counts `xpathParagraphIndex` and
    /// `xpathListItemIndex`.
    Paragraph(u32),
    /// The zero-based visible codepoint: the piece holding it, or with `inclusive` the piece
    /// ending at it too.
    Offset { target: u64, inclusive: bool },
    /// Nothing: count the visible codepoints.
    Count,
}

/// A chapter being searched, fed in pieces.
pub struct Resolver {
    parser: Parser,
    state: State,
    failed: bool,
}

/// A same-named sibling count of one open element.
struct Child {
    level: usize,
    name_start: usize,
    name_len: usize,
    count: u32,
}

/// An open element on the path below `<body>`.
struct Segment {
    name_start: usize,
    name_len: usize,
    index: u32,
}

#[allow(clippy::struct_excessive_bools)]
struct State {
    target: Target,
    spine_index: u32,
    inside_body: bool,
    depth: i32,
    body_depth: i32,
    non_visible_depth: u16,
    paragraphs: u32,
    visible: u64,
    last_text_end: u64,
    text_node_start: u64,
    pending_text_node: bool,
    /// Children seen so far of each open element, by level, innermost last.
    children: Vec<Child>,
    child_names: Vec<u8>,
    levels: usize,
    path: Vec<Segment>,
    path_names: Vec<u8>,
    /// Non-empty text runs so far in each open element.
    text_nodes: Vec<u32>,
    xpath: Vec<u8>,
    found: bool,
    stopped: bool,
    out_of_memory: bool,
}

fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&c| c == b':') {
        Some(colon) => name.get(colon + 1..).unwrap_or_default(),
        None => name,
    }
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\r' | b'\n' | b'\t')
}

/// Appends to a `Vec<u8>`, failing instead of aborting when memory runs out.
struct Out<'a>(&'a mut Vec<u8>);

impl Write for Out<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.try_reserve(s.len()).map_err(|_| core::fmt::Error)?;
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

impl State {
    fn push_level(&mut self) {
        self.levels += 1;
    }

    fn pop_level(&mut self) {
        if self.levels == 0 {
            return;
        }
        self.levels -= 1;
        let keep = self
            .children
            .iter()
            .rposition(|c| c.level < self.levels)
            .map_or(0, |i| i + 1);
        if let Some(first) = self.children.get(keep) {
            self.child_names.truncate(first.name_start);
        }
        self.children.truncate(keep);
    }

    fn clear_levels(&mut self) {
        self.levels = 0;
        self.children.clear();
        self.child_names.clear();
    }

    /// The next 1-based index of `name` among the innermost open element's children.
    fn next_index(&mut self, name: &[u8]) -> Option<u32> {
        let level = self.levels.checked_sub(1)?;
        let names = &self.child_names;
        for child in self.children.iter_mut().rev() {
            if child.level != level {
                break;
            }
            if names.get(child.name_start..child.name_start + child.name_len) == Some(name) {
                child.count += 1;
                return Some(child.count);
            }
        }
        if self.children.try_reserve(1).is_err()
            || self.child_names.try_reserve(name.len()).is_err()
        {
            self.out_of_memory = true;
            return None;
        }
        self.children.push(Child {
            level,
            name_start: self.child_names.len(),
            name_len: name.len(),
            count: 1,
        });
        self.child_names.extend_from_slice(name);
        Some(1)
    }

    fn push_segment(&mut self, name: &[u8], index: u32) -> bool {
        if self.path.try_reserve(1).is_err() || self.path_names.try_reserve(name.len()).is_err() {
            self.out_of_memory = true;
            return false;
        }
        self.path.push(Segment {
            name_start: self.path_names.len(),
            name_len: name.len(),
            index,
        });
        self.path_names.extend_from_slice(name);
        true
    }

    fn pop_segment(&mut self) {
        if let Some(segment) = self.path.pop() {
            self.path_names.truncate(segment.name_start);
        }
    }

    fn build_xpath(&mut self, text_node: u32, offset: u64) -> Flow {
        self.xpath.clear();
        let mut out = Out(&mut self.xpath);
        let mut ok = write!(
            out,
            "/body/DocFragment[{}]/body",
            u64::from(self.spine_index) + 1
        )
        .is_ok();
        for segment in &self.path {
            let name = self
                .path_names
                .get(segment.name_start..segment.name_start + segment.name_len)
                .unwrap_or_default();
            ok &= out.write_str("/").is_ok()
                && out.0.try_reserve(name.len()).is_ok()
                && {
                    out.0.extend_from_slice(name);
                    true
                }
                && write!(out, "[{}]", segment.index).is_ok();
        }
        if text_node > 0 {
            ok &= write!(out, "/text()[{text_node}].{offset}").is_ok();
        }
        if !ok {
            self.out_of_memory = true;
            self.xpath.clear();
            return Flow::Break(());
        }
        self.found = true;
        self.stopped = true;
        Flow::Break(())
    }

    fn text(&mut self, data: &[u8]) -> Flow {
        match self.target {
            Target::Paragraph(_) => Flow::Continue(()),
            Target::Count => {
                if !self.inside_body || self.non_visible_depth > 0 || data.is_empty() {
                    return Flow::Continue(());
                }
                self.visible += bembeyaz_utf8::codepoints(data).count() as u64;
                if !data.iter().all(|&c| is_space(c)) {
                    // Trailing whitespace is single-byte, so bytes equal codepoints here.
                    let trailing = data.iter().rev().take_while(|&&c| is_space(c)).count();
                    self.last_text_end = self.visible - trailing as u64;
                }
                Flow::Continue(())
            }
            Target::Offset { target, inclusive } => {
                if !self.inside_body
                    || self.non_visible_depth > 0
                    || data.is_empty()
                    || self.stopped
                {
                    return Flow::Continue(());
                }
                let count = bembeyaz_utf8::codepoints(data).count() as u64;
                if count == 0 {
                    return Flow::Continue(());
                }
                if self.pending_text_node {
                    if let Some(nodes) = self.text_nodes.last_mut() {
                        *nodes += 1;
                    }
                    self.text_node_start = self.visible;
                    self.pending_text_node = false;
                }
                let next = self.visible + count;
                let reached = if inclusive {
                    target <= next
                } else {
                    target < next
                };
                if reached && !data.iter().all(|&c| is_space(c)) {
                    let delta = target.saturating_sub(self.visible);
                    let node = self.text_nodes.last().copied().unwrap_or(0);
                    let offset = self.visible - self.text_node_start + delta;
                    return self.build_xpath(node, offset);
                }
                self.visible = next;
                Flow::Continue(())
            }
        }
    }
}

impl Handler for State {
    fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
        let name = local_name(tag.name().bytes());
        if !self.inside_body {
            if name == b"body" {
                self.inside_body = true;
                self.body_depth = self.depth;
                if self.target != Target::Count {
                    self.push_level();
                }
                if matches!(self.target, Target::Offset { .. }) {
                    if self.text_nodes.try_reserve(1).is_err() {
                        self.out_of_memory = true;
                        return Flow::Break(());
                    }
                    self.text_nodes.push(0);
                }
            }
            self.depth += 1;
            return Flow::Continue(());
        }
        match self.target {
            Target::Count => {
                if self.non_visible_depth > 0 || bembeyaz_html::is_non_visible_element(name) {
                    self.non_visible_depth = self.non_visible_depth.wrapping_add(1);
                }
            }
            Target::Paragraph(target) => {
                let Some(index) = self.next_index(name) else {
                    return Flow::Break(());
                };
                if !self.push_segment(name, index) {
                    return Flow::Break(());
                }
                self.push_level();
                if name == b"p" || name == b"li" {
                    self.paragraphs += 1;
                }
                if self.paragraphs == target {
                    self.depth += 1;
                    return self.build_xpath(0, 0);
                }
            }
            Target::Offset { .. } => {
                self.pending_text_node = true;
                let Some(index) = self.next_index(name) else {
                    return Flow::Break(());
                };
                if !self.push_segment(name, index) || self.text_nodes.try_reserve(1).is_err() {
                    self.out_of_memory = true;
                    return Flow::Break(());
                }
                self.push_level();
                self.text_nodes.push(0);
                if self.non_visible_depth > 0 || bembeyaz_html::is_non_visible_element(name) {
                    self.non_visible_depth = self.non_visible_depth.wrapping_add(1);
                }
            }
        }
        self.depth += 1;
        Flow::Continue(())
    }

    fn end_element(&mut self, raw: Str<'_>) -> Flow {
        let name = local_name(raw.bytes());
        self.depth -= 1;
        if !self.inside_body {
            return Flow::Continue(());
        }
        if self.depth == self.body_depth && name == b"body" {
            self.inside_body = false;
            self.non_visible_depth = 0;
            self.clear_levels();
            self.path.clear();
            self.path_names.clear();
            self.text_nodes.clear();
            return Flow::Continue(());
        }
        if self.non_visible_depth > 0 {
            self.non_visible_depth -= 1;
        }
        if matches!(self.target, Target::Offset { .. }) {
            self.pending_text_node = true;
            self.text_nodes.pop();
        }
        if self.target != Target::Count {
            self.pop_segment();
            self.pop_level();
        }
        Flow::Continue(())
    }

    fn text(&mut self, data: &[u8]) -> Flow {
        State::text(self, data)
    }

    fn comment(&mut self, _: Str<'_>) -> Flow {
        self.markup_boundary()
    }

    fn processing_instruction(&mut self, _: Str<'_>, _: Str<'_>) -> Flow {
        self.markup_boundary()
    }

    fn cdata_start(&mut self) -> Flow {
        self.markup_boundary()
    }

    fn cdata_end(&mut self) -> Flow {
        self.markup_boundary()
    }

    fn skipped_entity(&mut self, name: Str<'_>) -> Flow {
        if matches!(self.target, Target::Paragraph(_)) {
            return Flow::Continue(());
        }
        let name = name.bytes();
        if let Some(value) = bembeyaz_html::entity(name) {
            return State::text(self, value.text());
        }
        // Not an HTML entity: kept as written, in one piece when it fits in 48 bytes.
        if name.len() + 2 > 48 {
            for piece in [&b"&"[..], name, b";"] {
                if State::text(self, piece).is_break() {
                    return Flow::Break(());
                }
            }
            return Flow::Continue(());
        }
        let mut reference = [0u8; 48];
        let len = name.len() + 2;
        if let Some(slot) = reference.get_mut(1..len - 1) {
            slot.copy_from_slice(name);
        }
        if let Some(first) = reference.first_mut() {
            *first = b'&';
        }
        if let Some(semicolon) = reference.get_mut(len - 1) {
            *semicolon = b';';
        }
        State::text(self, reference.get(..len).unwrap_or_default())
    }
}

impl State {
    fn markup_boundary(&mut self) -> Flow {
        if matches!(self.target, Target::Offset { .. })
            && self.inside_body
            && self.non_visible_depth == 0
            && !self.stopped
        {
            self.pending_text_node = true;
        }
        Flow::Continue(())
    }
}

impl Resolver {
    /// `limit` bounds the XML parser (see `Parser::new`); `spine_index` is the chapter's
    /// zero-based place in the spine.
    #[must_use]
    pub fn new(target: Target, spine_index: u32, limit: usize) -> Self {
        let mut parser = Parser::new(limit);
        parser.skip_undefined_entities();
        Resolver {
            parser,
            state: State {
                target,
                spine_index,
                inside_body: false,
                depth: 0,
                body_depth: -1,
                non_visible_depth: 0,
                paragraphs: 0,
                visible: 0,
                last_text_end: 0,
                text_node_start: 0,
                pending_text_node: true,
                children: Vec::new(),
                child_names: Vec::new(),
                levels: 0,
                path: Vec::new(),
                path_names: Vec::new(),
                text_nodes: Vec::new(),
                xpath: Vec::new(),
                found: false,
                stopped: false,
                out_of_memory: false,
            },
            failed: false,
        }
    }

    fn run(&mut self, data: &[u8], last: bool) -> Result<(), Error> {
        if self.failed || self.state.stopped {
            return Ok(());
        }
        match self.parser.feed(data, last, &mut self.state) {
            Ok(()) => Ok(()),
            // Found while reading earlier input. Found only at the end of the chapter (input the
            // parser held back), it fails like any error at the end, as the C++ resolver did.
            Err(Error::Aborted) if !last && !self.state.out_of_memory => Ok(()),
            Err(error) => {
                self.failed = true;
                Err(if self.state.out_of_memory {
                    Error::NoMemory
                } else {
                    error
                })
            }
        }
    }

    /// Reads the next piece of the chapter; nothing once it failed or found its target.
    ///
    /// # Errors
    ///
    /// Why the chapter failed, when it fails here.
    pub fn feed(&mut self, data: &[u8]) -> Result<(), Error> {
        self.run(data, false)
    }

    /// Ends the chapter.
    ///
    /// # Errors
    ///
    /// Why the chapter failed, when it fails here; [`Error::Aborted`] when the target was found
    /// only now.
    pub fn finish(&mut self) -> Result<(), Error> {
        self.run(&[], true)
    }

    /// The chapter has failed.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// The path found, if any.
    #[must_use]
    pub fn xpath(&self) -> Option<&[u8]> {
        self.state.found.then_some(self.state.xpath.as_slice())
    }

    /// For [`Target::Count`]: the visible codepoints up to the last one that is not whitespace, so a
    /// 100% target lands inside real text.
    #[must_use]
    pub fn visible_codepoints(&self) -> u64 {
        self.state.last_text_end
    }
}

#[cfg(test)]
mod tests;
