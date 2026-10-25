//! Exports for `lib/Epub/Epub/ParsedText` (see `rust/include/bembeyaz/layout.h`).
//!
//! A paragraph lives in a Rust heap block that `bb_paragraph_new` hands out and
//! `bb_paragraph_free` takes back.

use alloc::alloc::{Layout, alloc, dealloc};
use alloc::vec::Vec;
use bembeyaz_layout::{
    Align, BlockStyle, Line, LinkSpan, MAX_SIZE_SLOTS, MAX_WORD_FONTS, Metrics, Paragraph,
};
use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use core::slice;

/// `bb_metrics`: the renderer's glyph metrics.
#[repr(C)]
pub struct CMetrics {
    context: *mut c_void,
    text_advance: Option<unsafe extern "C" fn(*mut c_void, c_int, *const c_char, u8, i8) -> c_int>,
    kerning: Option<unsafe extern "C" fn(*mut c_void, c_int, u32, u32, u8, i8) -> c_int>,
    space_advance: Option<unsafe extern "C" fn(*mut c_void, c_int, u32, u32, u8) -> c_int>,
    space_width: Option<unsafe extern "C" fn(*mut c_void, c_int, u8) -> c_int>,
    ascender: Option<unsafe extern "C" fn(*mut c_void, c_int) -> c_int>,
    is_sd_font: Option<unsafe extern "C" fn(*mut c_void, c_int) -> bool>,
    prewarm: Option<
        unsafe extern "C" fn(
            *mut c_void,
            c_int,
            *const *const c_char,
            *const usize,
            usize,
            bool,
            bool,
            u8,
            bool,
        ),
    >,
}

// SAFETY (for every call below): the callbacks are the caller's and take the arguments the
// header documents; text passed to them stays valid for the call.
impl Metrics for &CMetrics {
    fn text_advance(&mut self, font: i32, text: &[u8], style: u8, spacing: i8) -> i32 {
        self.text_advance.map_or(0, |f| unsafe {
            f(self.context, font, text.as_ptr().cast(), style, spacing)
        })
    }
    fn kerning(&mut self, font: i32, left: u32, right: u32, style: u8, spacing: i8) -> i32 {
        self.kerning.map_or(0, |f| unsafe {
            f(self.context, font, left, right, style, spacing)
        })
    }
    fn space_advance(&mut self, font: i32, left: u32, right: u32, style: u8) -> i32 {
        self.space_advance
            .map_or(0, |f| unsafe { f(self.context, font, left, right, style) })
    }
    fn space_width(&mut self, font: i32, style: u8) -> i32 {
        self.space_width
            .map_or(0, |f| unsafe { f(self.context, font, style) })
    }
    fn ascender(&mut self, font: i32) -> i32 {
        self.ascender
            .map_or(0, |f| unsafe { f(self.context, font) })
    }
    fn is_sd_font(&mut self, font: i32) -> bool {
        self.is_sd_font
            .is_some_and(|f| unsafe { f(self.context, font) })
    }
    fn prewarm(
        &mut self,
        font: i32,
        segments: &[&[u8]],
        space: bool,
        hyphen: bool,
        mask: u8,
        small_caps: bool,
    ) {
        let Some(f) = self.prewarm else {
            return;
        };
        let mut pointers = Vec::new();
        let mut lengths = Vec::new();
        if pointers.try_reserve(segments.len()).is_err()
            || lengths.try_reserve(segments.len()).is_err()
        {
            return;
        }
        pointers.extend(segments.iter().map(|s| s.as_ptr().cast::<c_char>()));
        lengths.extend(segments.iter().map(|s| s.len()));
        unsafe {
            f(
                self.context,
                font,
                pointers.as_ptr(),
                lengths.as_ptr(),
                segments.len(),
                space,
                hyphen,
                mask,
                small_caps,
            );
        }
    }
}

/// `bb_block_style`.
#[repr(C)]
pub struct CBlockStyle {
    align: u8,
    text_align_defined: bool,
    direction_defined: bool,
    is_rtl: bool,
    text_indent_defined: bool,
    text_indent: i16,
}

/// `bb_layout_params`.
#[repr(C)]
pub struct CParams {
    font: i32,
    slot_fonts: [i32; MAX_SIZE_SLOTS],
    slot_count: u8,
    width: u16,
    include_last_line: bool,
    character_spacing: i8,
    word_spacing: u8,
    max_lines: usize,
    language: c_int,
    link_count: usize,
}

/// `bb_line`.
#[repr(C)]
pub struct CLine {
    text: *const c_char,
    word_starts: *const u32,
    count: usize,
    xpos: *const i16,
    styles: *const u8,
    focus: *const u8,
    suffix_x: *const u16,
    ruby_text: *const c_char,
    ruby_starts: *const u32,
    /// `bb_link_span`: [`LinkSpan`] is `#[repr(C)]`.
    links: *const LinkSpan,
    link_count: usize,
    ranges: *const u32,
    paragraph_start: u16,
    word_slots: *const u8,
    word_fonts: [i32; MAX_WORD_FONTS],
    visible_offset: u32,
    lines_after: i32,
    is_rtl: bool,
}

// layout.h spells these out: slot_fonts[4], word_fonts[3], bb_link_span.
const _: () = assert!(MAX_WORD_FONTS == 3 && MAX_SIZE_SLOTS == 4);
const _: () = assert!(size_of::<LinkSpan>() == 8 && core::mem::offset_of!(LinkSpan, x) == 2);

/// `l` as a `bb_line`, pointing into its buffers.
pub(crate) fn c_line(l: &Line<'_>) -> CLine {
    CLine {
        text: l.text.as_ptr().cast(),
        word_starts: l.word_starts.as_ptr(),
        count: l.word_starts.len(),
        xpos: l.xpos.as_ptr(),
        styles: l.styles.as_ptr(),
        focus: or_null(l.focus),
        suffix_x: or_null(l.suffix_x),
        ruby_text: l.ruby_text.as_ptr().cast(),
        ruby_starts: l.ruby_starts.as_ptr(),
        links: or_null(l.links),
        link_count: l.links.len(),
        ranges: l.ranges.as_ptr().cast(),
        paragraph_start: l.paragraph_start,
        word_slots: or_null(l.word_slots),
        word_fonts: l.word_fonts,
        visible_offset: l.visible_offset,
        lines_after: l
            .lines_after
            .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX)),
        is_rtl: l.is_rtl,
    }
}

fn or_null<T>(values: &[T]) -> *const T {
    if values.is_empty() {
        ptr::null()
    } else {
        values.as_ptr()
    }
}

unsafe fn bytes<'a>(data: *const c_char, len: usize) -> &'a [u8] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: the caller passes len readable bytes.
        unsafe { slice::from_raw_parts(data.cast(), len) }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bb_paragraph_new(
    hyphenation: bool,
    focus_reading: bool,
    indent_spaces: u8,
    reclaim: Option<bembeyaz_layout::Reclaim>,
) -> *mut Paragraph {
    // SAFETY: Paragraph is not zero-sized.
    let block = unsafe { alloc(Layout::new::<Paragraph>()) }.cast::<Paragraph>();
    if !block.is_null() {
        // SAFETY: a fresh block of the right layout.
        unsafe {
            block.write(Paragraph::new(
                hyphenation,
                focus_reading,
                indent_spaces,
                reclaim,
            ));
        };
    }
    block
}

/// # Safety
/// `paragraph` is from `bb_paragraph_new` and not freed yet, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_free(paragraph: *mut Paragraph) {
    if !paragraph.is_null() {
        // SAFETY: as documented.
        unsafe {
            ptr::drop_in_place(paragraph);
            dealloc(paragraph.cast(), Layout::new::<Paragraph>());
        }
    }
}

/// # Safety
/// `paragraph` is from `bb_paragraph_new` and not in use; `word` points to `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_add_word(
    paragraph: *mut Paragraph,
    word: *const c_char,
    len: usize,
    style: u8,
    underline: bool,
    attach_to_previous: bool,
    offset: u32,
    link: u8,
    slot: u8,
    no_space_before: bool,
) {
    // SAFETY: as documented.
    if let Some(p) = unsafe { paragraph.as_mut() } {
        let word = unsafe { bytes(word, len) };
        p.add_word(
            word,
            style,
            underline,
            attach_to_previous,
            offset,
            link,
            slot,
            no_space_before,
        );
    }
}

/// # Safety
/// As `bb_paragraph_add_word`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_set_ruby(
    paragraph: *mut Paragraph,
    index: usize,
    ruby: *const c_char,
    len: usize,
) {
    // SAFETY: as documented.
    if let Some(p) = unsafe { paragraph.as_mut() } {
        p.set_ruby(index, unsafe { bytes(ruby, len) });
    }
}

/// # Safety
/// As `bb_paragraph_add_word`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_set_ruby_group(
    paragraph: *mut Paragraph,
    start: usize,
    count: usize,
    ruby: *const c_char,
    len: usize,
) {
    // SAFETY: as documented.
    if let Some(p) = unsafe { paragraph.as_mut() } {
        p.set_ruby_group(start, count, unsafe { bytes(ruby, len) });
    }
}

/// # Safety
/// `paragraph` is from `bb_paragraph_new`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_len(paragraph: *const Paragraph) -> usize {
    // SAFETY: as documented.
    unsafe { paragraph.as_ref() }.map_or(0, Paragraph::len)
}

/// # Safety
/// `paragraph` is from `bb_paragraph_new`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_dropped_words(paragraph: *const Paragraph) -> bool {
    // SAFETY: as documented.
    unsafe { paragraph.as_ref() }.is_none_or(Paragraph::dropped_words)
}

/// # Safety
/// `paragraph` is from `bb_paragraph_new`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_word_style(paragraph: *const Paragraph, index: usize) -> u8 {
    // SAFETY: as documented.
    unsafe { paragraph.as_ref() }.map_or(0, |p| p.word_style(index))
}

/// # Safety
/// `paragraph` is from `bb_paragraph_new`, or NULL; `len` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_ruby(
    paragraph: *const Paragraph,
    index: usize,
    len: *mut usize,
) -> *const c_char {
    // SAFETY: as documented.
    let ruby = unsafe { paragraph.as_ref() }.map_or(&[][..], |p| p.ruby_at(index));
    if let Some(len) = unsafe { len.as_mut() } {
        *len = ruby.len();
    }
    ruby.as_ptr().cast()
}

/// # Safety
/// `paragraph` is from `bb_paragraph_new` and not in use, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_suppress_first_line_indent(paragraph: *mut Paragraph) {
    // SAFETY: as documented.
    if let Some(p) = unsafe { paragraph.as_mut() } {
        p.suppress_first_line_indent();
    }
}

/// # Safety
/// `paragraph` is from `bb_paragraph_new` and not in use, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_reset_first_line_indent(paragraph: *mut Paragraph) {
    // SAFETY: as documented.
    if let Some(p) = unsafe { paragraph.as_mut() } {
        p.reset_first_line_indent();
    }
}

/// # Safety
/// `paragraph` is from `bb_paragraph_new` and not in use; `metrics`, `style` and `params` are
/// valid; neither the metrics nor `line` call back into this paragraph.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_paragraph_layout(
    paragraph: *mut Paragraph,
    metrics: *const CMetrics,
    style: *mut CBlockStyle,
    params: *const CParams,
    line: Option<unsafe extern "C" fn(*mut c_void, *const CLine) -> bool>,
    context: *mut c_void,
) {
    // SAFETY: as documented.
    let (Some(p), Some(metrics), Some(style), Some(params), Some(line)) = (
        unsafe { paragraph.as_mut() },
        unsafe { metrics.as_ref() },
        unsafe { style.as_mut() },
        unsafe { params.as_ref() },
        line,
    ) else {
        return;
    };
    let mut block = BlockStyle {
        align: Align::from_index(style.align),
        text_align_defined: style.text_align_defined,
        direction_defined: style.direction_defined,
        is_rtl: style.is_rtl,
        text_indent_defined: style.text_indent_defined,
        text_indent: style.text_indent,
    };
    let layout_params = bembeyaz_layout::Params {
        font: params.font,
        slot_fonts: params.slot_fonts,
        slot_count: params.slot_count,
        width: params.width,
        include_last_line: params.include_last_line,
        character_spacing: params.character_spacing,
        word_spacing: params.word_spacing,
        max_lines: params.max_lines,
        language: crate::hyphenation::language(params.language),
        link_count: params.link_count,
    };
    let mut emit = |l: &Line<'_>| {
        let c_line = c_line(l);
        // SAFETY: the caller's callback; the line's buffers stay valid for the call.
        unsafe { line(context, &raw const c_line) }
    };
    let mut m = metrics;
    p.layout(&mut m, &mut block, &layout_params, &mut emit);
    style.is_rtl = block.is_rtl;
}
