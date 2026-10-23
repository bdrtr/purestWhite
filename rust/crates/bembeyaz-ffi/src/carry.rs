//! Exports for the chapter parser's page breaks (see `rust/include/bembeyaz/carry.h`).

use bembeyaz_layout::carry::{self, Break, Carry};
use core::mem::offset_of;

// carry.h's bb_carry_break, bb_carry and BB_CARRY_MAX_LINES; the parser checks the same layout.
const _: () = assert!(
    size_of::<Break>() == 6
        && offset_of!(Break, paragraph) == 1
        && offset_of!(Break, drop_cap) == 2
        && offset_of!(Break, paragraph_lines) == 3
        && offset_of!(Break, last_line) == 4
        && offset_of!(Break, keep_with_next) == 5
        && size_of::<Carry>() == 2
        && offset_of!(Carry, paragraph_lines) == 1
        && carry::MAX_LINES == 6
);

/// # Safety
/// `end` is NULL or readable; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_carry_lines(end: *const Break, out: *mut Carry) {
    // SAFETY: as documented.
    let Some(end) = (unsafe { end.as_ref() }) else {
        return;
    };
    // SAFETY: as documented.
    unsafe { crate::set(out, carry::lines_to_carry(end)) };
}

#[unsafe(no_mangle)]
pub extern "C" fn bb_carry_keep_after_block(keep: u8, paragraph_lines: u8, heading: bool) -> u8 {
    carry::keep_after_block(keep, paragraph_lines, heading)
}
