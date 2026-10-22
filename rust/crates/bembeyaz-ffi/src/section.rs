//! Exports for lib/Epub's `Section` (see `rust/include/bembeyaz/section.h`).

use crate::{set, view, view_mut};
use bembeyaz_section::file::{
    self, CommitError, HEADER_SIZE, LutEntry, OffsetScan, PARTIAL_VERSION, Refusal, Spec, VERSION,
    Watermark,
};
use bembeyaz_section::{Sink, Source};
use core::ffi::{c_char, c_int, c_void};

// section.h's structs.
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Spec>() == 24
            && offset_of!(Spec, viewport_width) == 12
            && offset_of!(Spec, word_spacing_percent) == 21
            && size_of::<LutEntry>() == 12
            && offset_of!(LutEntry, visible) == 8
            && size_of::<Loaded>() == 16
            && offset_of!(Loaded, consumed) == 4
            && offset_of!(Loaded, version) == 12
            && size_of::<Watermark>() == 8
            && HEADER_SIZE == 44
            && VERSION == 60
            && PARTIAL_VERSION == 0xDE
    );
};

/// section.h's `bb_section_file`.
#[repr(C)]
pub struct FileView {
    context: *mut c_void,
    size: u32,
    read_at: Option<unsafe extern "C" fn(*mut c_void, u32, *mut u8, usize) -> bool>,
}

impl Source for &FileView {
    fn size(&self) -> u32 {
        self.size
    }
    fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool {
        // SAFETY: the caller's callback fills `buf`, which holds `buf.len()` bytes.
        self.read_at
            .is_some_and(|read| unsafe { read(self.context, offset, buf.as_mut_ptr(), buf.len()) })
    }
}

/// section.h's `bb_section_sink`.
#[repr(C)]
pub struct SinkView {
    context: *mut c_void,
    write_at: Option<unsafe extern "C" fn(*mut c_void, u32, *const u8, usize) -> bool>,
}

impl Sink for &SinkView {
    fn write_at(&mut self, offset: u32, bytes: &[u8]) -> bool {
        // SAFETY: the caller's callback reads `bytes`.
        self.write_at.is_some_and(|write| unsafe {
            write(self.context, offset, bytes.as_ptr(), bytes.len())
        })
    }
}

// Runs `lookup` on the caller's file and writes its answer.
//
// SAFETY: `file` is null or a `bb_section_file` as section.h describes; `out` is null or writable.
unsafe fn answer<T>(
    file: *const FileView,
    out: *mut T,
    lookup: impl FnOnce(&mut &FileView) -> Option<T>,
) -> bool {
    // SAFETY: as above.
    let Some(mut f) = (unsafe { file.as_ref() }) else {
        return false;
    };
    match lookup(&mut f) {
        Some(value) => {
            // SAFETY: as above.
            unsafe { set(out, value) };
            true
        }
        None => false,
    }
}

/// # Safety
///
/// `spec` is null or a `bb_section_spec`; `out` is null or holds 44 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_header(spec: *const Spec, out: *mut [u8; 44]) {
    // SAFETY: the caller's spec and output.
    if let Some(spec) = unsafe { spec.as_ref() } {
        unsafe { set(out, file::header(spec)) };
    }
}

/// section.h's `bb_section_loaded`.
#[repr(C)]
pub struct Loaded {
    page_count: u16,
    partial: bool,
    consumed: u32,
    total: u32,
    version: u8,
}

/// # Safety
///
/// `file` is null or a `bb_section_file` as section.h describes, `spec` null or a
/// `bb_section_spec`, `out` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_check(
    file: *const FileView,
    spec: *const Spec,
    out: *mut Loaded,
) -> c_int {
    // SAFETY: the caller's file and spec.
    let (Some(mut f), Some(spec)) = (unsafe { file.as_ref() }, unsafe { spec.as_ref() }) else {
        return 1;
    };
    let (code, loaded) = match file::check(&mut f, spec) {
        Ok(l) => {
            let w = l.watermark.unwrap_or_default();
            (
                0,
                Loaded {
                    page_count: l.page_count,
                    partial: l.watermark.is_some(),
                    consumed: w.consumed,
                    total: w.total,
                    version: 0,
                },
            )
        }
        Err(refusal) => {
            let (code, version) = match refusal {
                Refusal::Version(v) => (1, v),
                Refusal::Spec => (2, 0),
                Refusal::Partial => (3, 0),
            };
            (
                code,
                Loaded {
                    page_count: 0,
                    partial: false,
                    consumed: 0,
                    total: 0,
                    version,
                },
            )
        }
    };
    // SAFETY: the caller's output.
    unsafe { set(out, loaded) };
    code
}

/// # Safety
///
/// `file` is null or a `bb_section_file` as section.h describes; `out` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_finished_page_count(
    file: *const FileView,
    out: *mut u16,
) -> bool {
    // SAFETY: as documented.
    unsafe { answer(file, out, |f| file::finished_page_count(f)) }
}

/// # Safety
///
/// As `bb_section_finished_page_count`; `position` and `visible` are null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_page(
    file: *const FileView,
    page: u16,
    position: *mut u32,
    visible: *mut u32,
) -> bool {
    // SAFETY: the caller's file.
    let Some(mut f) = (unsafe { file.as_ref() }) else {
        return false;
    };
    let Some(p) = file::page(&mut f, page) else {
        return false;
    };
    // SAFETY: the caller's outputs.
    unsafe {
        set(position, p.position);
        set(visible, p.visible);
    }
    true
}

/// # Safety
///
/// As `bb_section_finished_page_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_visible_offset(
    file: *const FileView,
    page: u16,
    out: *mut u32,
) -> bool {
    // SAFETY: as documented.
    unsafe { answer(file, out, |f| file::visible_offset(f, page)) }
}

/// # Safety
///
/// As `bb_section_finished_page_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_page_for_visible_offset(
    file: *const FileView,
    offset: u32,
    prefer_first: bool,
    out: *mut u16,
) -> bool {
    // SAFETY: as documented.
    unsafe {
        answer(file, out, |f| {
            file::page_for_visible_offset(f, offset, prefer_first)
        })
    }
}

/// # Safety
///
/// `entries` holds `count` values; `out` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_page_for_visible_offset_in(
    entries: *const LutEntry,
    count: usize,
    offset: u32,
    prefer_first: bool,
    out: *mut u16,
) -> bool {
    // SAFETY: the caller's entries.
    let entries = unsafe { view(entries, count) };
    if entries.is_empty() {
        return false;
    }
    let mut scan = OffsetScan::new(offset, prefer_first);
    let page = entries
        .iter()
        .find_map(|e| scan.push(e.visible))
        .unwrap_or_else(|| scan.finish().0);
    // SAFETY: the caller's output.
    unsafe { set(out, page) };
    true
}

/// # Safety
///
/// As `bb_section_finished_page_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_page_for_paragraph(
    file: *const FileView,
    target: u16,
    out: *mut u16,
) -> bool {
    // SAFETY: as documented.
    unsafe { answer(file, out, |f| file::page_for_paragraph(f, target)) }
}

/// # Safety
///
/// As `bb_section_finished_page_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_page_for_list_item(
    file: *const FileView,
    target: u16,
    out: *mut u16,
) -> bool {
    // SAFETY: as documented.
    unsafe { answer(file, out, |f| file::page_for_list_item(f, target)) }
}

/// # Safety
///
/// As `bb_section_finished_page_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_paragraph_for_page(
    file: *const FileView,
    page: u16,
    out: *mut u16,
) -> bool {
    // SAFETY: as documented.
    unsafe { answer(file, out, |f| file::paragraph_for_page(f, page)) }
}

/// # Safety
///
/// As `bb_section_finished_page_count`; `name` holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_find_anchor(
    file: *const FileView,
    name: *const c_char,
    len: usize,
    out: *mut u16,
) -> bool {
    // SAFETY: the caller's name.
    let name = unsafe { view(name.cast::<u8>(), len) };
    // SAFETY: as documented.
    unsafe { answer(file, out, |f| file::find_anchor(f, name)) }
}

/// # Safety
///
/// `file` is null or a `bb_section_file` as section.h describes; `buf` holds `len` writable
/// bytes; `visit` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_anchors(
    file: *const FileView,
    buf: *mut u8,
    len: usize,
    context: *mut c_void,
    visit: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, u16)>,
) {
    // SAFETY: the caller's file.
    let (Some(mut f), Some(visit)) = (unsafe { file.as_ref() }, visit) else {
        return;
    };
    // SAFETY: the caller's buffer.
    let buf = unsafe { view_mut(buf, len) };
    file::anchors(&mut f, buf, |name, page| {
        // SAFETY: the caller's callback reads `name` while it runs.
        unsafe { visit(context, name.as_ptr().cast(), name.len(), page) };
    });
}

type AnchorFn = unsafe extern "C" fn(*mut c_void, usize, *mut usize, *mut u16) -> *const c_char;

/// # Safety
///
/// `sink` is null or a `bb_section_sink` as section.h describes; `entries` holds `count`
/// values; `anchor` returns, for each index below `anchor_count`, null or a name of `*len`
/// bytes that stays valid during the call; `watermark` is null or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_section_commit(
    sink: *const SinkView,
    at: u32,
    entries: *const LutEntry,
    count: usize,
    anchor_count: usize,
    anchor: Option<AnchorFn>,
    anchor_context: *mut c_void,
    watermark: *const Watermark,
) -> c_int {
    // SAFETY: the caller's sink.
    let Some(mut s) = (unsafe { sink.as_ref() }) else {
        return 2;
    };
    let anchor_count = if anchor.is_some() { anchor_count } else { 0 };
    let name_of = |i: usize| {
        let (mut len, mut page) = (0usize, 0u16);
        // SAFETY: the caller's callback, for an index below `anchor_count`; its name stays
        // valid during the call.
        let name = anchor.map_or(core::ptr::null(), |f| unsafe {
            f(anchor_context, i, &raw mut len, &raw mut page)
        });
        // SAFETY: as above.
        (unsafe { view(name.cast::<u8>(), len) }, page)
    };
    // SAFETY: the caller's watermark and entries.
    let (watermark, entries) = unsafe { (watermark.as_ref().copied(), view(entries, count)) };
    match file::commit(&mut s, at, entries, anchor_count, name_of, watermark) {
        Ok(()) => 0,
        Err(CommitError::MissingPage) => 1,
        Err(CommitError::Write) => 2,
    }
}
