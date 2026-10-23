//! Exports for the chapter parser's grid tables (see `rust/include/bembeyaz/table.h`).

use bembeyaz_layout::table::{self, Cell, MAX_COLUMNS, Measure, Row};
use core::ffi::{c_char, c_int, c_void};
use core::slice;

// table.h's bb_table_measure.
const _: () = assert!(
    MAX_COLUMNS == 4 && size_of::<Measure>() == 18 && core::mem::offset_of!(Measure, columns) == 16
);

type Read = unsafe extern "C" fn(*mut c_void, *mut u8, usize) -> usize;
type Advance = unsafe extern "C" fn(*mut c_void, *const c_char, bool) -> c_int;

struct Source {
    read: Read,
    advance: Advance,
    context: *mut c_void,
}

// SAFETY (both calls): the callbacks are the caller's and take the arguments the header
// documents; the buffer and the word stay valid for the call.
impl table::Source for Source {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        unsafe { (self.read)(self.context, buf.as_mut_ptr(), buf.len()) }
    }
    fn advance(&mut self, word: &[u8], bold: bool) -> i32 {
        unsafe { (self.advance)(self.context, word.as_ptr().cast(), bold) }
    }
}

/// # Safety
///
/// `out` is null or writable; `read` and `advance` follow table.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_table_measure_columns(
    read: Option<Read>,
    advance: Option<Advance>,
    context: *mut c_void,
    space_width: c_int,
    out: *mut Measure,
) -> bool {
    let measure = match (read, advance) {
        (Some(read), Some(advance)) => table::measure_columns(
            &mut Source {
                read,
                advance,
                context,
            },
            space_width,
        ),
        _ => None,
    };
    if !out.is_null() {
        // SAFETY: the caller's writable measure.
        unsafe { out.write(measure.unwrap_or_default()) };
    }
    measure.is_some()
}

/// # Safety
///
/// `measure` is null or readable, and `widths` null or has room for `measure->columns` widths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_table_plan_columns(
    measure: *const Measure,
    available: c_int,
    padding: c_int,
    widths: *mut u16,
) -> bool {
    // SAFETY: the caller's readable measure.
    let Some(measure) = (unsafe { measure.as_ref() }) else {
        return false;
    };
    let Some(planned) = table::plan_columns(measure, available, padding) else {
        return false;
    };
    if widths.is_null() {
        return false;
    }
    // SAFETY: room for `columns` widths, which plan_columns checked is at most MAX_COLUMNS.
    let out = unsafe { slice::from_raw_parts_mut(widths, usize::from(measure.columns)) };
    for (slot, width) in out.iter_mut().zip(planned) {
        *slot = width;
    }
    true
}

// table.h's bb_table_cell and bb_table_row; the parser checks the same layout.
const _: () = assert!(
    size_of::<Cell>() == 8
        && core::mem::offset_of!(Cell, padding_right) == 6
        && size_of::<Row>() == 28
        && core::mem::offset_of!(Row, x) == 8
        && core::mem::offset_of!(Row, frame_width) == 16
        && core::mem::offset_of!(Row, top_padding) == 24
        && core::mem::offset_of!(Row, bottom_padding) == 26
);

/// # Safety
/// `cells` is NULL or holds `count` cells; `planned` is NULL or holds `count` widths; `out` is
/// NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_table_plan_row(
    cells: *const Cell,
    count: usize,
    planned: *const u16,
    viewport: u16,
    line_height: i16,
    rtl: bool,
    out: *mut Row,
) -> bool {
    if out.is_null() {
        return false;
    }
    // SAFETY: as documented.
    let cells = unsafe { crate::view(cells, count) };
    // SAFETY: as documented.
    let planned = (!planned.is_null()).then(|| unsafe { crate::view(planned, count) });
    let Some(row) = table::plan_row(cells, planned, viewport, line_height, rtl) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe { out.write(row) };
    true
}
