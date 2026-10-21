//! Exports for `lib/KOReaderSync/ChapterXPathResolver` (see `rust/include/bembeyaz/kosync.h`).
//!
//! A resolver lives in a Rust heap block that `bb_kosync_new` hands out and `bb_kosync_free` takes back;
//! a locator likewise with `bb_kosync_locator_new` and `bb_kosync_locator_free`.

use alloc::alloc::{Layout, alloc, dealloc};
use bembeyaz_kosync::locate::{Locator, Step, XPath};
use bembeyaz_kosync::{Resolver, Target};
use core::ffi::{c_char, c_int};
use core::ptr;
use core::slice;

use crate::xml::code;

/// `BB_KOSYNC_PARAGRAPH` and the others in kosync.h.
const PARAGRAPH: c_int = 0;
const OFFSET: c_int = 1;
const OFFSET_INCLUSIVE: c_int = 2;
const COUNT: c_int = 3;

/// NULL for an unknown kind or when out of memory.
#[unsafe(no_mangle)]
pub extern "C" fn bb_kosync_new(
    kind: c_int,
    target: u64,
    spine_index: u32,
    limit: usize,
) -> *mut Resolver {
    let target = match kind {
        PARAGRAPH => Target::Paragraph(u32::try_from(target).unwrap_or(u32::MAX)),
        OFFSET => Target::Offset {
            target,
            inclusive: false,
        },
        OFFSET_INCLUSIVE => Target::Offset {
            target,
            inclusive: true,
        },
        COUNT => Target::Count,
        _ => return ptr::null_mut(),
    };
    // SAFETY: Resolver is not zero-sized.
    let block = unsafe { alloc(Layout::new::<Resolver>()) }.cast::<Resolver>();
    if !block.is_null() {
        // SAFETY: a fresh block of the right layout.
        unsafe { block.write(Resolver::new(target, spine_index, limit)) };
    }
    block
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not freed yet, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_free(resolver: *mut Resolver) {
    if !resolver.is_null() {
        // SAFETY: from bb_kosync_new, not freed yet.
        unsafe {
            ptr::drop_in_place(resolver);
            dealloc(resolver.cast(), Layout::new::<Resolver>());
        }
    }
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not in use, or NULL (`BB_XML_NO_MEMORY`); `data` points
/// to `len` bytes; `last` ends the chapter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_parse(
    resolver: *mut Resolver,
    data: *const c_char,
    len: usize,
    last: bool,
) -> c_int {
    // SAFETY: as documented.
    let Some(resolver) = (unsafe { resolver.as_mut() }) else {
        return code(bembeyaz_xml::Error::NoMemory);
    };
    let data = if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: len bytes at data.
        unsafe { slice::from_raw_parts(data.cast::<u8>(), len) }
    };
    let result = if last {
        resolver.finish()
    } else {
        resolver.feed(data)
    };
    result.map_or_else(code, |()| 0)
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not in use, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_failed(resolver: *const Resolver) -> bool {
    // SAFETY: as documented.
    unsafe { resolver.as_ref() }.is_none_or(Resolver::failed)
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not in use, or NULL; `len` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_xpath(
    resolver: *const Resolver,
    len: *mut usize,
) -> *const c_char {
    // SAFETY: as documented.
    let xpath = unsafe { resolver.as_ref() }.and_then(Resolver::xpath);
    // SAFETY: as documented.
    if let Some(len) = unsafe { len.as_mut() } {
        *len = xpath.map_or(0, <[u8]>::len);
    }
    xpath.map_or(ptr::null(), |x| x.as_ptr().cast())
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not in use, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_visible_codepoints(resolver: *const Resolver) -> u64 {
    // SAFETY: as documented.
    unsafe { resolver.as_ref() }.map_or(0, Resolver::visible_codepoints)
}

// kosync.h's bb_kosync_path and bb_kosync_step.
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Step>() == 16
            && offset_of!(Step, index) == 12
            && size_of::<XPath>() == 280
            && offset_of!(XPath, step_count) == 16
            && offset_of!(XPath, steps) == 20
            && offset_of!(XPath, body_text) == 276
            && offset_of!(XPath, chapter_start) == 277
    );
};

/// # Safety
/// `xpath` is NULL or `len` readable bytes; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_parse_xpath(xpath: *const c_char, len: usize, out: *mut XPath) {
    let text = if xpath.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: len bytes at xpath.
        unsafe { slice::from_raw_parts(xpath.cast::<u8>(), len) }
    };
    // SAFETY: as documented.
    if let Some(out) = unsafe { out.as_mut() } {
        *out = XPath::parse(text);
    }
}

/// `BB_KOSYNC_LOCATE_*` in kosync.h.
const LOCATE_FORWARD: c_int = 0;
const LOCATE_PARAGRAPH: c_int = 1;
const LOCATE_STEPS: c_int = 2;
const LOCATE_STEPS_RELAXED: c_int = 3;
const LOCATE_BODY_TEXT: c_int = 4;

/// NULL for an unknown mode, a NULL `xpath` outside `LOCATE_FORWARD`, or when out of memory.
///
/// # Safety
/// `xpath` is NULL or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_locator_new(
    mode: c_int,
    xpath: *const XPath,
    forward_target: u64,
) -> *mut Locator {
    // SAFETY: as documented.
    let xpath = unsafe { xpath.as_ref() };
    if !matches!(
        (mode, xpath),
        (LOCATE_FORWARD, _) | (LOCATE_PARAGRAPH..=LOCATE_BODY_TEXT, Some(_))
    ) {
        return ptr::null_mut();
    }
    // SAFETY: Locator is not zero-sized.
    let block = unsafe { alloc(Layout::new::<Locator>()) }.cast::<Locator>();
    if block.is_null() {
        return block;
    }
    // SAFETY: a fresh block of the right layout. Set up in place: a Locator is over 600 bytes.
    let locator = unsafe {
        block.write(Locator::EMPTY);
        &mut *block
    };
    match (mode, xpath) {
        (LOCATE_PARAGRAPH, Some(x)) => {
            locator.start_paragraph(x.paragraph, x.char_offset, x.text_node)
        }
        (LOCATE_STEPS | LOCATE_STEPS_RELAXED, Some(x)) => locator.start_steps(
            &x.steps,
            x.step_count,
            x.char_offset,
            x.text_node,
            mode == LOCATE_STEPS_RELAXED,
        ),
        (LOCATE_BODY_TEXT, Some(x)) => locator.start_body_text(x.char_offset, x.text_node),
        _ => locator.start_forward(forward_target),
    }
    block
}

/// # Safety
/// `locator` is from `bb_kosync_locator_new` and not freed yet, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_locator_free(locator: *mut Locator) {
    if !locator.is_null() {
        // SAFETY: from bb_kosync_locator_new, not freed yet.
        unsafe {
            ptr::drop_in_place(locator);
            dealloc(locator.cast(), Layout::new::<Locator>());
        }
    }
}

/// # Safety
/// `locator` is from `bb_kosync_locator_new` and not in use, or NULL; `data` points to `len`
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_locator_feed(
    locator: *mut Locator,
    data: *const u8,
    len: usize,
) {
    // SAFETY: as documented.
    let Some(locator) = (unsafe { locator.as_mut() }) else {
        return;
    };
    if !data.is_null() && len > 0 {
        // SAFETY: len bytes at data.
        locator.feed(unsafe { slice::from_raw_parts(data, len) });
    }
}

// kosync.h's bb_kosync_location.
const _: () = assert!(
    core::mem::offset_of!(Location, total_visible) == 16
        && core::mem::offset_of!(Location, anchor_len) == 32 + size_of::<*const c_char>()
);

/// `bb_kosync_location`.
#[repr(C)]
pub struct Location {
    found: bool,
    paragraph_count: i32,
    paragraph_at_match: i32,
    list_item_at_match: i32,
    total_visible: u64,
    target_visible: u64,
    anchor: *const c_char,
    anchor_len: usize,
}

/// # Safety
/// `locator` is from `bb_kosync_locator_new` and not in use, or NULL; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_locator_get_location(
    locator: *const Locator,
    out: *mut Location,
) {
    // SAFETY: as documented.
    let (Some(l), Some(out)) = (unsafe { locator.as_ref() }, unsafe { out.as_mut() }) else {
        return;
    };
    let anchor = l.anchor();
    *out = Location {
        found: l.found(),
        paragraph_count: l.paragraph_count(),
        paragraph_at_match: l.paragraph_at_match(),
        list_item_at_match: l.list_item_at_match(),
        total_visible: l.total_visible(),
        target_visible: l.target_visible(),
        anchor: anchor.map_or(ptr::null(), |a| a.as_ptr().cast()),
        anchor_len: anchor.map_or(0, <[u8]>::len),
    };
}
