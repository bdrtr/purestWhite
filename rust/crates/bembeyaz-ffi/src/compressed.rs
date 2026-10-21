//! Exports for lib/EpdFont's `FontDecompressor` (see `rust/include/bembeyaz/compressed_font.h`).

use crate::{view, view_mut};
use bembeyaz_font::compressed::{self, Entry, Font, Group, MAX_PAGE_GROUPS};
use bembeyaz_font::glyph::Glyph;
use bembeyaz_font::page::Interval;
use core::ffi::{CStr, c_char};

// compressed_font.h's bb_compressed_group (EpdFontGroup) and bb_compressed_entry, and the font view.
const _: () = {
    use core::mem::offset_of;
    let pointer = size_of::<usize>();
    assert!(
        size_of::<Group>() == 20
            && offset_of!(Group, glyph_count) == 12
            && offset_of!(Group, first_glyph) == 16
            && size_of::<Entry>() == 12
            && offset_of!(Entry, aligned_offset) == 8
            && size_of::<FontView>() == pointer * 9
            && MAX_PAGE_GROUPS == 128
            && compressed::MAX_PAGE_GLYPHS == 512
    );
};

/// compressed_font.h's bb_compressed_font.
#[repr(C)]
pub struct FontView {
    glyphs: *const Glyph,
    glyph_count: usize,
    intervals: *const Interval,
    interval_count: usize,
    groups: *const Group,
    group_count: usize,
    glyph_to_group: *const u16,
    ligatures: *const u8,
    ligature_count: usize,
}

// SAFETY: `font` is null or a view whose arrays hold their counts.
unsafe fn font<'a>(font: *const FontView) -> Option<Font<'a>> {
    // SAFETY: as above.
    let f = unsafe { font.as_ref() }?;
    // SAFETY: as above; the group table, when given, has one entry per glyph.
    unsafe {
        Some(Font {
            glyphs: view(f.glyphs, f.glyph_count),
            intervals: view(f.intervals, f.interval_count),
            groups: view(f.groups, f.group_count),
            glyph_to_group: (!f.glyph_to_group.is_null())
                .then(|| view(f.glyph_to_group, f.glyph_count)),
            ligatures: view(f.ligatures, f.ligature_count * 8),
        })
    }
}

/// # Safety
///
/// `font` is null or a `bb_compressed_font` as compressed_font.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_compressed_group_of(font_view: *const FontView, glyph: u32) -> i32 {
    // SAFETY: the caller's view.
    unsafe { font(font_view) }
        .and_then(|f| f.group_of(glyph))
        .map_or(-1, i32::from)
}

/// # Safety
///
/// As `bb_compressed_group_of`; `decompressed` holds `decompressed_len` bytes, `packed`
/// `packed_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_compressed_unpack(
    font_view: *const FontView,
    group: u16,
    glyph: u32,
    decompressed: *const u8,
    decompressed_len: usize,
    packed: *mut u8,
    packed_len: usize,
) {
    // SAFETY: the caller's view and buffers.
    unsafe {
        if let Some(f) = font(font_view) {
            f.unpack(
                group,
                glyph,
                view(decompressed, decompressed_len),
                view_mut(packed, packed_len),
            );
        }
    }
}

/// # Safety
///
/// As `bb_compressed_group_of`; `text` is a C string, `out` holds `out_len` values, `capped` is
/// null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_compressed_gather(
    font_view: *const FontView,
    text: *const c_char,
    out: *mut u32,
    out_len: usize,
    capped: *mut bool,
) -> usize {
    // SAFETY: the caller's view and C string.
    let (Some(f), false) = (unsafe { font(font_view) }, text.is_null()) else {
        return 0;
    };
    // SAFETY: as above.
    let text = unsafe { CStr::from_ptr(text) }.to_bytes();
    // SAFETY: the caller's output.
    let (count, was_capped) =
        compressed::gather_glyphs(&f, text, unsafe { view_mut(out, out_len) });
    if !capped.is_null() {
        // SAFETY: the caller's writable flag.
        unsafe { capped.write(was_capped) };
    }
    count
}

/// # Safety
///
/// As `bb_compressed_group_of`; `glyphs` holds `count` values, `groups` 128 writable values,
/// `total_bytes` and `capped` are null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_compressed_plan(
    font_view: *const FontView,
    glyphs: *const u32,
    count: usize,
    groups: *mut u16,
    total_bytes: *mut u32,
    capped: *mut bool,
) -> usize {
    // SAFETY: the caller's view.
    let Some(f) = (unsafe { font(font_view) }) else {
        return 0;
    };
    // SAFETY: the caller's arrays.
    let plan =
        unsafe { compressed::plan(&f, view(glyphs, count), view_mut(groups, MAX_PAGE_GROUPS)) };
    // SAFETY: the caller's writable outputs, if given.
    unsafe {
        if !total_bytes.is_null() {
            total_bytes.write(plan.total_bytes);
        }
        if !capped.is_null() {
            capped.write(plan.capped);
        }
    }
    plan.group_count
}

/// # Safety
///
/// As `bb_compressed_group_of`; `glyphs` and `entries` hold `count` values, `groups`
/// `group_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_compressed_index(
    font_view: *const FontView,
    glyphs: *const u32,
    count: usize,
    groups: *const u16,
    group_count: usize,
    entries: *mut Entry,
) {
    // SAFETY: the caller's view.
    let Some(f) = (unsafe { font(font_view) }) else {
        return;
    };
    // SAFETY: the caller's arrays.
    unsafe {
        compressed::index(
            &f,
            view(glyphs, count),
            view(groups, group_count),
            view_mut(entries, count),
        );
    }
}

/// # Safety
///
/// As `bb_compressed_group_of`; the arrays hold their lengths.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_compressed_extract(
    font_view: *const FontView,
    group: u16,
    decompressed: *const u8,
    decompressed_len: usize,
    entries: *mut Entry,
    count: usize,
    buffer: *mut u8,
    buffer_len: usize,
    write: u32,
) -> u32 {
    // SAFETY: the caller's view.
    let Some(f) = (unsafe { font(font_view) }) else {
        return write;
    };
    // SAFETY: the caller's arrays.
    unsafe {
        compressed::extract(
            &f,
            group,
            view(decompressed, decompressed_len),
            view_mut(entries, count),
            view_mut(buffer, buffer_len),
            write,
        )
    }
}

/// # Safety
///
/// `entries` holds `count` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_compressed_find_entry(
    entries: *const Entry,
    count: usize,
    glyph: u32,
) -> u32 {
    // SAFETY: the caller's entries.
    compressed::find_entry(unsafe { view(entries, count) }, glyph).unwrap_or(u32::MAX)
}
