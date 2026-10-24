//! Exports for lib/Epub's `Page` and `TextBlock` (see `rust/include/bembeyaz/page.h`).

use crate::io::{FileView, SinkView, write_with};
use crate::{room, set, view};
use bembeyaz_section::page::{
    self, BlockStyle, Builder, Error, Footnote, HREF_LEN, Line, Link, NUMBER_LEN, Side, TextHeader,
};
use core::ffi::{c_char, c_int, c_void};

// page.h's structs.
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<TextHeader>() == 8
            && offset_of!(TextHeader, text_bytes) == 4
            && size_of::<BlockStyle>() == 28
            && offset_of!(BlockStyle, margin_top) == 2
            && offset_of!(BlockStyle, text_indent_defined) == 20
            && offset_of!(BlockStyle, font_id) == 24
            && size_of::<Side>() == 2
            && size_of::<Footnote>() == 288
            && size_of::<Link>() == 264
            && offset_of!(Link, x) == 256
            && NUMBER_LEN == 32
            && HREF_LEN == 256
            && page::MAX_FOOTNOTES == 16
            && page::MAX_LINKS == 32
            && page::MAX_DROP_CAP == 12
            && page::WORD_FONTS == 3
    );
};

type Room = unsafe extern "C" fn(*mut c_void, usize) -> *mut u8;

/// page.h's `bb_page_builder`.
#[repr(C)]
pub struct BuilderView {
    context: *mut c_void,
    begin: Option<unsafe extern "C" fn(*mut c_void, u16)>,
    line: Option<
        unsafe extern "C" fn(*mut c_void, i16, i16, *const TextHeader, usize, *mut *mut u8) -> bool,
    >,
    ruby: Option<unsafe extern "C" fn(*mut c_void, u16, usize) -> *mut c_char>,
    word_fonts: Option<Room>,
    line_end: Option<unsafe extern "C" fn(*mut c_void, *const BlockStyle) -> bool>,
    image_text: Option<unsafe extern "C" fn(*mut c_void, bool, usize) -> *mut c_char>,
    image: Option<unsafe extern "C" fn(*mut c_void, i16, i16, i16, i16) -> bool>,
    rule: Option<unsafe extern "C" fn(*mut c_void, i16, i16, u16, u8) -> bool>,
    drop_cap: Option<
        unsafe extern "C" fn(*mut c_void, i16, i16, i32, u16, u8, *const c_char, usize) -> bool,
    >,
    border_box:
        Option<unsafe extern "C" fn(*mut c_void, i16, i16, u16, u16, *const Side, bool) -> bool>,
    footnotes: Option<unsafe extern "C" fn(*mut c_void, usize) -> *mut Footnote>,
    links: Option<unsafe extern "C" fn(*mut c_void, usize) -> *mut Link>,
}

impl Builder for &BuilderView {
    fn begin(&mut self, count: u16) {
        if let Some(f) = self.begin {
            // SAFETY: the caller's callback.
            unsafe { f(self.context, count) };
        }
    }
    fn line(&mut self, x: i16, y: i16, text: &TextHeader, size: usize) -> Option<&mut [u8]> {
        let f = self.line?;
        let mut arena = core::ptr::null_mut();
        // SAFETY: the caller's callback, which sets `arena` to `size` bytes of room.
        unsafe { f(self.context, x, y, text, size, &raw mut arena).then(|| room(arena, size))? }
    }
    fn ruby(&mut self, word: u16, len: usize) -> Option<&mut [u8]> {
        let f = self.ruby?;
        // SAFETY: the caller's callback gives `len` bytes of room.
        unsafe { room(f(self.context, word, len).cast(), len) }
    }
    fn word_fonts(&mut self, size: usize) -> Option<&mut [u8]> {
        let f = self.word_fonts?;
        // SAFETY: as `ruby`.
        unsafe { room(f(self.context, size), size) }
    }
    fn line_end(&mut self, style: &BlockStyle) -> bool {
        // SAFETY: the caller's callback reads `style` while it runs.
        self.line_end
            .is_some_and(|f| unsafe { f(self.context, style) })
    }
    fn image_text(&mut self, source: bool, len: usize) -> Option<&mut [u8]> {
        let f = self.image_text?;
        // SAFETY: as `ruby`.
        unsafe { room(f(self.context, source, len).cast(), len) }
    }
    fn image(&mut self, x: i16, y: i16, width: i16, height: i16) -> bool {
        // SAFETY: the caller's callback.
        self.image
            .is_some_and(|f| unsafe { f(self.context, x, y, width, height) })
    }
    fn rule(&mut self, x: i16, y: i16, width: u16, thickness: u8) -> bool {
        // SAFETY: the caller's callback.
        self.rule
            .is_some_and(|f| unsafe { f(self.context, x, y, width, thickness) })
    }
    fn drop_cap(&mut self, x: i16, y: i16, font_id: i32, scale: u16, style: u8, t: &[u8]) -> bool {
        self.drop_cap.is_some_and(|f| {
            // SAFETY: the caller's callback reads `t` while it runs.
            unsafe {
                f(
                    self.context,
                    x,
                    y,
                    font_id,
                    scale,
                    style,
                    t.as_ptr().cast(),
                    t.len(),
                )
            }
        })
    }
    fn border_box(
        &mut self,
        x: i16,
        y: i16,
        width: u16,
        height: u16,
        sides: &[Side; 4],
        shaded: bool,
    ) -> bool {
        self.border_box.is_some_and(|f| {
            // SAFETY: the caller's callback reads the four sides while it runs.
            unsafe { f(self.context, x, y, width, height, sides.as_ptr(), shaded) }
        })
    }
    fn footnotes(&mut self, count: usize) -> Option<&mut [Footnote]> {
        let f = self.footnotes?;
        // SAFETY: the caller's callback gives room for `count` footnotes.
        unsafe { room(f(self.context, count), count) }
    }
    fn links(&mut self, count: usize) -> Option<&mut [Link]> {
        let f = self.links?;
        // SAFETY: as `footnotes`.
        unsafe { room(f(self.context, count), count) }
    }
}

fn code(error: Error) -> c_int {
    c_int::from(error.code())
}

/// What a read's code means.
#[unsafe(no_mangle)]
pub extern "C" fn bb_page_error(code: c_int) -> *const c_char {
    Error::message(code).as_ptr()
}

/// # Safety
///
/// `text` is null or a `bb_page_text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_arena_size(text: *const TextHeader) -> usize {
    // SAFETY: the caller's header.
    unsafe { text.as_ref() }.map_or(0, page::arena_size)
}

// SAFETY: `file` is null or a `bb_section_file`, `builder` null or a `bb_page_builder` as page.h
// describes, `end` null or writable.
unsafe fn read_with(
    file: *const FileView,
    builder: *const BuilderView,
    end: *mut u32,
    read: impl FnOnce(&mut &FileView, &mut &BuilderView) -> Result<u32, Error>,
) -> c_int {
    // SAFETY: as above.
    let (Some(mut f), Some(mut b)) = (unsafe { file.as_ref() }, unsafe { builder.as_ref() }) else {
        return code(Error::Memory);
    };
    match read(&mut f, &mut b) {
        Ok(at) => {
            // SAFETY: as above.
            unsafe { set(end, at) };
            0
        }
        Err(e) => code(e),
    }
}

/// # Safety
///
/// `file` is null or a `bb_section_file`, `builder` null or a `bb_page_builder` as page.h
/// describes; `end` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_read(
    file: *const FileView,
    at: u32,
    builder: *const BuilderView,
    end: *mut u32,
) -> c_int {
    // SAFETY: as documented.
    unsafe { read_with(file, builder, end, |f, b| page::read(f, at, b)) }
}

/// # Safety
///
/// As `bb_page_read`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_read_text_block(
    file: *const FileView,
    at: u32,
    builder: *const BuilderView,
    end: *mut u32,
) -> c_int {
    // SAFETY: as documented.
    unsafe { read_with(file, builder, end, |f, b| page::read_text_block(f, at, b)) }
}

/// page.h's `bb_page_line`.
#[repr(C)]
pub struct LineView {
    text: TextHeader,
    arena: *const u8,
    ruby_context: *const c_void,
    ruby: Option<unsafe extern "C" fn(*const c_void, u16, *mut usize) -> *const c_char>,
    style: BlockStyle,
    word_fonts: *const u8,
}

// SAFETY: `line` is null or a `bb_page_line` as page.h describes.
unsafe fn with_line<R>(line: *const LineView, use_line: impl FnOnce(&Line) -> R) -> Option<R> {
    // SAFETY: as above.
    let l = unsafe { line.as_ref() }?;
    let ruby = |word: u16| -> &[u8] {
        let mut len = 0usize;
        // SAFETY: the caller's callback gives the word's ruby, `len` bytes, valid for the call.
        l.ruby.map_or(&[], |f| unsafe {
            view(f(l.ruby_context, word, &raw mut len).cast::<u8>(), len)
        })
    };
    let fonts = 4 * page::WORD_FONTS + usize::from(l.text.word_count);
    // SAFETY: the arena holds `arena_size` bytes and the word fonts, when given, theirs.
    let (arena, word_fonts) = unsafe {
        (
            view(l.arena, page::arena_size(&l.text)),
            (!l.word_fonts.is_null()).then(|| view(l.word_fonts, fonts)),
        )
    };
    Some(use_line(&Line {
        text: l.text,
        arena,
        ruby: &ruby,
        style: l.style,
        word_fonts,
    }))
}

/// # Safety
///
/// `sink` is null or a `bb_section_sink` as section.h describes; `at` is null or readable and
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_write_count(
    sink: *const SinkView,
    at: *mut u32,
    count: u16,
) -> bool {
    // SAFETY: as documented.
    unsafe { write_with(sink, at, |s, a| page::write_count(s, a, count)) }
}

/// # Safety
///
/// As `bb_page_write_count`; `line` is null or a `bb_page_line` as page.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_write_line(
    sink: *const SinkView,
    at: *mut u32,
    x: i16,
    y: i16,
    line: *const LineView,
) -> bool {
    // SAFETY: as documented.
    unsafe {
        write_with(sink, at, |s, a| {
            with_line(line, |l| page::write_line(s, a, x, y, l)).flatten()
        })
    }
}

/// # Safety
///
/// As `bb_page_write_line`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_write_text_block(
    sink: *const SinkView,
    at: *mut u32,
    line: *const LineView,
) -> bool {
    // SAFETY: as documented.
    unsafe {
        write_with(sink, at, |s, a| {
            with_line(line, |l| page::write_text_block(s, a, l)).flatten()
        })
    }
}

/// # Safety
///
/// As `bb_page_write_count`; `path` and `source` hold their lengths.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_page_write_image(
    sink: *const SinkView,
    at: *mut u32,
    x: i16,
    y: i16,
    path: *const c_char,
    path_len: usize,
    source: *const c_char,
    source_len: usize,
    width: i16,
    height: i16,
) -> bool {
    // SAFETY: the caller's strings.
    let (path, source) = unsafe {
        (
            view(path.cast::<u8>(), path_len),
            view(source.cast::<u8>(), source_len),
        )
    };
    // SAFETY: as documented.
    unsafe {
        write_with(sink, at, |s, a| {
            page::write_image(s, a, x, y, path, source, width, height)
        })
    }
}

/// # Safety
///
/// As `bb_page_write_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_write_rule(
    sink: *const SinkView,
    at: *mut u32,
    x: i16,
    y: i16,
    width: u16,
    thickness: u8,
) -> bool {
    // SAFETY: as documented.
    unsafe {
        write_with(sink, at, |s, a| {
            page::write_rule(s, a, x, y, width, thickness)
        })
    }
}

/// # Safety
///
/// As `bb_page_write_count`; `text` holds `len` bytes.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_page_write_drop_cap(
    sink: *const SinkView,
    at: *mut u32,
    x: i16,
    y: i16,
    font_id: i32,
    scale256: u16,
    style: u8,
    text: *const c_char,
    len: usize,
) -> bool {
    // SAFETY: the caller's text.
    let text = unsafe { view(text.cast::<u8>(), len) };
    // SAFETY: as documented.
    unsafe {
        write_with(sink, at, |s, a| {
            page::write_drop_cap(s, a, x, y, font_id, scale256, style, text)
        })
    }
}

/// # Safety
///
/// As `bb_page_write_count`; `sides` is null or holds four sides.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_page_write_border_box(
    sink: *const SinkView,
    at: *mut u32,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    sides: *const [Side; 4],
    shaded: bool,
) -> bool {
    // SAFETY: the caller's sides.
    let Some(sides) = (unsafe { sides.as_ref() }) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe {
        write_with(sink, at, |s, a| {
            page::write_border_box(s, a, x, y, width, height, sides, shaded)
        })
    }
}

/// # Safety
///
/// As `bb_page_write_count`; `footnotes` and `links` hold their counts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_write_notes(
    sink: *const SinkView,
    at: *mut u32,
    footnotes: *const Footnote,
    footnote_count: usize,
    links: *const Link,
    link_count: usize,
) -> bool {
    // SAFETY: the caller's records.
    let (footnotes, links) = unsafe { (view(footnotes, footnote_count), view(links, link_count)) };
    // SAFETY: as documented.
    unsafe { write_with(sink, at, |s, a| page::write_notes(s, a, footnotes, links)) }
}

/// # Safety
/// `links` is NULL or holds `count` links, used by nothing else during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_split_links(links: *mut Link, count: usize, top: i16) -> usize {
    // SAFETY: as documented.
    bembeyaz_section::notes::split_links(unsafe { crate::view_mut(links, count) }, top)
}

type FootnoteSink = unsafe extern "C" fn(*mut c_void, *const Footnote);

/// # Safety
/// `notes` is NULL or holds `count` footnotes, used by nothing else during the call; `staying`
/// and `carried` are NULL or hold their counts of links; `moved` takes the footnote it is handed
/// for the call only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_page_split_footnotes(
    notes: *mut Footnote,
    count: usize,
    staying: *const Link,
    staying_count: usize,
    carried: *const Link,
    carried_count: usize,
    moved: Option<FootnoteSink>,
    context: *mut c_void,
) -> usize {
    // SAFETY: as documented.
    let (notes, staying, carried) = unsafe {
        (
            crate::view_mut(notes, count),
            view(staying, staying_count),
            view(carried, carried_count),
        )
    };
    bembeyaz_section::notes::split_footnotes(notes, staying, carried, |note| {
        if let Some(moved) = moved {
            // SAFETY: as documented.
            unsafe { moved(context, note) };
        }
    })
}
