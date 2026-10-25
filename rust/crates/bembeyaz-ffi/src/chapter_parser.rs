//! Exports for the chapter parser (see `rust/include/bembeyaz/chapter_parser.h`).

use crate::layout::{CLine, CMetrics, c_line};
use alloc::alloc::{Layout, alloc, dealloc};
use alloc::vec::Vec;
use bembeyaz_chapter::anchors::Anchors;
use bembeyaz_chapter::chapter::{Chapter, Settings, Step};
use bembeyaz_chapter::flow::Tag;
use bembeyaz_chapter::host::{Host, Image, LineHandle, LineInfo, Links};
use bembeyaz_css::Rules;
use bembeyaz_css::block::Block;
use bembeyaz_layout::boxes::Segment;
use bembeyaz_layout::pager::Pager;
use bembeyaz_layout::{Line, LinkSpan, Metrics};
use core::ffi::{c_char, c_int, c_void};
use core::ptr;

/// `bb_chapter_settings`.
#[repr(C)]
pub struct CSettings {
    font: i32,
    line_compression: f32,
    extra_paragraph_spacing: bool,
    paragraph_alignment: u8,
    viewport_width: u16,
    viewport_height: u16,
    hyphenation: bool,
    focus_reading: bool,
    embedded_style: bool,
    image_rendering: u8,
    paragraph_indent_spaces: u8,
    character_spacing: i8,
    word_spacing_percent: u8,
    language: c_int,
    reclaim: Option<extern "C" fn(usize)>,
}

// As chapter_parser.h's, which ChapterHtmlSlimParser.cpp asserts too.
const _: () = assert!(core::mem::offset_of!(CSettings, language) == 24);
const _: () = assert!(size_of::<CSettings>() == 24 + 2 * size_of::<usize>());

/// Link targets a line passes on the stack.
const FEW_LINKS: usize = 8;

impl CSettings {
    fn settings(&self) -> Settings {
        Settings {
            font: self.font,
            line_compression: self.line_compression,
            extra_paragraph_spacing: self.extra_paragraph_spacing,
            paragraph_alignment: self.paragraph_alignment,
            viewport_width: self.viewport_width,
            viewport_height: self.viewport_height,
            hyphenation: self.hyphenation,
            focus_reading: self.focus_reading,
            embedded_style: self.embedded_style,
            image_rendering: self.image_rendering,
            paragraph_indent_spaces: self.paragraph_indent_spaces,
            character_spacing: self.character_spacing,
            word_spacing_percent: self.word_spacing_percent,
            language: crate::hyphenation::language(self.language),
            reclaim: self.reclaim,
        }
    }
}

// chapter_parser.h's bb_chapter_line; the parser checks the same layout.
const _: () =
    assert!(size_of::<CLineInfo>() == 16 && core::mem::offset_of!(CLineInfo, base_height) == 8);

/// `bb_chapter_line`.
#[repr(C)]
#[derive(Default)]
pub struct CLineInfo {
    handle: u32,
    words: u16,
    base_height: i32,
    ruby_shift: i32,
}

/// `bb_chapter_host`.
#[repr(C)]
pub struct CHost {
    context: *mut c_void,
    metrics: CMetrics,
    rules: *const Rules,
    new_page: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    split: Option<unsafe extern "C" fn(*mut c_void, u8, i16) -> bool>,
    emit: Option<unsafe extern "C" fn(*mut c_void, u32, u16, u16)>,
    add_box: Option<unsafe extern "C" fn(*mut c_void, *const Segment) -> bool>,
    make_line: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const CLine,
            *const Block,
            *const *const c_char,
            *mut CLineInfo,
        ) -> bool,
    >,
    set_line_left: Option<unsafe extern "C" fn(*mut c_void, u32, i16)>,
    push_line: Option<unsafe extern "C" fn(*mut c_void, u32, i16) -> bool>,
    drop_line: Option<unsafe extern "C" fn(*mut c_void, u32)>,
    push_rule: Option<unsafe extern "C" fn(*mut c_void, i16, u8, i16, i16) -> bool>,
    push_drop_cap:
        Option<unsafe extern "C" fn(*mut c_void, i32, u16, bool, *const c_char, i16, i16) -> bool>,
    push_image: Option<unsafe extern "C" fn(*mut c_void, i32, i32, i32, i16) -> bool>,
    add_footnote: Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char)>,
    line_height: Option<unsafe extern "C" fn(*mut c_void, i32, f32) -> c_int>,
    font_variant: Option<unsafe extern "C" fn(*mut c_void, i32, f32) -> i32>,
    glyph: Option<unsafe extern "C" fn(*mut c_void, i32, u32, bool, *mut i32, *mut c_int) -> bool>,
    read: Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize, *mut bool) -> usize>,
    table_open: Option<unsafe extern "C" fn(*mut c_void, u64) -> bool>,
    table_read: Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize) -> usize>,
    table_close: Option<unsafe extern "C" fn(*mut c_void)>,
    table_word: Option<unsafe extern "C" fn(*mut c_void, *const c_char, bool) -> c_int>,
    image: Option<
        unsafe extern "C" fn(*mut c_void, *const c_char, usize, *mut i16, *mut i16) -> c_int,
    >,
}

/// The C host, borrowed for a call.
struct Callbacks<'a>(&'a CHost);

impl Metrics for Callbacks<'_> {
    fn text_advance(&mut self, font: i32, text: &[u8], style: u8, spacing: i8) -> i32 {
        Metrics::text_advance(&mut &self.0.metrics, font, text, style, spacing)
    }
    fn kerning(&mut self, font: i32, left: u32, right: u32, style: u8, spacing: i8) -> i32 {
        Metrics::kerning(&mut &self.0.metrics, font, left, right, style, spacing)
    }
    fn space_advance(&mut self, font: i32, left: u32, right: u32, style: u8) -> i32 {
        Metrics::space_advance(&mut &self.0.metrics, font, left, right, style)
    }
    fn space_width(&mut self, font: i32, style: u8) -> i32 {
        Metrics::space_width(&mut &self.0.metrics, font, style)
    }
    fn ascender(&mut self, font: i32) -> i32 {
        Metrics::ascender(&mut &self.0.metrics, font)
    }
    fn is_sd_font(&mut self, font: i32) -> bool {
        Metrics::is_sd_font(&mut &self.0.metrics, font)
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
        Metrics::prewarm(
            &mut &self.0.metrics,
            font,
            segments,
            space,
            hyphen,
            mask,
            small_caps,
        );
    }
}

// SAFETY (for every call below): the callbacks are the caller's and take the arguments
// chapter_parser.h documents; the pointers passed to them stay valid for the call. A missing
// callback does nothing and reports failure.
impl Host for Callbacks<'_> {
    fn new_page(&mut self) -> bool {
        self.0
            .new_page
            .is_some_and(|f| unsafe { f(self.0.context) })
    }
    fn split(&mut self, lines: u8, top: i16) -> bool {
        self.0
            .split
            .is_some_and(|f| unsafe { f(self.0.context, lines, top) })
    }
    fn emit(&mut self, visible_offset: u32, paragraph: u16, list_item: u16) {
        if let Some(f) = self.0.emit {
            unsafe { f(self.0.context, visible_offset, paragraph, list_item) }
        }
    }
    fn add_box(&mut self, segment: &Segment) -> bool {
        self.0
            .add_box
            .is_some_and(|f| unsafe { f(self.0.context, segment) })
    }
    fn make_line(&mut self, line: &Line<'_>, block: &Block, links: &dyn Links) -> Option<LineInfo> {
        let f = self.0.make_line?;
        // Each link span's target, in span order; on the stack unless the line has many.
        let target = |span: &LinkSpan| links.target(span.link).as_ptr().cast::<c_char>();
        let mut few = [core::ptr::null::<c_char>(); FEW_LINKS];
        let mut many = Vec::new();
        let targets: &[*const c_char] = if line.links.len() <= FEW_LINKS {
            for (slot, span) in few.iter_mut().zip(line.links) {
                *slot = target(span);
            }
            &few
        } else {
            many.try_reserve(line.links.len()).ok()?;
            many.extend(line.links.iter().map(target));
            &many
        };
        let c = c_line(line);
        let mut out = CLineInfo::default();
        let made = unsafe {
            f(
                self.0.context,
                &raw const c,
                block,
                targets.as_ptr(),
                &raw mut out,
            )
        };
        made.then_some(LineInfo {
            handle: out.handle,
            words: out.words,
            base_height: out.base_height,
            ruby_shift: out.ruby_shift,
        })
    }
    fn set_line_left(&mut self, line: LineHandle, x: i16) {
        if let Some(f) = self.0.set_line_left {
            unsafe { f(self.0.context, line, x) }
        }
    }
    fn push_line(&mut self, line: LineHandle, y: i16) -> bool {
        self.0
            .push_line
            .is_some_and(|f| unsafe { f(self.0.context, line, y) })
    }
    fn drop_line(&mut self, line: LineHandle) {
        if let Some(f) = self.0.drop_line {
            unsafe { f(self.0.context, line) }
        }
    }
    fn push_rule(&mut self, width: i16, thickness: u8, x: i16, y: i16) -> bool {
        self.0
            .push_rule
            .is_some_and(|f| unsafe { f(self.0.context, width, thickness, x, y) })
    }
    fn push_drop_cap(
        &mut self,
        font: i32,
        scale: u16,
        bold: bool,
        text: &[u8],
        x: i16,
        y: i16,
    ) -> bool {
        // `text` ends with its NUL.
        self.0.push_drop_cap.is_some_and(|f| unsafe {
            f(
                self.0.context,
                font,
                scale,
                bold,
                text.as_ptr().cast(),
                x,
                y,
            )
        })
    }
    fn push_image(&mut self, width: i32, height: i32, x: i32, y: i16) -> bool {
        self.0
            .push_image
            .is_some_and(|f| unsafe { f(self.0.context, width, height, x, y) })
    }
    fn add_footnote(&mut self, number: &[u8], href: &[u8]) {
        // Both end with their NUL.
        if let Some(f) = self.0.add_footnote {
            unsafe { f(self.0.context, number.as_ptr().cast(), href.as_ptr().cast()) }
        }
    }
    fn line_height(&mut self, font: i32, compression: f32) -> i32 {
        self.0
            .line_height
            .map_or(0, |f| unsafe { f(self.0.context, font, compression) })
    }
    fn font_variant(&mut self, font: i32, scale: f32) -> i32 {
        self.0
            .font_variant
            .map_or(0, |f| unsafe { f(self.0.context, font, scale) })
    }
    fn glyph(&mut self, font: i32, cp: u32, bold: bool) -> Option<(i32, i32)> {
        let f = self.0.glyph?;
        let (mut advance, mut top) = (0, 0);
        unsafe {
            f(
                self.0.context,
                font,
                cp,
                bold,
                &raw mut advance,
                &raw mut top,
            )
        }
        .then_some((advance, top))
    }
    fn read(&mut self, buf: &mut [u8]) -> (usize, bool) {
        let Some(f) = self.0.read else {
            return (0, false);
        };
        let mut more = false;
        let n = unsafe { f(self.0.context, buf.as_mut_ptr(), buf.len(), &raw mut more) };
        (n.min(buf.len()), more)
    }
    fn table_open(&mut self, offset: u64) -> bool {
        self.0
            .table_open
            .is_some_and(|f| unsafe { f(self.0.context, offset) })
    }
    fn table_read(&mut self, buf: &mut [u8]) -> usize {
        self.0.table_read.map_or(0, |f| {
            unsafe { f(self.0.context, buf.as_mut_ptr(), buf.len()) }.min(buf.len())
        })
    }
    fn table_close(&mut self) {
        if let Some(f) = self.0.table_close {
            unsafe { f(self.0.context) }
        }
    }
    fn table_word(&mut self, word: &[u8], bold: bool) -> i32 {
        // `word` ends with its NUL.
        self.0.table_word.map_or(0, |f| unsafe {
            f(self.0.context, word.as_ptr().cast(), bold)
        })
    }
    fn image(&mut self, path: &[u8]) -> Image {
        let Some(f) = self.0.image else {
            return Image::Unsupported;
        };
        let (mut width, mut height) = (0, 0);
        match unsafe {
            f(
                self.0.context,
                path.as_ptr().cast(),
                path.len(),
                &raw mut width,
                &raw mut height,
            )
        } {
            2 => Image::Sized { width, height },
            1 => Image::Unsized,
            _ => Image::Unsupported,
        }
    }
    fn rules(&self) -> Option<&Rules> {
        // SAFETY: NULL or a bb_css_rules that bb_css_rules_init set up, alive for the call.
        unsafe { self.0.rules.as_ref() }
    }
}

/// A start tag from C: a name and `atts` (name, value, ..., NULL).
struct CTag {
    name: *const c_char,
    atts: *const *const c_char,
}

/// The C string at `s`, without its NUL.
///
/// # Safety
/// `s` is NULL or NUL-terminated and outlives `'a`.
unsafe fn c_str<'a>(s: *const c_char) -> &'a [u8] {
    if s.is_null() {
        &[]
    } else {
        // SAFETY: as documented.
        unsafe { core::ffi::CStr::from_ptr(s) }.to_bytes()
    }
}

impl Tag for CTag {
    fn name(&self) -> &[u8] {
        // SAFETY: bb_chapter_start_element's contract.
        unsafe { c_str(self.name) }
    }
    fn attributes(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        let atts = self.atts;
        (0..).map_while(move |i: usize| {
            if atts.is_null() {
                return None;
            }
            // SAFETY: bb_chapter_start_element's contract: pairs, then NULL.
            let name = unsafe { *atts.add(2 * i) };
            if name.is_null() {
                return None;
            }
            let value = unsafe { *atts.add(2 * i + 1) };
            Some((unsafe { c_str(name) }, unsafe { c_str(value) }))
        })
    }
    fn offset(&self) -> u64 {
        0
    }
}

/// # Safety
/// `settings` is NULL or a `bb_chapter_settings`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_new(settings: *const CSettings) -> *mut Chapter {
    // SAFETY: as documented.
    let Some(settings) = (unsafe { settings.as_ref() }) else {
        return ptr::null_mut();
    };
    let layout = Layout::new::<Chapter>();
    // SAFETY: Chapter is not zero-sized.
    let block = unsafe { alloc(layout) }.cast::<Chapter>();
    if !block.is_null() {
        // SAFETY: a fresh block of the right layout.
        unsafe { block.write(Chapter::new(settings.settings())) };
    }
    block
}

/// # Safety
/// `chapter` is NULL or from `bb_chapter_new`, not freed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_free(chapter: *mut Chapter) {
    if !chapter.is_null() {
        // SAFETY: as documented.
        unsafe {
            ptr::drop_in_place(chapter);
            dealloc(chapter.cast(), Layout::new::<Chapter>());
        }
    }
}

/// Runs `f` on the chapter and the host, unless either is NULL.
///
/// # Safety
/// `chapter` is NULL or from `bb_chapter_new`, used by nothing else during the call; `host` is
/// NULL or a `bb_chapter_host` whose callbacks do not use the chapter.
unsafe fn with<R>(
    chapter: *mut Chapter,
    host: *const CHost,
    otherwise: R,
    f: impl FnOnce(&mut Chapter, &mut Callbacks<'_>) -> R,
) -> R {
    // SAFETY: as documented.
    match unsafe { (chapter.as_mut(), host.as_ref()) } {
        (Some(chapter), Some(host)) => f(chapter, &mut Callbacks(host)),
        _ => otherwise,
    }
}

/// # Safety
/// `chapter` as in `bb_chapter_free`; `settings` is NULL or a `bb_chapter_settings`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_set_settings(
    chapter: *mut Chapter,
    settings: *const CSettings,
) {
    // SAFETY: as documented.
    if let (Some(chapter), Some(settings)) = unsafe { (chapter.as_mut(), settings.as_ref()) } {
        chapter.set_settings(settings.settings());
    }
}

/// # Safety
/// `chapter` as in `bb_chapter_free`; `base` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_set_content_base(
    chapter: *mut Chapter,
    base: *const c_char,
    len: usize,
) -> bool {
    // SAFETY: as documented.
    let (Some(chapter), base) = (unsafe { chapter.as_mut() }, unsafe {
        crate::view(base.cast::<u8>(), len)
    }) else {
        return false;
    };
    chapter.set_content_base(base).is_ok()
}

/// # Safety
/// `chapter` as in `bb_chapter_free`; `name` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_add_toc_anchor(
    chapter: *mut Chapter,
    name: *const c_char,
    len: usize,
) {
    // SAFETY: as documented.
    if let Some(chapter) = unsafe { chapter.as_mut() } {
        chapter.add_toc_anchor(unsafe { crate::view(name.cast::<u8>(), len) });
    }
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_begin(chapter: *mut Chapter, host: *const CHost) -> bool {
    // SAFETY: as documented.
    unsafe { with(chapter, host, false, |c, h| c.begin(h)) }
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_step(chapter: *mut Chapter, host: *const CHost) -> c_int {
    // SAFETY: as documented.
    unsafe {
        with(chapter, host, 2, |c, h| match c.step(h) {
            Step::More => 0,
            Step::Done => 1,
            Step::Error => 2,
        })
    }
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_finish(chapter: *mut Chapter, host: *const CHost) -> bool {
    // SAFETY: as documented.
    unsafe { with(chapter, host, false, |c, h| c.finish(h)) }
}

/// # Safety
/// `chapter` as in `bb_chapter_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_abort(chapter: *mut Chapter) {
    // SAFETY: as documented.
    if let Some(chapter) = unsafe { chapter.as_mut() } {
        chapter.abort();
    }
}

/// # Safety
/// `chapter` is NULL or from `bb_chapter_new`; the anchors stay valid until it changes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_anchors(chapter: *const Chapter) -> *const Anchors {
    // SAFETY: as documented.
    unsafe { chapter.as_ref() }.map_or(ptr::null(), |c| ptr::from_ref(c.anchors()))
}

/// # Safety
/// `chapter` is NULL or from `bb_chapter_new`; the pager stays valid until it changes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_pager(chapter: *const Chapter) -> *const Pager {
    // SAFETY: as documented.
    unsafe { chapter.as_ref() }.map_or(ptr::null(), |c| ptr::from_ref(c.pager()))
}

/// # Safety
/// As `with`'s; `name` is NULL or NUL-terminated; `atts` is NULL or name, value, ..., NULL, all
/// NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_start_element(
    chapter: *mut Chapter,
    host: *const CHost,
    name: *const c_char,
    atts: *const *const c_char,
) {
    // SAFETY: as documented.
    unsafe {
        with(chapter, host, (), |c, h| {
            c.start_element(h, &CTag { name, atts })
        })
    }
}

/// # Safety
/// As `with`'s; `text` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_text(
    chapter: *mut Chapter,
    host: *const CHost,
    text: *const c_char,
    len: usize,
) {
    // SAFETY: as documented.
    let text = unsafe { crate::view(text.cast::<u8>(), len) };
    // SAFETY: as documented.
    unsafe { with(chapter, host, (), |c, h| c.character_data(h, text)) }
}

/// # Safety
/// `chapter` as in `bb_chapter_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_restart_pages(chapter: *mut Chapter) {
    // SAFETY: as documented.
    if let Some(chapter) = unsafe { chapter.as_mut() } {
        chapter.restart_pages();
    }
}

/// # Safety
/// `chapter` as in `bb_chapter_free`; `widths` is NULL or holds `count` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_plan_table_columns(
    chapter: *mut Chapter,
    widths: *const u16,
    count: usize,
) {
    // SAFETY: as documented.
    if let Some(chapter) = unsafe { chapter.as_mut() } {
        chapter.plan_table_columns(unsafe { crate::view(widths, count) });
    }
}

/// # Safety
/// As `with`'s; `name` is NULL or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_chapter_end_element(
    chapter: *mut Chapter,
    host: *const CHost,
    name: *const c_char,
) {
    // SAFETY: as documented.
    let name = unsafe { c_str(name) };
    // SAFETY: as documented.
    unsafe { with(chapter, host, (), |c, h| c.end_element(h, name)) }
}
