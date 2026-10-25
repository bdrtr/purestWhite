//! Exports for the chapter parser's page cursor (see `rust/include/bembeyaz/pager.h`).

use bembeyaz_layout::boxes::{Segment, Side};
use bembeyaz_layout::pager::{Line, Pager, Pages};
use core::ffi::{c_int, c_void};
use core::mem::{MaybeUninit, offset_of};

// pager.h's bb_pager (opaque), bb_pager_line, bb_box_side, bb_box_segment and BB_BOXES_MAX; the
// parser checks bb_pager_line, bb_box_side and bb_box_segment too.
const _: () = assert!(
    size_of::<Side>() == 2
        && size_of::<Segment>() == 18
        && offset_of!(Segment, width) == 4
        && offset_of!(Segment, sides) == 8
        && offset_of!(Segment, shaded) == 16
        && bembeyaz_layout::boxes::MAX_BOXES == 8
        && size_of::<Pager>() <= 71 * 4
        && align_of::<Pager>() <= 4
        && !core::mem::needs_drop::<Pager>()
        && size_of::<Line>() == 3
        && offset_of!(Line, drop_cap) == 1
        && offset_of!(Line, last_line) == 2
);

/// pager.h's `bb_pages`: the caller's pages.
#[repr(C)]
pub struct Callbacks {
    context: *mut c_void,
    new_page: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    split: Option<unsafe extern "C" fn(*mut c_void, u8, i16) -> bool>,
    emit: Option<unsafe extern "C" fn(*mut c_void, u32)>,
    add_box: Option<unsafe extern "C" fn(*mut c_void, *const Segment)>,
}

// SAFETY (all four): the callbacks are the caller's and take the arguments pager.h documents; a
// missing one does nothing and reports failure.
impl Pages for &Callbacks {
    fn new_page(&mut self) -> bool {
        self.new_page.is_some_and(|f| unsafe { f(self.context) })
    }
    fn split(&mut self, lines: u8, top: i16) -> bool {
        self.split
            .is_some_and(|f| unsafe { f(self.context, lines, top) })
    }
    fn emit(&mut self, visible_offset: u32) {
        if let Some(f) = self.emit {
            unsafe { f(self.context, visible_offset) }
        }
    }
    fn add_box(&mut self, segment: &Segment) {
        if let Some(f) = self.add_box {
            unsafe { f(self.context, segment) }
        }
    }
}

/// Runs `f` on the pager and the pages, unless either is NULL.
///
/// # Safety
/// `pager` is NULL or a `bb_pager` that `bb_pager_init` set up, used by nothing else during the
/// call; `pages` is NULL or a `bb_pages` whose callbacks do not use the pager.
unsafe fn with<R>(
    pager: *mut Pager,
    pages: *const Callbacks,
    otherwise: R,
    f: impl FnOnce(&mut Pager, &mut &Callbacks) -> R,
) -> R {
    // SAFETY: as documented.
    match unsafe { (pager.as_mut(), pages.as_ref()) } {
        (Some(pager), Some(mut pages)) => f(pager, &mut pages),
        _ => otherwise,
    }
}

/// # Safety
/// `pager` is NULL or a `bb_pager`, of any contents.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_init(pager: *mut MaybeUninit<Pager>, page_height: u16) {
    // SAFETY: as documented.
    if let Some(slot) = unsafe { pager.as_mut() } {
        slot.write(Pager::new(page_height));
    }
}

/// Exports a read of the pager.
macro_rules! getter {
    ($name:ident, $ty:ty, $default:expr, $get:expr) => {
        /// # Safety
        /// `pager` is NULL or a `bb_pager` that `bb_pager_init` set up.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(pager: *const Pager) -> $ty {
            // SAFETY: as documented.
            unsafe { pager.as_ref() }.map_or($default, $get)
        }
    };
}

/// Exports a change to the pager that needs no pages.
macro_rules! setter {
    ($name:ident ( $($arg:ident : $ty:ty),* ), $set:expr) => {
        /// # Safety
        /// `pager` is NULL or a `bb_pager` that `bb_pager_init` set up, used by nothing else during
        /// the call.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(pager: *mut Pager $(, $arg: $ty)*) {
            // SAFETY: as documented.
            if let Some(pager) = unsafe { pager.as_mut() } {
                let set: fn(&mut Pager $(, $ty)*) = $set;
                set(pager $(, $arg)*);
            }
        }
    };
}

getter!(bb_pager_y, i16, 0, Pager::y);
getter!(bb_pager_completed, u16, 0, Pager::completed);
getter!(bb_pager_has_page, bool, false, Pager::has_page);
getter!(bb_pager_has_content, bool, false, Pager::has_content);
getter!(
    bb_pager_keep_with_next_carry,
    u8,
    0,
    Pager::keep_with_next_carry
);
getter!(bb_pager_box_empty_height, c_int, 0, Pager::box_empty_height);
getter!(bb_pager_box_full, bool, true, |p: &Pager| p
    .boxes_ref()
    .is_full());

setter!(bb_pager_set_y(y: i16), Pager::set_y);
setter!(bb_pager_advance(rows: c_int), Pager::advance);
setter!(bb_pager_note(top: c_int, bottom: c_int), Pager::note);
setter!(bb_pager_added(), Pager::added);
setter!(bb_pager_set_visible_offset(offset: u32), Pager::set_visible_offset);
setter!(bb_pager_line_added(height: c_int, offset: u32, paragraph: bool), Pager::line_added);
setter!(bb_pager_end_block(heading: bool), Pager::end_block);
setter!(bb_pager_clear_keep_with_next(), Pager::clear_keep_with_next);
setter!(bb_pager_blank_line(line_height: c_int), Pager::blank_line);
setter!(bb_pager_reach(y: c_int), Pager::reach);
setter!(bb_pager_rule_added(thickness: i16, bottom: i16, offset: u32), Pager::rule_added);
setter!(bb_pager_row_separator_added(), Pager::row_separator_added);
setter!(bb_pager_image_added(height: c_int, margin_bottom: i16, offset: u32), Pager::image_added);

/// # Safety
/// `pager` is NULL or a `bb_pager` that `bb_pager_init` set up.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_box_innermost_is(pager: *const Pager, depth: c_int) -> bool {
    // SAFETY: as documented.
    unsafe { pager.as_ref() }.is_some_and(|p| p.boxes_ref().innermost_is(depth))
}

/// # Safety
/// `pager` is NULL or a `bb_pager` that `bb_pager_init` set up.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_row_overflows(
    pager: *const Pager,
    gaps: c_int,
    height: c_int,
) -> bool {
    // SAFETY: as documented.
    unsafe { pager.as_ref() }.is_some_and(|p| p.row_overflows(gaps, height))
}

/// # Safety
/// `pager` is NULL or a `bb_pager` that `bb_pager_init` set up; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_row_separator_top(
    pager: *const Pager,
    page_width: u16,
    out: *mut i16,
) -> bool {
    // SAFETY: as documented.
    let Some(top) = unsafe { pager.as_ref() }.and_then(|p| p.row_separator_top(page_width)) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe { crate::set(out, top) };
    true
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_ensure_page(pager: *mut Pager, pages: *const Callbacks) {
    // SAFETY: as documented.
    unsafe { with(pager, pages, (), |p, h| p.ensure_page(h)) }
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_complete_page(pager: *mut Pager, pages: *const Callbacks) {
    // SAFETY: as documented.
    unsafe { with(pager, pages, (), |p, h| p.complete_page(h)) }
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_page_break(
    pager: *mut Pager,
    pages: *const Callbacks,
    requested: bool,
) {
    // SAFETY: as documented.
    unsafe { with(pager, pages, (), |p, h| p.page_break(h, requested)) }
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_finish(pager: *mut Pager, pages: *const Callbacks, offset: u32) {
    // SAFETY: as documented.
    unsafe { with(pager, pages, (), |p, h| p.finish(h, offset)) }
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_break_carrying(
    pager: *mut Pager,
    pages: *const Callbacks,
    lines: u8,
    paragraph_lines: u8,
    offset: u32,
) -> bool {
    // SAFETY: as documented.
    unsafe {
        with(pager, pages, false, |p, h| {
            p.break_page_carrying(h, lines, paragraph_lines, offset)
        })
    }
}

/// # Safety
/// As `with`'s; `line` is NULL or a `bb_pager_line`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_line_top(
    pager: *mut Pager,
    pages: *const Callbacks,
    height: c_int,
    offset: u32,
    line: *const Line,
) -> i16 {
    // SAFETY: as documented.
    let line = unsafe { line.as_ref() }.copied().unwrap_or_default();
    // SAFETY: as documented.
    unsafe { with(pager, pages, 0, |p, h| p.line_top(h, height, offset, line)) }
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_drop_cap_top(
    pager: *mut Pager,
    pages: *const Callbacks,
    height: c_int,
    offset: u32,
) -> i16 {
    // SAFETY: as documented.
    unsafe { with(pager, pages, 0, |p, h| p.drop_cap_top(h, height, offset)) }
}

/// # Safety
/// As `with`'s; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_rule_top(
    pager: *mut Pager,
    pages: *const Callbacks,
    top: i16,
    total: i16,
    offset: u32,
    out: *mut i16,
) -> bool {
    // SAFETY: as documented.
    let Some(y) = (unsafe { with(pager, pages, None, |p, h| p.rule_top(h, top, total, offset)) })
    else {
        return false;
    };
    // SAFETY: as documented.
    unsafe { crate::set(out, y) };
    true
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_break_table_row(
    pager: *mut Pager,
    pages: *const Callbacks,
    offset: u32,
) -> bool {
    // SAFETY: as documented.
    unsafe { with(pager, pages, false, |p, h| p.break_table_row(h, offset)) }
}

/// # Safety
/// As `with`'s; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_image_top(
    pager: *mut Pager,
    pages: *const Callbacks,
    margin_top: i16,
    height: c_int,
    margin_bottom: i16,
    offset: u32,
    out: *mut i16,
) -> bool {
    // SAFETY: as documented.
    let Some(y) = (unsafe {
        with(pager, pages, None, |p, h| {
            p.image_top(h, margin_top, height, margin_bottom, offset)
        })
    }) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe { crate::set(out, y) };
    true
}

/// # Safety
/// `pager` is NULL or a `bb_pager` that `bb_pager_init` set up, used by nothing else during the
/// call; `sides` is NULL or holds four sides; `padding` is NULL or holds four writable values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_open_box(
    pager: *mut Pager,
    depth: c_int,
    sides: *const [Side; 4],
    shaded: bool,
    left: i16,
    right: i16,
    padding: *mut [i16; 4],
) -> bool {
    // SAFETY: as documented.
    let (Some(pager), Some(sides), Some(padding)) =
        (unsafe { (pager.as_mut(), sides.as_ref(), padding.as_mut()) })
    else {
        return false;
    };
    pager.open_box(depth, *sides, shaded, left, right, padding)
}

/// # Safety
/// As `with`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_close_box(
    pager: *mut Pager,
    pages: *const Callbacks,
    above: c_int,
    below: c_int,
) {
    // SAFETY: as documented.
    unsafe { with(pager, pages, (), |p, h| p.close_box(h, above, below)) }
}
