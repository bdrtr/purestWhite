//! Exports for lib/Epub's `BookMetadataCache` (see `rust/include/bembeyaz/book.h`).

use crate::io::{FileView, SinkView, write_with};
use crate::{room, set, view, view_mut};
use bembeyaz_book::cache::{
    self, Build, BuildError, Field, Header, IndexEntry, LoadError, Parts, Sizes, Target, WINDOW,
};
use core::ffi::{c_char, c_int, c_void};

// book.h's structs.
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<IndexEntry>() == 16
            && offset_of!(IndexEntry, len) == 8
            && offset_of!(IndexEntry, spine) == 10
            && size_of::<Header>() == 8
            && offset_of!(Header, spine_count) == 4
            && WINDOW == 32
            && Field::Anchor as u8 == 7
    );
};

/// book.h's `bb_book_target`.
#[repr(C)]
pub struct TargetView {
    context: *mut c_void,
    text: Option<unsafe extern "C" fn(*mut c_void, u8, usize) -> *mut c_char>,
}

impl Target for &TargetView {
    fn text(&mut self, field: Field, len: usize) -> Option<&mut [u8]> {
        let f = self.text?;
        // SAFETY: the caller's callback gives `len` bytes of room.
        unsafe { room(f(self.context, field as u8, len).cast(), len) }
    }
}

/// book.h's `bb_book_records`.
#[repr(C)]
pub struct Records {
    file: *const FileView,
    buf: *mut u8,
    buf_len: usize,
    count: u16,
}

// The caller's file, its buffer and its record count.
//
// SAFETY: `records` is null or a `bb_book_records` as book.h describes.
unsafe fn open<'a>(records: *const Records) -> Option<(&'a FileView, &'a mut [u8], u16)> {
    // SAFETY: as above.
    unsafe {
        let r = records.as_ref()?;
        Some((r.file.as_ref()?, view_mut(r.buf, r.buf_len), r.count))
    }
}

// The caller's file, buffer and target.
//
// SAFETY: as book.h describes `bb_book_records` and `bb_book_target`.
unsafe fn with_target<'a>(
    records: *const Records,
    target: *const TargetView,
) -> Option<(&'a FileView, &'a mut [u8], &'a TargetView)> {
    // SAFETY: as above.
    unsafe {
        let (file, buf, _) = open(records)?;
        Some((file, buf, target.as_ref()?))
    }
}

/// # Safety
///
/// `book` and `target` are null or as book.h describes; `out` and `version` are null or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_load(
    book: *const Records,
    target: *const TargetView,
    out: *mut Header,
    version: *mut u8,
) -> c_int {
    // SAFETY: as documented.
    let Some((mut f, buf, mut t)) = (unsafe { with_target(book, target) }) else {
        return 2;
    };
    match cache::load(&mut f, buf, &mut t) {
        Ok(header) => {
            // SAFETY: the caller's output.
            unsafe { set(out, header) };
            0
        }
        Err(LoadError::Version(v)) => {
            // SAFETY: the caller's output.
            unsafe { set(version, v) };
            1
        }
        Err(LoadError::Damaged) => 2,
    }
}

/// # Safety
///
/// `book` is null or as book.h describes; `header` null or readable; `starts` holds `count`
/// writable values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_window_starts(
    book: *const Records,
    header: *const Header,
    starts: *mut u32,
    count: usize,
) -> u32 {
    // SAFETY: as documented.
    let (Some((mut f, buf, _)), Some(header)) = (unsafe { open(book) }, unsafe { header.as_ref() })
    else {
        return 0;
    };
    // SAFETY: the caller's starts.
    let starts = unsafe { view_mut(starts, count) };
    cache::window_starts(&mut f, buf, header, starts)
}

/// # Safety
///
/// As `bb_book_load`; `cumulative` and `toc` are null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_spine_entry(
    book: *const Records,
    lut: u32,
    index: u16,
    target: *const TargetView,
    cumulative: *mut u32,
    toc: *mut i16,
) {
    // SAFETY: as documented.
    let Some((mut f, buf, mut t)) = (unsafe { with_target(book, target) }) else {
        return;
    };
    let (size, toc_index) = cache::spine_entry(&mut f, buf, lut, index, &mut t);
    // SAFETY: the caller's outputs.
    unsafe {
        set(cumulative, size);
        set(toc, toc_index);
    }
}

/// # Safety
///
/// As `bb_book_load`; `level` and `spine` are null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_toc_entry(
    book: *const Records,
    lut: u32,
    spine_count: u16,
    index: u16,
    target: *const TargetView,
    level: *mut u8,
    spine: *mut i16,
) {
    // SAFETY: as documented.
    let Some((mut f, buf, mut t)) = (unsafe { with_target(book, target) }) else {
        return;
    };
    let (l, s) = cache::toc_entry(&mut f, buf, lut, spine_count, index, &mut t);
    // SAFETY: the caller's outputs.
    unsafe {
        set(level, l);
        set(spine, s);
    }
}

/// # Safety
///
/// `book` is null or as book.h describes (its count is the spine count); `window` is null or
/// holds 32 writable sizes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_fill_window(
    book: *const Records,
    lut: u32,
    first: u16,
    window: *mut [u32; WINDOW],
) -> bool {
    // SAFETY: as documented.
    let (Some((mut f, buf, count)), Some(window)) =
        (unsafe { open(book) }, unsafe { window.as_mut() })
    else {
        return false;
    };
    cache::fill_window(&mut f, buf, lut, count, first, window)
}

/// book.h's `bb_book_sizes`.
#[repr(C)]
pub struct SizesView {
    spine_count: u16,
    book_size: u32,
    starts: *const u32,
    start_count: usize,
    window_first: *mut i32,
    window: *mut [u32; WINDOW],
}

type Fill = unsafe extern "C" fn(*mut c_void, i32, *mut [u32; WINDOW]) -> bool;

// Runs `ask` on the caller's sizes, filling the window through `fill`.
//
// SAFETY: `sizes` is null or a `bb_book_sizes` as book.h describes; `fill` takes `context`.
unsafe fn with_sizes<R>(
    sizes: *const SizesView,
    context: *mut c_void,
    fill: Option<Fill>,
    ask: impl FnOnce(&mut Sizes, &mut dyn FnMut(i32, &mut [u32; WINDOW]) -> bool) -> R,
) -> Option<R> {
    // SAFETY: as above.
    let s = unsafe { sizes.as_ref() }?;
    // SAFETY: as above.
    let (window_first, window) = unsafe { (s.window_first.as_mut()?, s.window.as_mut()?) };
    let mut sizes = Sizes {
        spine_count: s.spine_count,
        book_size: s.book_size,
        // SAFETY: the caller's starts.
        starts: unsafe { view(s.starts, s.start_count) },
        window_first,
        window,
    };
    // SAFETY: the caller's callback fills the window it is given.
    let mut refill =
        |first: i32, w: &mut [u32; WINDOW]| fill.is_some_and(|f| unsafe { f(context, first, w) });
    Some(ask(&mut sizes, &mut refill))
}

/// # Safety
///
/// `sizes` is null or a `bb_book_sizes` as book.h describes; `fill` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_cumulative_size(
    sizes: *const SizesView,
    index: i32,
    context: *mut c_void,
    fill: Option<Fill>,
) -> u32 {
    // SAFETY: as documented.
    unsafe { with_sizes(sizes, context, fill, |s, f| s.cumulative(index, f)) }.unwrap_or(0)
}

/// # Safety
///
/// As `bb_book_cumulative_size`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_index_for_size(
    sizes: *const SizesView,
    size: u32,
    context: *mut c_void,
    fill: Option<Fill>,
) -> i32 {
    // SAFETY: as documented.
    unsafe { with_sizes(sizes, context, fill, |s, f| s.index_for(size, f)) }.unwrap_or(-1)
}

/// # Safety
///
/// `sink` is null or a `bb_io_sink` as io.h describes; `at` is null or readable and writable;
/// `href` holds `href_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_write_spine_record(
    sink: *const SinkView,
    at: *mut u32,
    href: *const c_char,
    href_len: usize,
    cumulative: u32,
    toc: i16,
) -> bool {
    // SAFETY: the caller's href.
    let href = unsafe { view(href.cast::<u8>(), href_len) };
    // SAFETY: as documented.
    unsafe {
        write_with(sink, at, |s, a| {
            cache::write_spine_record(s, a, href, cumulative, toc)
        })
    }
}

/// # Safety
///
/// As `bb_book_write_spine_record`; each string holds its length.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_book_write_toc_record(
    sink: *const SinkView,
    at: *mut u32,
    title: *const c_char,
    title_len: usize,
    href: *const c_char,
    href_len: usize,
    anchor: *const c_char,
    anchor_len: usize,
    level: u8,
    spine: i16,
) -> bool {
    // SAFETY: the caller's strings.
    let texts = unsafe {
        [
            view(title.cast::<u8>(), title_len),
            view(href.cast::<u8>(), href_len),
            view(anchor.cast::<u8>(), anchor_len),
        ]
    };
    // SAFETY: as documented.
    unsafe {
        write_with(sink, at, |s, a| {
            cache::write_toc_record(s, a, texts, level, spine)
        })
    }
}

/// # Safety
///
/// `spine` is null or as book.h describes; `entries` holds its count of writable entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_index_spine(
    spine: *const Records,
    entries: *mut IndexEntry,
) -> bool {
    // SAFETY: as documented.
    let Some((mut f, buf, count)) = (unsafe { open(spine) }) else {
        return false;
    };
    // SAFETY: as documented.
    let entries = unsafe { view_mut(entries, usize::from(count)) };
    cache::index_spine(&mut f, buf, count, entries)
}

/// # Safety
///
/// `entries` holds `count` entries, `href` `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_find_in_index(
    entries: *const IndexEntry,
    count: usize,
    href: *const c_char,
    len: usize,
) -> i16 {
    // SAFETY: as documented.
    let (entries, href) = unsafe { (view(entries, count), view(href.cast::<u8>(), len)) };
    cache::find_in_index(entries, href)
}

/// # Safety
///
/// `spine` is null or as book.h describes; `href` holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_find_spine(
    spine: *const Records,
    href: *const c_char,
    len: usize,
) -> i16 {
    // SAFETY: as documented.
    let Some((mut f, buf, count)) = (unsafe { open(spine) }) else {
        return -1;
    };
    // SAFETY: the caller's href.
    let href = unsafe { view(href.cast::<u8>(), len) };
    cache::find_spine(&mut f, buf, count, href)
}

/// book.h's `bb_book_items`.
#[repr(C)]
pub struct ItemsView {
    context: *mut c_void,
    href: Option<unsafe extern "C" fn(*mut c_void, usize) -> *mut c_char>,
    size: Option<unsafe extern "C" fn(*mut c_void, u16) -> u32>,
}

impl Build for &ItemsView {
    fn href(&mut self, len: usize) -> Option<&mut [u8]> {
        let f = self.href?;
        // SAFETY: the caller's callback gives `len` bytes of room.
        unsafe { room(f(self.context, len).cast(), len) }
    }
    fn size(&mut self, index: u16) -> u32 {
        // SAFETY: the caller's callback.
        self.size.map_or(0, |f| unsafe { f(self.context, index) })
    }
}

/// # Safety
///
/// `spine` is null or as book.h describes, `items` null or a `bb_book_items`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_spine_hrefs(
    spine: *const Records,
    items: *const ItemsView,
) -> bool {
    // SAFETY: as documented.
    let (Some((mut f, buf, count)), Some(mut items)) =
        (unsafe { open(spine) }, unsafe { items.as_ref() })
    else {
        return false;
    };
    cache::spine_hrefs(&mut f, buf, count, &mut items)
}

/// # Safety
///
/// `spine` and `toc` are null or as book.h describes, `out` null or a `bb_io_sink` as io.h
/// describes; `metadata` and `metadata_len` are null or hold five strings and their lengths;
/// `toc_map` holds the spine count of writable entries; `items` is null or a `bb_book_items`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_book_build(
    spine: *const Records,
    toc: *const Records,
    out: *const SinkView,
    metadata: *const [*const c_char; 5],
    metadata_len: *const [usize; 5],
    toc_map: *mut i16,
    items: *const ItemsView,
) -> c_int {
    // SAFETY: as documented.
    let (Some((mut spine, spine_buf, spine_count)), Some((mut toc, toc_buf, toc_count))) =
        (unsafe { open(spine) }, unsafe { open(toc) })
    else {
        return 1;
    };
    // SAFETY: as documented.
    let (Some(mut out), Some(names), Some(lens), Some(mut items)) = (unsafe {
        (
            out.as_ref(),
            metadata.as_ref(),
            metadata_len.as_ref(),
            items.as_ref(),
        )
    }) else {
        return 2;
    };
    let mut texts: [&[u8]; 5] = [&[]; 5];
    for (text, (&name, &len)) in texts.iter_mut().zip(names.iter().zip(lens)) {
        // SAFETY: the caller's strings.
        *text = unsafe { view(name.cast::<u8>(), len) };
    }
    // SAFETY: the caller's map.
    let toc_map = unsafe { view_mut(toc_map, usize::from(spine_count)) };
    let parts = Parts {
        spine: &mut spine,
        spine_buf,
        spine_count,
        toc: &mut toc,
        toc_buf,
        toc_count,
    };
    match cache::build(parts, &mut out, texts, toc_map, &mut items) {
        Ok(()) => 0,
        Err(BuildError::Damaged) => 1,
        Err(BuildError::Memory) => 2,
        Err(BuildError::Write) => 3,
    }
}
