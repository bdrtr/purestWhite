//! The other way: a `KOReader` path in a chapter's raw XHTML. The path is read by
//! [`XPath::parse`]; the chapter by a [`Locator`] a byte at a time, with a loose scanner rather
//! than an XML parser: tags end at `>` (quotes are only tracked for `/`), names are cut at 11
//! bytes and compared without case, entities of up to 15 bytes count as their text, `\r\n` and
//! `\r` count as one newline, and visible text is what [`crate`] counts. Arithmetic on numbers in
//! the path wraps.

/// Steps of a path kept.
pub const MAX_STEPS: usize = 16;
/// Bytes of a step's name, with its terminating NUL.
pub const TAG_BYTES: usize = 12;

const ENTITY_BYTES: usize = 16;
const ANCHOR_BYTES: usize = 64;

/// One step of a path below the chapter's `<body>`: a name and a 1-based sibling index (0: any).
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Step {
    pub tag: [u8; TAG_BYTES],
    pub index: i32,
}

/// What a path says. `bb_kosync_path` in C.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct XPath {
    /// The 1-based `DocFragment[N]`, or -1.
    pub doc_fragment: i32,
    /// The last `/p[N]`, or -1.
    pub paragraph: i32,
    /// The digits after the first `.` following the last `text()` (else the last `.`), or 0.
    pub char_offset: i32,
    /// The N of the last `text()[N]`, at least 1.
    pub text_node: i32,
    /// The steps between `DocFragment[N]/body/` and the text position, at most 16; 0 when the path
    /// has none or is malformed.
    pub step_count: i32,
    pub steps: [Step; MAX_STEPS],
    /// `.../body/text()...`: text directly in `<body>`.
    pub body_text: bool,
    /// A path to the start of the chapter.
    pub chapter_start: bool,
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let rest = hay.get(from..)?;
    if needle.is_empty() {
        return Some(from);
    }
    rest.windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).rposition(|w| w == needle)
}

/// `std::string::rfind(needle, pos)`: the last match starting at or before `pos`.
fn rfind_before(hay: &[u8], needle: &[u8], pos: usize) -> Option<usize> {
    let end = pos.saturating_add(needle.len()).min(hay.len());
    rfind(hay.get(..end)?, needle)
}

fn byte(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

/// Digits `start..end` as a wrapping decimal; `None` at a non-digit.
fn digits(s: &[u8], start: usize, end: usize) -> Option<i32> {
    let mut value = 0i32;
    for &c in s.get(start..end).unwrap_or_default() {
        if !c.is_ascii_digit() {
            return None;
        }
        value = value.wrapping_mul(10).wrapping_add(i32::from(c - b'0'));
    }
    Some(value)
}

fn parse_index(xpath: &[u8], prefix: &[u8], last: bool) -> i32 {
    let pos = if last {
        rfind(xpath, prefix)
    } else {
        find(xpath, prefix, 0)
    };
    let Some(pos) = pos else { return -1 };
    let start = pos + prefix.len();
    let Some(end) = find(xpath, b"]", start) else {
        return -1;
    };
    if end == start {
        return -1;
    }
    digits(xpath, start, end).unwrap_or(-1)
}

fn parse_char_offset(xpath: &[u8]) -> i32 {
    let dot = match rfind(xpath, b"text()") {
        Some(text) => find(xpath, b".", text),
        None => rfind(xpath, b"."),
    };
    let Some(dot) = dot else { return 0 };
    if dot + 1 >= xpath.len() {
        return 0;
    }
    digits(xpath, dot + 1, xpath.len()).unwrap_or(0)
}

fn parse_text_node(xpath: &[u8]) -> i32 {
    let Some(text) = rfind(xpath, b"text()[") else {
        return 1;
    };
    let start = text + 7;
    let Some(end) = find(xpath, b"]", start) else {
        return 1;
    };
    if end == start {
        return 1;
    }
    match digits(xpath, start, end) {
        Some(value) if value > 0 => value,
        _ => 1,
    }
}

fn all_zeros(s: &[u8]) -> bool {
    s.iter().all(|&c| c == b'0')
}

fn is_chapter_start(xpath: &[u8]) -> bool {
    const DOC_FRAGMENT: &[u8] = b"/body/DocFragment[";
    const DOC_BODY: &[u8] = b"]/body";
    if find(xpath, b"/p[", 0).is_some() || find(xpath, b"/li[", 0).is_some() {
        return false;
    }
    let Some(frag) = find(xpath, DOC_FRAGMENT, 0) else {
        return false;
    };
    let Some(frag_end) = find(xpath, b"]", frag + DOC_FRAGMENT.len()) else {
        return false;
    };
    if frag_end + 1 == xpath.len() {
        return true;
    }
    if byte(xpath, frag_end + 1) == b'.' {
        if frag_end + 2 >= xpath.len() {
            return false;
        }
        return all_zeros(xpath.get(frag_end + 2..).unwrap_or_default());
    }
    let Some(body) = find(xpath, DOC_BODY, 0) else {
        return false;
    };
    let mut content = body + DOC_BODY.len();
    if content == xpath.len() {
        return true;
    }
    if byte(xpath, content) != b'/' {
        return false;
    }
    content += 1;
    if content == xpath.len() {
        return true;
    }
    let Some(dot) = rfind(xpath, b".") else {
        return false;
    };
    if dot <= content || dot + 1 >= xpath.len() {
        return false;
    }
    let mut terminal = dot;
    if let Some(text) = rfind_before(xpath, b"/text()", dot)
        && text >= content
    {
        terminal = text;
    }
    if find(xpath, b"/", content).is_some_and(|slash| slash < terminal) {
        return false;
    }
    all_zeros(xpath.get(dot + 1..).unwrap_or_default()) && parse_text_node(xpath) <= 1
}

/// `std::string::compare(pos, n, s) == 0`; false past the end.
fn compare_at(s: &[u8], pos: usize, what: &[u8]) -> bool {
    let end = pos.saturating_add(what.len()).min(s.len());
    s.get(pos..end) == Some(what)
}

fn after_body_slash(xpath: &[u8]) -> Option<usize> {
    const BODY_FRAG: &[u8] = b"/body/DocFragment[";
    let frag = find(xpath, BODY_FRAG, 0)?;
    let after = find(xpath, b"]", frag + BODY_FRAG.len())?;
    compare_at(xpath, after + 1, b"/body/").then_some(after + 1 + 6)
}

fn is_body_text(xpath: &[u8]) -> bool {
    after_body_slash(xpath).is_some_and(|content| compare_at(xpath, content, b"text()"))
}

/// Fills `steps`; their count, 0 when the path has none or is malformed.
fn parse_steps(xpath: &[u8], steps: &mut [Step; MAX_STEPS]) -> i32 {
    let Some(mut pos) = after_body_slash(xpath) else {
        return 0;
    };
    let end = if let Some(end) = rfind(xpath, b"/text()") {
        end
    } else {
        let Some(dot) = rfind(xpath, b".") else {
            return 0;
        };
        if dot <= pos || dot + 1 >= xpath.len() {
            return 0;
        }
        if !xpath
            .get(dot + 1..)
            .unwrap_or_default()
            .iter()
            .all(u8::is_ascii_digit)
        {
            return 0;
        }
        dot
    };
    if end <= pos {
        return 0;
    }
    let mut count = 0;
    while pos < end && count < MAX_STEPS {
        let slash = find(xpath, b"/", pos);
        let seg_end = slash.filter(|&s| s < end).unwrap_or(end);
        let bracket = find(xpath, b"[", pos).filter(|&b| b < seg_end);
        let name_end = bracket.unwrap_or(seg_end);
        let name_len = name_end - pos;
        let Some(step) = steps.get_mut(count) else {
            return 0;
        };
        if name_len == 0 || name_len >= TAG_BYTES {
            return 0;
        }
        step.tag = [0; TAG_BYTES];
        if let (Some(dst), Some(src)) = (step.tag.get_mut(..name_len), xpath.get(pos..name_end)) {
            dst.copy_from_slice(src);
        }
        step.index = match bracket {
            Some(bracket) => {
                let Some(close) = find(xpath, b"]", bracket + 1) else {
                    return 0;
                };
                if close > seg_end {
                    return 0;
                }
                let Some(index) = digits(xpath, bracket + 1, close) else {
                    return 0;
                };
                index
            }
            None => 0,
        };
        count += 1;
        pos = slash.filter(|&s| s < end).map_or(end, |s| s + 1);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    {
        count as i32
    }
}

impl XPath {
    #[must_use]
    pub fn parse(xpath: &[u8]) -> XPath {
        let mut steps = [Step::default(); MAX_STEPS];
        let step_count = parse_steps(xpath, &mut steps);
        XPath {
            doc_fragment: parse_index(xpath, b"/body/DocFragment[", false),
            paragraph: parse_index(xpath, b"/p[", true),
            char_offset: parse_char_offset(xpath),
            text_node: parse_text_node(xpath),
            step_count,
            steps,
            body_text: step_count == 0 && is_body_text(xpath),
            chapter_start: is_chapter_start(xpath),
        }
    }
}

/// What a [`Locator`] looks for.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// How many `<p>` start before byte `target` (`generateXPath`).
    Forward { target: u64 },
    /// `char_offset` codepoints into text node `text_node` of the Nth `<p>` anywhere.
    Paragraph {
        paragraph: i32,
        char_offset: i32,
        text_node: i32,
    },
    /// The same in the element the path's steps name; with `relax`, the first step at any depth
    /// below `<body>`.
    Steps {
        steps: [Step; MAX_STEPS],
        count: i32,
        char_offset: i32,
        text_node: i32,
        relax: bool,
    },
    /// The same in text directly in `<body>`.
    BodyText { char_offset: i32, text_node: i32 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TagState {
    Idle,
    InName,
    Attrs,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AttrState {
    FindName,
    ReadName,
    AfterName,
    BeforeValue,
    CaptureDouble,
    CaptureSingle,
}

/// A chapter being scanned, a byte at a time. `bb_kosync_locator` in C.
#[allow(clippy::struct_excessive_bools)]
pub struct Locator {
    bytes_written: u64,
    in_tag: bool,
    in_entity: bool,
    entity: [u8; ENTITY_BYTES],
    entity_len: usize,
    prev_cr: bool,

    forward_target: u64,
    forward_result: i32,
    forward_captured: bool,

    rev_char: i32,
    found_element: bool,
    done: bool,
    rev_vis: i32,
    total_vis: u64,
    target_vis: u64,

    rev_paragraph: i32,
    p_count: i32,
    paragraph_at_match: i32,
    li_count: i32,
    li_at_match: i32,
    target_text_node: i32,
    current_text_node: i32,
    paragraph_depth: i32,

    steps: [Step; MAX_STEPS],
    step_count: i32,
    sibling_counters: [i32; MAX_STEPS],
    inside_step: [bool; MAX_STEPS],
    html_depth: i32,
    body_depth: i32,
    step_entered_at: [i32; MAX_STEPS],
    relax_first_step: bool,

    tag_state: TagState,
    tag_is_close: bool,
    tag: [u8; TAG_BYTES],
    tag_len: usize,

    matched: i32,

    anchor: [u8; ANCHOR_BYTES],
    anchor_len: usize,
    capturing_anchor: bool,
    attr_state: AttrState,
    attr_name_len: u8,
    attr_is_id: bool,
    in_attr_quote: bool,
    attr_quote: u8,
    non_visible_depth: u8,
    inside_body: bool,
    target_body_text: bool,
}

/// The bytes before the first NUL, as a C string reads.
fn c_str(bytes: &[u8]) -> &[u8] {
    bytes
        .get(..bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len()))
        .unwrap_or_default()
}

fn is_attr_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_attr_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b':' | b'.')
}

fn index(i: i32) -> usize {
    usize::try_from(i).unwrap_or(usize::MAX)
}

impl Locator {
    /// A locator before its mode is set.
    pub const EMPTY: Locator = Locator {
        bytes_written: 0,
        in_tag: false,
        in_entity: false,
        entity: [0; ENTITY_BYTES],
        entity_len: 0,
        prev_cr: false,
        forward_target: u64::MAX,
        forward_result: 0,
        forward_captured: false,
        rev_char: 0,
        found_element: false,
        done: false,
        rev_vis: 0,
        total_vis: 0,
        target_vis: 0,
        rev_paragraph: 0,
        p_count: 0,
        paragraph_at_match: 0,
        li_count: 0,
        li_at_match: 0,
        target_text_node: 1,
        current_text_node: 0,
        paragraph_depth: -1,
        steps: [Step {
            tag: [0; TAG_BYTES],
            index: 0,
        }; MAX_STEPS],
        step_count: 0,
        sibling_counters: [0; MAX_STEPS],
        inside_step: [false; MAX_STEPS],
        html_depth: 0,
        body_depth: -1,
        step_entered_at: [-1; MAX_STEPS],
        relax_first_step: false,
        tag_state: TagState::Idle,
        tag_is_close: false,
        tag: [0; TAG_BYTES],
        tag_len: 0,
        matched: 0,
        anchor: [0; ANCHOR_BYTES],
        anchor_len: 0,
        capturing_anchor: false,
        attr_state: AttrState::FindName,
        attr_name_len: 0,
        attr_is_id: false,
        in_attr_quote: false,
        attr_quote: 0,
        non_visible_depth: 0,
        inside_body: false,
        target_body_text: false,
    };

    #[must_use]
    pub fn new(mode: Mode) -> Self {
        let mut locator = Locator::EMPTY;
        match mode {
            Mode::Forward { target } => locator.start_forward(target),
            Mode::Paragraph {
                paragraph,
                char_offset,
                text_node,
            } => locator.start_paragraph(paragraph, char_offset, text_node),
            Mode::Steps {
                steps,
                count,
                char_offset,
                text_node,
                relax,
            } => locator.start_steps(&steps, count, char_offset, text_node, relax),
            Mode::BodyText {
                char_offset,
                text_node,
            } => locator.start_body_text(char_offset, text_node),
        }
        locator
    }

    /// [`Mode::Forward`], set on an [`EMPTY`](Self::EMPTY) locator.
    pub fn start_forward(&mut self, target: u64) {
        self.forward_target = target;
    }

    /// [`Mode::Paragraph`], set on an [`EMPTY`](Self::EMPTY) locator.
    pub fn start_paragraph(&mut self, paragraph: i32, char_offset: i32, text_node: i32) {
        self.rev_char = char_offset;
        self.rev_paragraph = paragraph;
        self.target_text_node = text_node;
    }

    /// [`Mode::Steps`], set on an [`EMPTY`](Self::EMPTY) locator.
    pub fn start_steps(
        &mut self,
        steps: &[Step; MAX_STEPS],
        count: i32,
        char_offset: i32,
        text_node: i32,
        relax: bool,
    ) {
        self.rev_char = char_offset;
        self.steps = *steps;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let most = MAX_STEPS as i32;
        self.step_count = count.clamp(0, most);
        self.target_text_node = text_node;
        self.relax_first_step = relax;
    }

    /// [`Mode::BodyText`], set on an [`EMPTY`](Self::EMPTY) locator.
    pub fn start_body_text(&mut self, char_offset: i32, text_node: i32) {
        self.rev_char = char_offset;
        self.target_text_node = text_node;
        self.target_body_text = true;
    }

    /// A C string ended where the last name ended, so a '/' among the attributes closes the tag by
    /// that name.
    fn tag_name(&self) -> &[u8] {
        c_str(&self.tag)
    }

    fn tag_is(&self, name: &[u8]) -> bool {
        self.tag_name().eq_ignore_ascii_case(name)
    }

    fn end_name(&mut self) {
        if let Some(slot) = self.tag.get_mut(self.tag_len) {
            *slot = 0;
        }
    }

    fn reset_attr_scan(&mut self) {
        self.attr_state = AttrState::FindName;
        self.attr_name_len = 0;
        self.attr_is_id = false;
    }

    fn finish_anchor(&mut self) {
        if let Some(end) = self.anchor.get_mut(self.anchor_len) {
            *end = 0;
        }
        self.capturing_anchor = false;
        self.reset_attr_scan();
    }

    fn begin_anchor_scan(&mut self) {
        self.capturing_anchor = true;
        self.reset_attr_scan();
    }

    fn end_anchor_scan(&mut self) {
        if self.capturing_anchor {
            self.anchor_len = 0;
        }
        self.capturing_anchor = false;
        self.reset_attr_scan();
    }

    fn append_anchor(&mut self, c: u8) {
        if self.anchor_len + 1 < ANCHOR_BYTES
            && let Some(slot) = self.anchor.get_mut(self.anchor_len)
        {
            *slot = c;
            self.anchor_len += 1;
        }
    }

    fn scan_anchor_attribute(&mut self, c: u8) {
        match self.attr_state {
            AttrState::FindName => {
                if is_attr_name_char(c) {
                    self.attr_state = AttrState::ReadName;
                    self.attr_name_len = 1;
                    self.attr_is_id = c == b'i';
                }
            }
            AttrState::ReadName => {
                if is_attr_name_char(c) {
                    if self.attr_name_len == 1 {
                        self.attr_is_id = self.attr_is_id && c == b'd';
                    } else {
                        self.attr_is_id = false;
                    }
                    self.attr_name_len = self.attr_name_len.wrapping_add(1);
                } else {
                    self.attr_is_id = self.attr_is_id && self.attr_name_len == 2;
                    if is_attr_space(c) {
                        self.attr_state = AttrState::AfterName;
                    } else if c == b'=' {
                        self.attr_state = AttrState::BeforeValue;
                    } else {
                        self.reset_attr_scan();
                    }
                }
            }
            AttrState::AfterName => {
                if is_attr_space(c) {
                } else if c == b'=' {
                    self.attr_state = AttrState::BeforeValue;
                } else if is_attr_name_char(c) {
                    self.attr_state = AttrState::ReadName;
                    self.attr_name_len = 1;
                    self.attr_is_id = c == b'i';
                } else {
                    self.reset_attr_scan();
                }
            }
            AttrState::BeforeValue => {
                if is_attr_space(c) {
                } else if self.attr_is_id && c == b'"' {
                    self.anchor_len = 0;
                    self.attr_state = AttrState::CaptureDouble;
                } else if self.attr_is_id && c == b'\'' {
                    self.anchor_len = 0;
                    self.attr_state = AttrState::CaptureSingle;
                } else if c == b'"' {
                    self.attr_state = AttrState::CaptureDouble;
                } else if c == b'\'' {
                    self.attr_state = AttrState::CaptureSingle;
                } else {
                    self.reset_attr_scan();
                }
            }
            AttrState::CaptureDouble | AttrState::CaptureSingle => {
                let quote = if self.attr_state == AttrState::CaptureDouble {
                    b'"'
                } else {
                    b'\''
                };
                if c == quote {
                    if self.attr_is_id {
                        self.finish_anchor();
                    } else {
                        self.reset_attr_scan();
                    }
                } else if self.attr_is_id {
                    self.append_anchor(c);
                }
            }
        }
    }

    fn on_visible_codepoint(&mut self) {
        self.total_vis += 1;
        if self.found_element && !self.done {
            let in_target = if self.step_count > 0 {
                self.matched == self.step_count
                    && self
                        .step_entered_at
                        .get(index(self.step_count - 1))
                        .copied()
                        == Some(self.html_depth)
                    && self.current_text_node == self.target_text_node
            } else {
                self.paragraph_depth >= 0
                    && self.html_depth == self.paragraph_depth
                    && self.current_text_node == self.target_text_node
            };
            if in_target {
                self.rev_vis = self.rev_vis.wrapping_add(1);
                if self.rev_vis >= self.rev_char {
                    self.target_vis = self.total_vis;
                    self.done = true;
                }
            }
        }
    }

    fn flush_entity_as_literal(&mut self) {
        for _ in 0..self.entity_len {
            self.on_visible_codepoint();
        }
    }

    fn finish_entity(&mut self) {
        let entity = self.entity;
        let text = entity.get(..self.entity_len).unwrap_or_default();
        let value = (text.len() >= 2 && text.first() == Some(&b'&') && text.last() == Some(&b';'))
            .then(|| bembeyaz_html::entity(text.get(1..text.len() - 1).unwrap_or_default()))
            .flatten();
        if let Some(value) = value {
            for _ in bembeyaz_utf8::codepoints(value.text()) {
                self.on_visible_codepoint();
            }
        } else if self.entity_len >= 3 && text.get(1) == Some(&b'#') {
            // A character reference: the XML parser that builds the page table reads one
            // codepoint.
            self.on_visible_codepoint();
        } else {
            self.flush_entity_as_literal();
        }
        self.in_entity = false;
        self.entity_len = 0;
    }

    fn at_match(&mut self) {
        self.found_element = true;
        self.rev_vis = 0;
        self.current_text_node = 1;
        if self.rev_char <= 0 && self.target_text_node <= 1 {
            self.target_vis = self.total_vis;
            self.done = true;
        }
    }

    fn on_legacy_p(&mut self) {
        self.p_count = self.p_count.wrapping_add(1);
        if !self.found_element && self.rev_paragraph > 0 && self.p_count >= self.rev_paragraph {
            self.paragraph_depth = self.html_depth;
            self.at_match();
        }
    }

    fn on_open_tag(&mut self) {
        self.html_depth = self.html_depth.wrapping_add(1);
        if self.tag_is(b"body") {
            self.inside_body = true;
            self.body_depth = self.html_depth;
            if self.target_body_text {
                // Unlike a matched element, the visible count is not reset.
                self.found_element = true;
                self.paragraph_depth = self.html_depth;
                self.current_text_node = 1;
                if self.rev_char <= 0 && self.target_text_node <= 1 {
                    self.target_vis = self.total_vis;
                    self.done = true;
                }
            }
            return;
        }
        if !self.inside_body {
            return;
        }
        if self.non_visible_depth > 0 || bembeyaz_html::is_non_visible_element(self.tag_name()) {
            self.non_visible_depth = self.non_visible_depth.wrapping_add(1);
            return;
        }
        if self.step_count == 0 {
            if self.tag_is(b"p") {
                self.on_legacy_p();
            }
            return;
        }
        // An <a id> inside the matched element is captured even after the target is found.
        if self.found_element
            && self.matched == self.step_count
            && self.anchor_len == 0
            && self.tag_is(b"a")
        {
            self.begin_anchor_scan();
        }
        if self.done {
            return;
        }
        if self.tag_is(b"p") {
            self.p_count = self.p_count.wrapping_add(1);
        }
        if self.tag_is(b"li") {
            self.li_count = self.li_count.wrapping_add(1);
        }
        if self.matched >= self.step_count {
            return;
        }
        let m = index(self.matched);
        let Some(step) = self.steps.get(m).copied() else {
            return;
        };
        if !self.tag_is(c_str(&step.tag)) {
            return;
        }
        // Only direct children of the previous step count; the first at any depth when relaxed.
        let at_depth = if m == 0 {
            self.relax_first_step || self.html_depth == self.body_depth.wrapping_add(1)
        } else {
            self.step_entered_at
                .get(m - 1)
                .is_some_and(|&d| self.html_depth == d.wrapping_add(1))
        };
        if !at_depth {
            return;
        }
        let Some(counter) = self.sibling_counters.get_mut(m) else {
            return;
        };
        *counter = counter.wrapping_add(1);
        if step.index == 0 || *counter == step.index {
            if let Some(inside) = self.inside_step.get_mut(m) {
                *inside = true;
            }
            if let Some(entered) = self.step_entered_at.get_mut(m) {
                *entered = self.html_depth;
            }
            self.matched += 1;
            if self.matched == self.step_count {
                self.begin_anchor_scan();
                self.paragraph_at_match = self.p_count;
                self.li_at_match = self.li_count;
                self.anchor_len = 0;
                self.at_match();
            }
        }
    }

    fn pop_depth(&mut self) {
        if self.html_depth > 0 {
            self.html_depth -= 1;
        }
    }

    fn on_close_tag(&mut self) {
        if self.tag_is(b"body") {
            self.inside_body = false;
            self.pop_depth();
            return;
        }
        if !self.inside_body {
            self.pop_depth();
            return;
        }
        if self.non_visible_depth > 0 {
            self.non_visible_depth -= 1;
            self.pop_depth();
            return;
        }
        let legacy =
            self.step_count == 0 && self.found_element && !self.done && self.paragraph_depth >= 0;
        // Each direct child closing advances the text node.
        if legacy && self.html_depth == self.paragraph_depth.wrapping_add(1) {
            self.next_text_node();
        }
        // Stop tracking when the matched paragraph closes.
        if self.step_count == 0
            && self.found_element
            && !self.done
            && self.paragraph_depth >= 0
            && self.html_depth == self.paragraph_depth
        {
            self.found_element = false;
            self.paragraph_depth = -1;
        }
        if self.step_count > 0
            && self.matched == self.step_count
            && self.found_element
            && !self.done
        {
            let element = self
                .step_entered_at
                .get(index(self.step_count - 1))
                .copied()
                .unwrap_or(-1);
            if self.html_depth == element.wrapping_add(1) {
                self.next_text_node();
            }
        }
        if self.step_count > 0 && self.matched > 0 {
            let step = index(self.matched - 1);
            if self.inside_step.get(step) == Some(&true)
                && self.step_entered_at.get(step) == Some(&self.html_depth)
            {
                if let Some(inside) = self.inside_step.get_mut(step) {
                    *inside = false;
                }
                self.matched -= 1;
                // The matched element closed before the target: give up on it.
                if self.matched < self.step_count && self.found_element && !self.done {
                    self.found_element = false;
                }
                for i in index(self.matched + 1)..index(self.step_count) {
                    if let Some(c) = self.sibling_counters.get_mut(i) {
                        *c = 0;
                    }
                    if let Some(inside) = self.inside_step.get_mut(i) {
                        *inside = false;
                    }
                    if let Some(entered) = self.step_entered_at.get_mut(i) {
                        *entered = -1;
                    }
                }
            }
        }
        self.pop_depth();
    }

    fn next_text_node(&mut self) {
        self.current_text_node = self.current_text_node.wrapping_add(1);
        if self.current_text_node == self.target_text_node && self.rev_char <= 0 {
            self.target_vis = self.total_vis;
            self.done = true;
        }
    }

    fn fire_tag(&mut self) {
        if self.tag_is_close {
            self.on_close_tag();
        } else {
            self.on_open_tag();
        }
    }

    fn byte_in_tag(&mut self, c: u8) {
        match self.tag_state {
            TagState::Idle => {
                if c == b'/' {
                    self.tag_is_close = true;
                    self.tag_state = TagState::InName;
                } else if c != b'!' && c != b'?' {
                    self.tag_is_close = false;
                    if let Some(first) = self.tag.first_mut() {
                        *first = c;
                    }
                    self.tag_len = 1;
                    self.tag_state = TagState::InName;
                }
            }
            TagState::InName => {
                if matches!(c, b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/') {
                    self.end_name();
                    if self.tag_len > 0 {
                        self.fire_tag();
                        // A self-closing open tag (<br/>); a close tag (</br/>) fires once.
                        if c == b'/' && !self.tag_is_close {
                            self.on_close_tag();
                        }
                    }
                    self.tag_len = 0;
                    self.tag_state = if c == b'>' {
                        TagState::Idle
                    } else {
                        TagState::Attrs
                    };
                } else if self.tag_len + 1 < TAG_BYTES
                    && let Some(slot) = self.tag.get_mut(self.tag_len)
                {
                    *slot = c;
                    self.tag_len += 1;
                }
            }
            TagState::Attrs => {
                // A '/' inside a quoted value does not close the tag.
                if !self.in_attr_quote {
                    if c == b'"' || c == b'\'' {
                        self.in_attr_quote = true;
                        self.attr_quote = c;
                    }
                } else if c == self.attr_quote {
                    self.in_attr_quote = false;
                    self.attr_quote = 0;
                }
                if self.capturing_anchor {
                    self.scan_anchor_attribute(c);
                }
                if c == b'/' && !self.in_attr_quote {
                    self.end_anchor_scan();
                    self.on_close_tag();
                }
            }
        }
    }

    /// Reads the next byte of the chapter.
    pub fn byte(&mut self, c: u8) {
        if !self.forward_captured && self.bytes_written >= self.forward_target {
            self.forward_result = self.p_count;
            self.forward_captured = true;
        }
        self.bytes_written += 1;

        if self.in_entity {
            if self.entity_len + 1 < ENTITY_BYTES {
                if let Some(slot) = self.entity.get_mut(self.entity_len) {
                    *slot = c;
                }
                self.entity_len += 1;
            } else {
                self.flush_entity_as_literal();
                self.in_entity = false;
                self.entity_len = 0;
            }
            if self.in_entity {
                if c == b';' {
                    self.finish_entity();
                } else if matches!(c, b'<' | b' ' | b'\t' | b'\n' | b'\r') {
                    self.flush_entity_as_literal();
                    self.in_entity = false;
                    self.entity_len = 0;
                }
            }
            return;
        }

        // Line ends count as the XML parser normalizes them: "\r\n" and "\r" are one "\n".
        let after_cr = self.prev_cr;
        self.prev_cr = false;

        if c == b'<' {
            self.in_tag = true;
            self.tag_state = TagState::Idle;
            self.tag_len = 0;
            self.tag_is_close = false;
            self.capturing_anchor = false;
            self.reset_attr_scan();
            self.in_attr_quote = false;
            self.attr_quote = 0;
        } else if c == b'>' {
            if self.tag_state == TagState::Attrs {
                self.end_anchor_scan();
            }
            self.in_tag = false;
            self.in_attr_quote = false;
            if self.tag_state == TagState::InName && self.tag_len > 0 {
                self.end_name();
                self.fire_tag();
                self.tag_len = 0;
            }
            self.tag_state = TagState::Idle;
        } else if self.in_tag {
            self.byte_in_tag(c);
        } else if !self.inside_body || self.non_visible_depth > 0 {
            // Text outside <body> or in hidden elements does not count.
        } else if c == b'&' {
            self.in_entity = true;
            if let Some(first) = self.entity.first_mut() {
                *first = b'&';
            }
            self.entity_len = 1;
        } else if c == b'\n' && after_cr {
            // The second half of a CRLF: counted on the CR.
        } else {
            if c & 0xC0 != 0x80 {
                self.on_visible_codepoint();
            }
            self.prev_cr = c == b'\r';
        }
    }

    pub fn feed(&mut self, data: &[u8]) {
        for &c in data {
            self.byte(c);
        }
    }

    /// For [`Mode::Forward`]: the `<p>` count at the target byte (or at the end when the chapter
    /// is shorter).
    #[must_use]
    pub fn paragraph_count(&self) -> i32 {
        if self.forward_captured {
            self.forward_result
        } else {
            self.p_count
        }
    }

    #[must_use]
    pub fn found(&self) -> bool {
        self.done
    }

    /// `<p>` up to and including the matched element.
    #[must_use]
    pub fn paragraph_at_match(&self) -> i32 {
        self.paragraph_at_match
    }

    /// `<li>` up to the matched element.
    #[must_use]
    pub fn list_item_at_match(&self) -> i32 {
        self.li_at_match
    }

    /// The `id` of the matched element or an `<a>` in it, if any.
    #[must_use]
    pub fn anchor(&self) -> Option<&[u8]> {
        // A capture cut short leaves no NUL, so this reads on into what an earlier id left, as
        // the C++ did.
        (self.anchor_len > 0).then(|| c_str(&self.anchor))
    }

    #[must_use]
    pub fn total_visible(&self) -> u64 {
        self.total_vis
    }

    #[must_use]
    pub fn target_visible(&self) -> u64 {
        self.target_vis
    }
}
