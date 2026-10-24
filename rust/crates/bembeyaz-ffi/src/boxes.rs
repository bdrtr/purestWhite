//! Exports for the chapter parser's framed boxes (see `rust/include/bembeyaz/boxes.h`).

use bembeyaz_layout::boxes::{Boxes, MAX_BOXES, Segment, Side};
use core::ffi::c_int;
use core::mem::{MaybeUninit, offset_of};

// boxes.h's bb_boxes (opaque), bb_box_side, bb_box_segment and BB_BOXES_MAX; the parser checks the
// same layout.
const _: () = assert!(
    size_of::<Boxes>() <= 57 * 4
        && align_of::<Boxes>() <= 4
        && !core::mem::needs_drop::<Boxes>()
        && size_of::<Side>() == 2
        && size_of::<Segment>() == 18
        && offset_of!(Segment, width) == 4
        && offset_of!(Segment, sides) == 8
        && offset_of!(Segment, shaded) == 16
        && MAX_BOXES == 8
);

/// # Safety
/// `boxes` is NULL or a `bb_boxes`, of any contents.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_boxes_init(boxes: *mut MaybeUninit<Boxes>) {
    // SAFETY: as documented.
    if let Some(slot) = unsafe { boxes.as_mut() } {
        slot.write(Boxes::default());
    }
}

/// # Safety
/// `boxes` is NULL or a `bb_boxes` that `bb_boxes_init` set up.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_boxes_full(boxes: *const Boxes) -> bool {
    // SAFETY: as documented.
    unsafe { boxes.as_ref() }.is_none_or(Boxes::is_full)
}

/// # Safety
/// `boxes` is NULL or a `bb_boxes` that `bb_boxes_init` set up.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_boxes_innermost_is(boxes: *const Boxes, depth: c_int) -> bool {
    // SAFETY: as documented.
    unsafe { boxes.as_ref() }.is_some_and(|b| b.innermost_is(depth))
}

/// # Safety
/// `boxes` is NULL or a `bb_boxes` that `bb_boxes_init` set up, used by nothing else during the
/// call; `sides` is NULL or holds four sides; `padding` is NULL or holds four writable values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_boxes_open(
    boxes: *mut Boxes,
    depth: c_int,
    sides: *const [Side; 4],
    shaded: bool,
    left: i16,
    right: i16,
    padding: *mut [i16; 4],
) -> bool {
    // SAFETY: as documented.
    let (Some(boxes), Some(sides), Some(padding)) =
        (unsafe { (boxes.as_mut(), sides.as_ref(), padding.as_mut()) })
    else {
        return false;
    };
    boxes.open(depth, *sides, shaded, left, right, padding)
}

/// # Safety
/// `boxes` is NULL or a `bb_boxes` that `bb_boxes_init` set up, used by nothing else during the
/// call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_boxes_note(boxes: *mut Boxes, top: c_int, bottom: c_int) {
    // SAFETY: as documented.
    if let Some(boxes) = unsafe { boxes.as_mut() } {
        boxes.note(top, bottom);
    }
}

/// # Safety
/// `boxes` is NULL or a `bb_boxes` that `bb_boxes_init` set up, used by nothing else during the
/// call; `out` is NULL or holds `BB_BOXES_MAX` writable segments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_boxes_break(
    boxes: *mut Boxes,
    page_height: u16,
    out: *mut [MaybeUninit<Segment>; MAX_BOXES],
) -> usize {
    // SAFETY: as documented.
    let (Some(boxes), Some(out)) = (unsafe { (boxes.as_mut(), out.as_mut()) }) else {
        return 0;
    };
    let mut slots = out.iter_mut();
    let mut count = 0;
    // At most one segment per open box, and no more than MAX_BOXES are open.
    boxes.break_page(page_height, |segment| {
        if let Some(slot) = slots.next() {
            slot.write(segment);
            count += 1;
        }
    });
    count
}

/// # Safety
/// `boxes` is NULL or a `bb_boxes` that `bb_boxes_init` set up.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_boxes_empty_height(boxes: *const Boxes) -> c_int {
    // SAFETY: as documented.
    unsafe { boxes.as_ref() }.map_or(0, Boxes::empty_height)
}

/// # Safety
/// `boxes` is NULL or a `bb_boxes` that `bb_boxes_init` set up, used by nothing else during the
/// call; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_boxes_close(
    boxes: *mut Boxes,
    placed: bool,
    top: c_int,
    page_height: u16,
    out: *mut Segment,
) -> bool {
    // SAFETY: as documented.
    let Some(boxes) = (unsafe { boxes.as_mut() }) else {
        return false;
    };
    let Some(segment) = boxes.close(placed.then_some(top), page_height) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe { crate::set(out, segment) };
    true
}
