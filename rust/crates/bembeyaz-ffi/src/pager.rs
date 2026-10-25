//! Exports for the chapter parser's page cursor (see `rust/include/bembeyaz/pager.h`).

use bembeyaz_layout::boxes::{Segment, Side};
use bembeyaz_layout::pager::Pager;
use core::mem::offset_of;

// pager.h's bb_box_side and bb_box_segment; the parser checks the same layouts.
const _: () = assert!(
    size_of::<Side>() == 2
        && size_of::<Segment>() == 18
        && offset_of!(Segment, width) == 4
        && offset_of!(Segment, sides) == 8
        && offset_of!(Segment, shaded) == 16
);

/// # Safety
/// `pager` is NULL or a pager from `bb_chapter_pager`, valid for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_pager_y(pager: *const Pager) -> i16 {
    // SAFETY: as documented.
    unsafe { pager.as_ref() }.map_or(0, Pager::y)
}
