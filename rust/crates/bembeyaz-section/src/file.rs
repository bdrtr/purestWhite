//! The section file's header and tables around its pages (`test/section_file` pins them). Packed,
//! little-endian:
//!
//! - the header, 44 bytes: the version (u8), the [`Spec`] it was built with, the page count (u16),
//!   then where the page LUT, the anchor map, the paragraph LUT, the list-item LUT and the
//!   visible-offset LUT start (u32 each);
//! - the pages ([`crate::page`]);
//! - the page LUT: each page's position (u32);
//! - the anchor map: a count (u16), then each anchor's name (u32 length, bytes) and page (u16);
//! - the paragraph LUT: a count (u16), then each page's paragraph index (u16);
//! - the list-item LUT: each page's list-item index (u16);
//! - the visible-offset LUT: the visible codepoint each page starts at (u32);
//! - for a build suspended partway, its parse watermark: bytes consumed and total bytes (u32 each).
//!
//! A build writes [`INCOMPLETE_VERSION`] first and the real version last, as its commit point.

// v28: text decoration bits now include line-through in serialized wordStyles.
// v29: TextBlock word data stored as one flat arena (offset table + NUL-terminated
// text blob) instead of length-prefixed strings and per-field arrays.
// v30: Arabic shaping changed both drawing and measurement (getTextAdvanceX now
//      measures the shaped visual text); cached word positions from v29 no longer
//      match what drawText renders.
// v32: ImageBlock serializes the book-internal source href after the cache path
//      (lazy extraction: images are header-probed at build time and extracted on
//      first render).
// v33: Support <ruby> and <rt> tags. Skip <rp> tags
// v34: Word gaps are only suppressed for tokens glued in the source, so spaces between
//      Hangul words survive again; ruby element boundaries carry the continuation flag
//      instead. Invalidates v33 caches, whose word positions have the spaces collapsed.
// v34: <br> handling changed layout — a <br> after text is now a margin-stripped
//      line break (browser-like) and only a <br> whose block stays empty injects
//      the scene-break gap, so cached pages laid out by older versions no longer
//      match. Keeps <br>-per-paragraph books (common CJK formatting) from
//      re-adding container spacing at every paragraph.
// v35: Persist a uint32_t visible-text start offset for every page.
// v36: Ruby and CJK justification layout changes invalidate cached word positions.
// v37: Footnote href records grew from 96 to 256 bytes.
// v38: Focus Reading line breaking changed — a visible hyphen/dash inside a word is now a
//      break opportunity, and hyphenation of a focus-split word considers the whole word
//      instead of only its regular-weight suffix. Pages cached by older versions were laid
//      out with the previous, more restrictive break set and no longer match.
// v39: Image top margin is clamped so a full-viewport-height image cannot
//      overflow the page bottom; older caches can hold placements that panels
//      with no bottom inset refuse to draw.
// v40: Ruby groups remain intact when a large text block is soft-flushed.
// v41: Simple HTML table rows are laid out as positioned columns instead of
//      flattened paragraphs with synthetic row/cell labels.
// v42: Closing a block strips inherited vertical margins and padding.
// v43: Paragraph base direction excludes direction changes from inline elements.
// v44: Persist internal-link rectangles with each page for touch navigation.
// v45: Internal EPUB links preserve CSS superscript/subscript positioning.
// v46: Ordered lists number their items, list-style-type: none suppresses markers,
//      and <ul>/<ol> containers contribute their own margins/padding to child insets.
// v47: Word and character spacing in the header (cache validation); cached BlockStyle stores only character spacing.
// v48: Hangul words wrap at spaces; with hyphenation on they may also split at a line end.
//      Justification no longer stretches between syllables.
// v49 was used by pre-release builds with a different header layout.
// v50: Paragraph indentation width in the header for cache validation.
// v51: Preserve paragraph continuity and top spacing across soft flushes.
// v52: Missing full-block and black-square symbols now have visible widths.
// v53: Persist per-word source ranges alongside the redaction layout changes.
// v54: Source ranges retain codepoints absorbed by NFC composition.
// v55: Persist the first logical word of each paragraph for clipping separators.
// v56: Rendering layout (fonts per word, drop caps, border boxes).
// v57: Includes explicit hyphen/dash line breaks and invalidates both v56 layouts.
// v58: Rebuild sections for corrected rendering layout and footnote placement.
// v59: Preserve stored CSS styles during low-heap layout.
// v60: Text running into a block or heading start stays in its own paragraph.
// A bump changes section.h's BB_SECTION_VERSION and BB_SECTION_PARTIAL_VERSION too.
pub const VERSION: u8 = 60;
/// A suspended build's file: its pages so far and the watermark. Changes with [`VERSION`].
pub const PARTIAL_VERSION: u8 = 0xFE - (VERSION - 28);
pub const INCOMPLETE_VERSION: u8 = 0;
pub const HEADER_SIZE: u32 = 44;

use crate::{Sink, Source};
use bembeyaz_io::Out;

const HEADER: usize = HEADER_SIZE as usize;
const PAGE_COUNT_AT: u32 = 22;
const LUT_AT: u32 = 24;
const ANCHORS_AT: u32 = 28;
const PARAGRAPHS_AT: u32 = 32;
const LIST_ITEMS_AT: u32 = 36;
const VISIBLE_AT: u32 = 40;
// LUT entries read at a time.
const BATCH: u32 = 32;

/// The layout settings a section was built with: any change rebuilds it. `bb_section_spec` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Spec {
    pub font_id: i32,
    pub line_compression: f32,
    pub extra_paragraph_spacing: bool,
    pub paragraph_indent_spaces: u8,
    pub paragraph_alignment: u8,
    pub viewport_width: u16,
    pub viewport_height: u16,
    pub hyphenation_enabled: bool,
    pub embedded_style: bool,
    pub image_rendering: u8,
    pub focus_reading_enabled: bool,
    pub character_spacing: i8,
    pub word_spacing_percent: u8,
}

/// A built page: where it is and where it starts in the text. `bb_section_lut_entry` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct LutEntry {
    pub position: u32,
    pub paragraph: u16,
    pub list_item: u16,
    pub visible: u32,
}

/// How far a suspended build had parsed its HTML. `bb_section_watermark` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Watermark {
    pub consumed: u32,
    pub total: u32,
}

fn put(bytes: &mut [u8], at: usize, value: &[u8]) {
    if let Some(slot) = bytes.get_mut(at..at + value.len()) {
        slot.copy_from_slice(value);
    }
}

/// The header a build starts with: [`INCOMPLETE_VERSION`], `spec`, and zero for the count and
/// the tables.
#[must_use]
pub fn header(spec: &Spec) -> [u8; HEADER] {
    let mut h = [0u8; HEADER];
    put(&mut h, 1, &spec.font_id.to_le_bytes());
    put(&mut h, 5, &spec.line_compression.to_le_bytes());
    put(
        &mut h,
        9,
        &[
            u8::from(spec.extra_paragraph_spacing),
            spec.paragraph_indent_spaces,
            spec.paragraph_alignment,
        ],
    );
    put(&mut h, 12, &spec.viewport_width.to_le_bytes());
    put(&mut h, 14, &spec.viewport_height.to_le_bytes());
    put(
        &mut h,
        16,
        &[
            u8::from(spec.hyphenation_enabled),
            u8::from(spec.embedded_style),
            spec.image_rendering,
            u8::from(spec.focus_reading_enabled),
            spec.character_spacing.to_le_bytes()[0],
            spec.word_spacing_percent,
        ],
    );
    h
}

// `base + index * size`, or None when it overflows u32.
fn entry(base: u32, index: u32, size: u32) -> Option<u32> {
    base.checked_add(index.checked_mul(size)?)
}

fn read<const N: usize>(file: &mut impl Source, offset: Option<u32>) -> Option<[u8; N]> {
    let mut b = [0u8; N];
    offset.is_some_and(|o| file.read_at(o, &mut b)).then_some(b)
}

// A field that cannot be read is zero.
fn u8_at(file: &mut impl Source, offset: u32) -> u8 {
    read::<1>(file, Some(offset)).map_or(0, |b| b[0])
}

fn u16_at(file: &mut impl Source, offset: Option<u32>) -> u16 {
    read(file, offset).map_or(0, u16::from_le_bytes)
}

fn u32_at(file: &mut impl Source, offset: Option<u32>) -> u32 {
    read(file, offset).map_or(0, u32::from_le_bytes)
}

// Whether `start .. start + len` lies in a file of `size` bytes.
fn fits(start: u32, len: u64, size: u32) -> bool {
    u64::from(start) + len <= u64::from(size)
}

/// Why [`check`] turned a file down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refusal {
    Version(u8),
    Spec,
    /// A partial file whose watermark cannot be where it should be.
    Partial,
}

/// A file [`check`] accepted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Loaded {
    pub page_count: u16,
    /// Set for a partial file.
    pub watermark: Option<Watermark>,
}

/// Checks that the file is a finished or partial section built with `spec`.
///
/// # Errors
///
/// Another version, another spec, or a partial file without its watermark.
pub fn check(file: &mut impl Source, spec: &Spec) -> Result<Loaded, Refusal> {
    let size = file.size();
    let len = HEADER.min(size as usize);
    let mut h = [0u8; HEADER];
    if !h.get_mut(..len).is_some_and(|b| file.read_at(0, b)) {
        h = [0; HEADER];
    }
    let field = |at: usize, n: usize| h.get(at..at + n).filter(|_| at + n <= len);
    let version = field(0, 1).and_then(|b| b.first().copied()).unwrap_or(0);
    if version != VERSION && version != PARTIAL_VERSION {
        return Err(Refusal::Version(version));
    }
    let expected = header(spec);
    let line_compression = field(5, 4)
        .and_then(|b| b.try_into().ok())
        .map_or(0.0, f32::from_le_bytes);
    // Field by field: one the file is too short for reads as zero, not as its first bytes.
    let same = |at: usize, n: usize| {
        let want = expected.get(at..at + n);
        match field(at, n) {
            Some(got) => Some(got) == want,
            None => want.is_some_and(|w| w.iter().all(|&b| b == 0)),
        }
    };
    // Exactly the value it was built with.
    #[allow(clippy::float_cmp)]
    let spec_matches = same(1, 4)
        && line_compression == spec.line_compression
        && [(9, 1), (10, 1), (11, 1), (12, 2), (14, 2)]
            .iter()
            .chain(&[(16, 1), (17, 1), (18, 1), (19, 1), (20, 1), (21, 1)])
            .all(|&(at, n)| same(at, n));
    if !spec_matches {
        return Err(Refusal::Spec);
    }
    let u16_field = |at: usize| {
        field(at, 2)
            .and_then(|b| b.try_into().ok())
            .map_or(0, u16::from_le_bytes)
    };
    let u32_field = |at: usize| {
        field(at, 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, u32::from_le_bytes)
    };
    let page_count = u16_field(PAGE_COUNT_AT as usize);
    if version == VERSION {
        return Ok(Loaded {
            page_count,
            watermark: None,
        });
    }
    let list_items = u32_field(LIST_ITEMS_AT as usize);
    let visible = u32_field(VISIBLE_AT as usize);
    let trailer = u64::from(visible) + u64::from(page_count) * 4;
    let valid = page_count > 0
        && list_items >= HEADER_SIZE
        && visible > list_items
        && trailer + 8 <= u64::from(size);
    if !valid {
        return Err(Refusal::Partial);
    }
    let trailer = u32::try_from(trailer).ok();
    Ok(Loaded {
        page_count,
        watermark: Some(Watermark {
            consumed: u32_at(file, trailer),
            total: u32_at(file, trailer.and_then(|t| t.checked_add(4))),
        }),
    })
}

/// A finished section's page count; a partial's count is only how far its build got.
#[must_use]
pub fn finished_page_count(file: &mut impl Source) -> Option<u16> {
    if file.size() < HEADER_SIZE || u8_at(file, 0) != VERSION {
        return None;
    }
    Some(u16_at(file, Some(PAGE_COUNT_AT)))
}

/// Where a page is, and the visible offset it starts at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PageAt {
    pub position: u32,
    /// Zero when the visible-offset LUT does not hold the page.
    pub visible: u32,
}

/// Where `page` is. None when its LUT entry cannot be read or points into the header.
#[must_use]
pub fn page(file: &mut impl Source, page: u16) -> Option<PageAt> {
    let size = file.size();
    let lut = u32_at(file, Some(LUT_AT));
    let position = u32_at(file, entry(lut, u32::from(page), 4));
    if position < HEADER_SIZE {
        return None;
    }
    let visible_lut = u32_at(file, Some(VISIBLE_AT));
    let at = entry(visible_lut, u32::from(page), 4).filter(|&at| fits(at, 4, size));
    let visible = if visible_lut >= HEADER_SIZE && at.is_some() {
        u32_at(file, at)
    } else {
        0
    };
    Some(PageAt { position, visible })
}

// The version, when it is a finished or a partial section.
fn readable_version(file: &mut impl Source) -> Option<u8> {
    if file.size() < HEADER_SIZE {
        return None;
    }
    let version = u8_at(file, 0);
    (version == VERSION || version == PARTIAL_VERSION).then_some(version)
}

/// The visible offset `page` starts at.
#[must_use]
pub fn visible_offset(file: &mut impl Source, page: u16) -> Option<u32> {
    readable_version(file)?;
    if page >= u16_at(file, Some(PAGE_COUNT_AT)) {
        return None;
    }
    let lut = u32_at(file, Some(VISIBLE_AT));
    let at = entry(lut, u32::from(page), 4)?;
    if lut < HEADER_SIZE || !fits(at, 4, file.size()) {
        return None;
    }
    Some(u32_at(file, Some(at)))
}

/// Finds the page holding a visible offset from the pages' start offsets, given in order.
pub struct OffsetScan {
    offset: u32,
    prefer_first: bool,
    next: u16,
    result: u16,
    last: u32,
}

impl OffsetScan {
    /// The last page starting at or before `offset`, or with `prefer_first` the first page
    /// starting exactly there (of the pages sharing a start offset, all but the last hold no
    /// text, e.g. an image).
    #[must_use]
    pub fn new(offset: u32, prefer_first: bool) -> Self {
        Self {
            offset,
            prefer_first,
            next: 0,
            result: 0,
            last: 0,
        }
    }

    /// Takes the next page's start; the page, once it is known.
    pub fn push(&mut self, start: u32) -> Option<u16> {
        self.last = start;
        if start > self.offset {
            return Some(self.result);
        }
        self.result = self.next;
        self.next = self.next.wrapping_add(1);
        (self.prefer_first && start == self.offset).then_some(self.result)
    }

    /// The page once every start was pushed, and the last start.
    #[must_use]
    pub fn finish(&self) -> (u16, u32) {
        (self.result, self.last)
    }
}

/// The page holding visible offset `offset` (see [`OffsetScan`]). None when there are no
/// pages, or past the last page's start of a partial file, whose later pages are not built yet.
#[must_use]
pub fn page_for_visible_offset(
    file: &mut impl Source,
    offset: u32,
    prefer_first: bool,
) -> Option<u16> {
    let partial = readable_version(file)? == PARTIAL_VERSION;
    let count = u16_at(file, Some(PAGE_COUNT_AT));
    let lut = u32_at(file, Some(VISIBLE_AT));
    if count == 0 || lut < HEADER_SIZE || !fits(lut, u64::from(count) * 4, file.size()) {
        return None;
    }
    let mut scan = OffsetScan::new(offset, prefer_first);
    let mut buf = [0u8; BATCH as usize * 4];
    let mut base = 0u32;
    while base < u32::from(count) {
        let batch = BATCH.min(u32::from(count) - base);
        let bytes = buf.get_mut(..batch as usize * 4)?;
        if !file.read_at(entry(lut, base, 4)?, bytes) {
            return None;
        }
        for start in bytes.as_chunks::<4>().0 {
            if let Some(page) = scan.push(u32::from_le_bytes(*start)) {
                return Some(page);
            }
        }
        base += batch;
    }
    let (result, last) = scan.finish();
    (!partial || offset <= last).then_some(result)
}

// The paragraph LUT: where its entries start and how many.
fn paragraph_lut(file: &mut impl Source) -> Option<(u32, u16)> {
    let at = u32_at(file, Some(PARAGRAPHS_AT));
    if at == 0 || at >= file.size() {
        return None;
    }
    Some((at.checked_add(2)?, u16_at(file, Some(at))))
}

// The first of `count` u16 entries from `start` that reaches `target`, else the last.
fn first_reaching(file: &mut impl Source, start: u32, count: u16, target: u16) -> Option<u16> {
    let mut buf = [0u8; BATCH as usize * 2];
    let mut base = 0u32;
    while base < u32::from(count) {
        let batch = BATCH.min(u32::from(count) - base);
        let bytes = buf.get_mut(..batch as usize * 2)?;
        if !file.read_at(entry(start, base, 2)?, bytes) {
            return None;
        }
        if let Some(i) = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .position(|v| u16::from_le_bytes(*v) >= target)
        {
            return u16::try_from(base as usize + i).ok();
        }
        base += batch;
    }
    Some(count - 1)
}

/// The first page whose paragraph index reaches `target`, else the last page.
#[must_use]
pub fn page_for_paragraph(file: &mut impl Source, target: u16) -> Option<u16> {
    let (start, count) = paragraph_lut(file)?;
    if count == 0 || !fits(start, u64::from(count) * 2, file.size()) {
        return None;
    }
    first_reaching(file, start, count, target)
}

/// The paragraph index `page` starts in.
#[must_use]
pub fn paragraph_for_page(file: &mut impl Source, page: u16) -> Option<u16> {
    let (start, count) = paragraph_lut(file)?;
    let at = entry(start, u32::from(page), 2)?;
    if count == 0 || page >= count || !fits(at, 2, file.size()) {
        return None;
    }
    Some(u16_at(file, Some(at)))
}

/// The first page whose list-item index reaches `target`, else the last page.
#[must_use]
pub fn page_for_list_item(file: &mut impl Source, target: u16) -> Option<u16> {
    let size = file.size();
    let start = u32_at(file, Some(LIST_ITEMS_AT));
    if start == 0 || start >= size {
        return None;
    }
    // The list-item LUT has the paragraph LUT's count.
    let (_, count) = paragraph_lut(file)?;
    if count == 0 || !fits(start, u64::from(count) * 2, size) {
        return None;
    }
    first_reaching(file, start, count, target)
}

// Walks the anchor map until `visit` (given each name's offset and length; its page follows it)
// says to stop or an entry cannot be read.
fn walk<F: Source>(file: &mut F, mut visit: impl FnMut(&mut F, u32, u32) -> bool) {
    let size = file.size();
    let map = u32_at(file, Some(ANCHORS_AT));
    if map == 0 || map >= size {
        return;
    }
    let count = u16_at(file, Some(map));
    let mut at = map.checked_add(2);
    for _ in 0..count {
        let Some(len) = read(file, at).map(u32::from_le_bytes) else {
            return;
        };
        let Some(name) = at.and_then(|a| a.checked_add(4)) else {
            return;
        };
        if !fits(name, u64::from(len), size) {
            return;
        }
        if visit(file, name, len) {
            return;
        }
        at = (name + len).checked_add(2);
    }
}

// Whether the file holds `name` at `at`.
fn holds(file: &mut impl Source, at: u32, name: &[u8]) -> bool {
    let mut buf = [0u8; 64];
    let mut offset = at;
    for part in name.chunks(buf.len()) {
        let Some(got) = buf.get_mut(..part.len()) else {
            return false;
        };
        if !file.read_at(offset, got) || got != part {
            return false;
        }
        offset = offset.wrapping_add(64);
    }
    true
}

/// The page of the anchor called `name`.
#[must_use]
pub fn find_anchor(file: &mut impl Source, name: &[u8]) -> Option<u16> {
    let mut found = None;
    walk(file, |file, at, len| {
        if len as usize == name.len() && holds(file, at, name) {
            // A page the file is cut short of reads as zero.
            found = Some(u16_at(file, Some(at + len)));
        }
        found.is_some()
    });
    found
}

/// Hands every anchor whose name and page fit `buf` (name bytes, then the page) to `visit`, in
/// order. A name too long for `buf` is skipped: size it to the longest name sought.
pub fn anchors(file: &mut impl Source, buf: &mut [u8], mut visit: impl FnMut(&[u8], u16)) {
    walk(file, |file, at, n| {
        let len = n as usize;
        let Some(entry) = buf.get_mut(..len + 2) else {
            return false;
        };
        if file.read_at(at, entry)
            && let (name, [low, high]) = entry.split_at(len)
        {
            visit(name, u16::from_le_bytes([*low, *high]));
        } else if let Some(name) = entry.get_mut(..len)
            && file.read_at(at, name)
        {
            // A page the file is cut short of reads as zero.
            visit(name, 0);
        }
        false
    });
}

/// Why [`commit`] failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommitError {
    /// A page the build could not write.
    MissingPage,
    Write,
}

/// Finishes a build's file: writes the tables from `at`, the end of its pages, with `entries`
/// (one per built page) and the `anchor_count` anchors `anchor` gives (for a partial file, those
/// on its pages only); then the header's page count and table offsets; and last the version,
/// finished or (with a `watermark`) partial, the commit point.
///
/// # Errors
///
/// A page the build failed to write (position 0), or a write that failed.
pub fn commit<'a, S: Sink>(
    sink: &mut S,
    at: u32,
    entries: &[LutEntry],
    anchor_count: usize,
    anchor: impl Fn(usize) -> (&'a [u8], u16),
    watermark: Option<Watermark>,
) -> Result<(), CommitError> {
    if entries.iter().any(|e| e.position == 0) {
        return Err(CommitError::MissingPage);
    }
    let page_count = u16::try_from(entries.len()).unwrap_or(u16::MAX);
    let kept = |page: u16| watermark.is_none() || page < page_count;
    let mut out = Out::new(sink, at);
    let lut = out.position();
    for e in entries {
        out.push(&e.position.to_le_bytes());
    }
    let anchors = out.position();
    let count = (0..anchor_count)
        .filter(|&i| kept(anchor(i).1))
        .count()
        .min(usize::from(u16::MAX));
    out.push(&u16::try_from(count).unwrap_or(u16::MAX).to_le_bytes());
    for (name, page) in (0..anchor_count)
        .map(&anchor)
        .filter(|&(_, page)| kept(page))
        .take(count)
    {
        out.push(&u32::try_from(name.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.push(name);
        out.push(&page.to_le_bytes());
    }
    let paragraphs = out.position();
    out.push(&page_count.to_le_bytes());
    for e in entries {
        out.push(&e.paragraph.to_le_bytes());
    }
    let list_items = out.position();
    for e in entries {
        out.push(&e.list_item.to_le_bytes());
    }
    let visible = out.position();
    for e in entries {
        out.push(&e.visible.to_le_bytes());
    }
    if let Some(w) = watermark {
        out.push(&w.consumed.to_le_bytes());
        out.push(&w.total.to_le_bytes());
    }
    out.flush();
    let (Some(lut), Some(anchors), Some(paragraphs), Some(list_items), Some(visible), true) =
        (lut, anchors, paragraphs, list_items, visible, out.ok())
    else {
        return Err(CommitError::Write);
    };
    let mut patch = [0u8; (HEADER_SIZE - PAGE_COUNT_AT) as usize];
    put(&mut patch, 0, &page_count.to_le_bytes());
    for (i, offset) in [lut, anchors, paragraphs, list_items, visible]
        .iter()
        .enumerate()
    {
        put(&mut patch, 2 + i * 4, &offset.to_le_bytes());
    }
    let version = if watermark.is_some() {
        PARTIAL_VERSION
    } else {
        VERSION
    };
    if !sink.write_at(PAGE_COUNT_AT, &patch) || !sink.write_at(0, &[version]) {
        return Err(CommitError::Write);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::cast_possible_truncation
    )]

    use super::{
        CommitError, HEADER_SIZE, Loaded, LutEntry, OffsetScan, PARTIAL_VERSION, Refusal, Spec,
        VERSION, Watermark, anchors, check, commit, find_anchor, finished_page_count, header, page,
        page_for_list_item, page_for_paragraph, page_for_visible_offset, paragraph_for_page,
        visible_offset,
    };
    use crate::Sink;
    use std::vec;
    use std::vec::Vec;

    use bembeyaz_io::testing::Bytes;

    fn spec() -> Spec {
        Spec {
            font_id: 7,
            line_compression: 1.0,
            paragraph_indent_spaces: 2,
            viewport_width: 200,
            viewport_height: 120,
            word_spacing_percent: 100,
            ..Spec::default()
        }
    }

    // Three 10-byte pages after the header, and the anchors "a" (page 0), "bc" (page 2) and "d"
    // (page 3, still being laid out when a build is suspended).
    fn built(watermark: Option<Watermark>) -> Bytes {
        let mut file = Bytes::new(header(&spec()).to_vec());
        file.data.extend([0xEE; 30]);
        let entries: Vec<LutEntry> = (0..3)
            .map(|i| LutEntry {
                position: HEADER_SIZE + 10 * i,
                paragraph: [0, 2, 5][i as usize],
                list_item: [0, 0, 1][i as usize],
                visible: [0, 40, 40][i as usize],
            })
            .collect();
        let names: [(&[u8], u16); 3] = [(b"a", 0), (b"bc", 2), (b"d", 3)];
        commit(
            &mut file,
            HEADER_SIZE + 30,
            &entries,
            3,
            |i| names[i],
            watermark,
        )
        .unwrap();
        file
    }

    #[test]
    fn a_committed_file_reads_back() {
        let mut file = built(None);
        assert_eq!(
            check(&mut file, &spec()),
            Ok(Loaded {
                page_count: 3,
                watermark: None
            })
        );
        assert_eq!(finished_page_count(&mut file), Some(3));
        assert_eq!(
            page(&mut file, 1).map(|p| (p.position, p.visible)),
            Some((HEADER_SIZE + 10, 40))
        );
        assert_eq!(visible_offset(&mut file, 2), Some(40));
        assert_eq!(visible_offset(&mut file, 3), None);
        assert_eq!(page_for_visible_offset(&mut file, 40, false), Some(2));
        assert_eq!(page_for_visible_offset(&mut file, 40, true), Some(1));
        assert_eq!(page_for_visible_offset(&mut file, 39, false), Some(0));
        assert_eq!(page_for_paragraph(&mut file, 3), Some(2));
        assert_eq!(page_for_paragraph(&mut file, 9), Some(2));
        assert_eq!(paragraph_for_page(&mut file, 1), Some(2));
        assert_eq!(page_for_list_item(&mut file, 1), Some(2));
        assert_eq!(find_anchor(&mut file, b"bc"), Some(2));
        assert_eq!(find_anchor(&mut file, b"b"), None);
        let mut seen = Vec::new();
        anchors(&mut file, &mut [0; 4], |name, page| {
            seen.push((name.to_vec(), page));
        });
        assert_eq!(
            seen,
            [(b"a".to_vec(), 0), (b"bc".to_vec(), 2), (b"d".to_vec(), 3)]
        );
        // A name too long for the buffer is left out.
        seen.clear();
        anchors(&mut file, &mut [0; 3], |name, page| {
            seen.push((name.to_vec(), page));
        });
        assert_eq!(seen, [(b"a".to_vec(), 0), (b"d".to_vec(), 3)]);

        let mut other = spec();
        other.viewport_height += 1;
        assert_eq!(check(&mut file, &other), Err(Refusal::Spec));
        file.data[0] = 0;
        assert_eq!(check(&mut file, &spec()), Err(Refusal::Version(0)));
    }

    #[test]
    fn a_partial_file_keeps_its_watermark_and_pages() {
        let watermark = Watermark {
            consumed: 100,
            total: 400,
        };
        let mut file = built(Some(watermark));
        assert_eq!(file.data[0], PARTIAL_VERSION);
        assert_eq!(
            check(&mut file, &spec()),
            Ok(Loaded {
                page_count: 3,
                watermark: Some(watermark)
            })
        );
        assert_eq!(finished_page_count(&mut file), None);
        // Anchors past its pages are left out.
        assert_eq!(find_anchor(&mut file, b"bc"), Some(2));
        assert_eq!(find_anchor(&mut file, b"d"), None);
        assert_eq!(page_for_visible_offset(&mut file, 40, false), Some(2));
        assert_eq!(page_for_visible_offset(&mut file, 41, false), None);
        file.data.truncate(file.data.len() - 1);
        assert_eq!(check(&mut file, &spec()), Err(Refusal::Partial));
    }

    #[test]
    fn short_fields_read_as_zero() {
        let mut file = built(None);
        file.data.truncate(23);
        assert_eq!(check(&mut file, &spec()).map(|l| l.page_count), Ok(0));
        // A spec field the file is too short for matches only a zero spec field.
        file.data.truncate(13);
        assert_eq!(check(&mut file, &spec()), Err(Refusal::Spec));
        let zero = Spec {
            font_id: 7,
            line_compression: 1.0,
            paragraph_indent_spaces: 2,
            ..Spec::default()
        };
        file.data.truncate(12);
        assert_eq!(check(&mut file, &zero).map(|l| l.page_count), Ok(0));
        assert_eq!(finished_page_count(&mut file), None);
    }

    #[test]
    fn scans_offsets_in_order() {
        let mut scan = OffsetScan::new(10, false);
        assert_eq!([0, 10, 10].map(|s| scan.push(s)), [None, None, None]);
        assert_eq!(scan.push(11), Some(2));
        let mut first = OffsetScan::new(10, true);
        assert_eq!([0, 10].map(|s| first.push(s)), [None, Some(1)]);
    }

    struct Failing;
    impl Sink for Failing {
        fn write_at(&mut self, _: u32, _: &[u8]) -> bool {
            false
        }
    }

    #[test]
    fn a_missing_page_or_failed_write_fails_the_commit() {
        let entries = [LutEntry {
            position: 0,
            ..LutEntry::default()
        }];
        let none = |_: usize| (&b""[..], 0);
        assert_eq!(
            commit(&mut Bytes::new(Vec::new()), 44, &entries, 0, none, None),
            Err(CommitError::MissingPage)
        );
        let entries = [LutEntry {
            position: 44,
            ..LutEntry::default()
        }];
        assert_eq!(
            commit(&mut Failing, 44, &entries, 0, none, None),
            Err(CommitError::Write)
        );
    }

    #[test]
    fn any_file_never_panics() {
        let mut seed = 0x5ec7_105e_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let good = built(None).data;
        for _ in 0..2000 {
            let mut bytes = good.clone();
            for _ in 0..=next() % 6 {
                let at = next() as usize % bytes.len();
                bytes[at] = next() as u8;
            }
            if next() % 3 == 0 {
                bytes.truncate(next() as usize % bytes.len());
            }
            if let Some(v) = bytes.first_mut() {
                *v = if next() % 2 == 0 {
                    VERSION
                } else {
                    PARTIAL_VERSION
                };
            }
            let mut file = Bytes::new(bytes);
            let _ = check(&mut file, &spec());
            let _ = finished_page_count(&mut file);
            let p = next() as u16;
            let _ = page(&mut file, p % 8);
            let _ = visible_offset(&mut file, p);
            let _ = page_for_visible_offset(&mut file, next(), next() % 2 == 0);
            let _ = page_for_paragraph(&mut file, p);
            let _ = paragraph_for_page(&mut file, p % 8);
            let _ = page_for_list_item(&mut file, p);
            let _ = find_anchor(&mut file, b"bc");
            anchors(&mut file, &mut [0; 8], |_, _| {});
        }
        let names = vec![(vec![b'x'; 300], 1u16); 3];
        let mut file = Bytes::new(header(&spec()).to_vec());
        let entries = [LutEntry {
            position: 44,
            ..LutEntry::default()
        }];
        commit(
            &mut file,
            44,
            &entries,
            3,
            |i| (&names[i].0[..], names[i].1),
            None,
        )
        .unwrap();
        let mut handed = 0;
        anchors(&mut file, &mut [0; 302], |name, page| {
            assert_eq!((name, page), (&names[0].0[..], 1));
            handed += 1;
        });
        assert_eq!(handed, 3);
        assert_eq!(find_anchor(&mut file, &names[0].0), Some(1));
    }
}
