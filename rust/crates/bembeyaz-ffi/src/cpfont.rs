//! Exports for lib/EpdFont's `SdCardFont` (see `rust/include/bembeyaz/cpfont.h`).

use bembeyaz_font::Source;
use bembeyaz_font::cpfont::{self, Header, IntervalCheck, IntervalFault, Style};
use bembeyaz_font::kern::{self, MiniKern};
use core::ffi::{c_int, c_void};
use core::ptr::null_mut;
use core::slice;

// cpfont.h's bb_cpfont_style, bb_cpfont_header, bb_cpfont_intervals, bb_cpfont_interval_fault,
// BB_CPFONT_FORMAT_VERSION and BB_CPFONT_MAX_STYLES.
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Style>() == 52
            && offset_of!(Style, bitmap_offset) == 32
            && offset_of!(Style, ascender) == 36
            && offset_of!(Style, kern_right_entries) == 42
            && offset_of!(Style, advance_y) == 44
            && offset_of!(Style, present) == 48
            && size_of::<Header>() == 220
            && offset_of!(Header, content_hash) == 208
            && offset_of!(Header, version) == 212
            && offset_of!(Header, two_bit) == 214
            && offset_of!(Header, style_count) == 215
            && offset_of!(Header, failed_entry) == 216
            && size_of::<IntervalCheck>() == 16
            && size_of::<IntervalFault>() == 12
            && offset_of!(IntervalFault, span) == 8
            && align_of::<IntervalCheck>() == 4
            && cpfont::VERSION == 4
            && cpfont::MAX_STYLES == 4
    );
};

type Read = unsafe extern "C" fn(*mut c_void, *mut u8, usize) -> usize;
type Seek = unsafe extern "C" fn(*mut c_void, u32) -> bool;

/// cpfont.h's bb_cpfont_file.
#[repr(C)]
pub struct File {
    read: Option<Read>,
    seek: Option<Seek>,
    context: *mut c_void,
}

// SAFETY (every call): the callbacks are the caller's and take the arguments cpfont.h
// documents; the buffer stays valid for the call.
impl Source for File {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        match self.read {
            Some(read) => unsafe { read(self.context, buf.as_mut_ptr(), buf.len()) }.min(buf.len()),
            None => 0,
        }
    }
    fn seek(&mut self, position: u32) -> bool {
        self.seek
            .is_some_and(|seek| unsafe { seek(self.context, position) })
    }
}

/// # Safety
///
/// `header` is null or writable; `file` is null or a `bb_cpfont_file` as cpfont.h documents.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_read_header(header: *mut Header, file: *mut File) -> c_int {
    // SAFETY: the caller's bb_cpfont_file.
    let (false, Some(file)) = (header.is_null(), unsafe { file.as_mut() }) else {
        return -1;
    };
    // SAFETY: the caller's writable bb_cpfont_header, which may not be set yet.
    let header = unsafe {
        header.write(Header::default());
        &mut *header
    };
    let result = cpfont::read_header(header, file);
    match result {
        Ok(()) => 0,
        Err(error) => c_int::from(error as u8),
    }
}

/// # Safety
///
/// `check` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_intervals_init(check: *mut IntervalCheck, glyph_count: u32) {
    if !check.is_null() {
        // SAFETY: the caller's writable bb_cpfont_intervals, which may not be set up yet.
        unsafe { check.write(IntervalCheck::new(glyph_count)) };
    }
}

/// # Safety
///
/// `check` is null or a `bb_cpfont_intervals` set up by `bb_cpfont_intervals_init`; `fault` is
/// null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_intervals_check(
    check: *mut IntervalCheck,
    first: u32,
    last: u32,
    offset: u32,
    fault: *mut IntervalFault,
) -> bool {
    // SAFETY: the caller's bb_cpfont_intervals.
    let Some(check) = (unsafe { check.as_mut() }) else {
        return false;
    };
    match check.check(first, last, offset) {
        Ok(()) => true,
        Err(f) => {
            if !fault.is_null() {
                // SAFETY: the caller's writable fault, which may not be set yet.
                unsafe { fault.write(f) };
            }
            false
        }
    }
}

/// # Safety
///
/// `check` is null or a `bb_cpfont_intervals` set up by `bb_cpfont_intervals_init`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_intervals_bmp16(check: *const IntervalCheck) -> bool {
    // SAFETY: the caller's bb_cpfont_intervals.
    unsafe { check.as_ref() }.is_some_and(IntervalCheck::bmp16)
}

/// cpfont.h's bb_cpfont_mini_kern.
#[repr(C)]
#[derive(Default)]
pub struct MiniKernResult {
    matrix_bytes: u32,
    left_entries: u16,
    right_entries: u16,
    left_classes: u8,
    right_classes: u8,
    failed_row: u8,
}

const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<MiniKernResult>() == 12
            && offset_of!(MiniKernResult, left_entries) == 4
            && offset_of!(MiniKernResult, right_entries) == 6
            && offset_of!(MiniKernResult, left_classes) == 8
            && offset_of!(MiniKernResult, right_classes) == 9
            && offset_of!(MiniKernResult, failed_row) == 10
    );
};

impl From<&MiniKern> for MiniKernResult {
    fn from(k: &MiniKern) -> Self {
        MiniKernResult {
            matrix_bytes: k.matrix_bytes,
            left_entries: k.left_entries,
            right_entries: k.right_entries,
            left_classes: k.left_classes,
            right_classes: k.right_classes,
            failed_row: 0,
        }
    }
}

type Reserve = unsafe extern "C" fn(
    *mut c_void,
    *const MiniKernResult,
    *mut *mut u8,
    *mut *mut u8,
    *mut *mut i8,
) -> bool;

/// # Safety
///
/// As cpfont.h's `bb_cpfont_build_mini_kern` says: `file` is a `bb_cpfont_file`, `style`
/// readable; `codepoints` holds `count` values, `scratch` `scratch_len` writable bytes, `index`
/// null or `bb_cpfont_kern_index_len(style)` writable values; `index_ready` and `result` are
/// writable; the tables `reserve` gives hold the sizes it was asked for, stay valid until it
/// returns and do not overlap `scratch`, `codepoints` or `index`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_cpfont_build_mini_kern(
    file: *mut File,
    style: *const Style,
    codepoints: *const u32,
    count: usize,
    scratch: *mut u8,
    scratch_len: usize,
    index: *mut u16,
    index_ready: *mut bool,
    reserve: Option<Reserve>,
    context: *mut c_void,
    result: *mut MiniKernResult,
) -> c_int {
    // SAFETY: the caller's arguments, as documented above.
    let (Some(file), Some(style), Some(index_ready), Some(reserve), false, false, false) = (
        unsafe { file.as_mut() },
        unsafe { style.as_ref() },
        unsafe { index_ready.as_mut() },
        reserve,
        result.is_null(),
        codepoints.is_null() && count != 0,
        scratch.is_null() && scratch_len != 0,
    ) else {
        return -1;
    };
    if scratch_len < kern::scratch_len(style, count) {
        return -1;
    }
    // SAFETY: as documented above; empty slices need no memory.
    let codepoints: &[u32] = if count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(codepoints, count) }
    };
    // SAFETY: as documented above; scratch_len is at least 192 here, so scratch is not null.
    let scratch = unsafe { slice::from_raw_parts_mut(scratch, scratch_len) };
    // SAFETY: as documented above.
    let index = (!index.is_null())
        .then(|| unsafe { slice::from_raw_parts_mut(index, kern::index_len(style)) });
    let mut out = MiniKernResult::default();
    let ask = |asked: &MiniKern| {
        out = MiniKernResult::from(asked);
        let (mut left, mut right, mut matrix) = (null_mut(), null_mut(), null_mut());
        // SAFETY: the caller's callback; it fills the three pointers when it returns true.
        if !unsafe { reserve(context, &out, &mut left, &mut right, &mut matrix) }
            || left.is_null()
            || right.is_null()
            || matrix.is_null()
        {
            return None;
        }
        // SAFETY: the caller's tables, at least as large as asked.
        unsafe {
            Some(kern::Tables {
                left: slice::from_raw_parts_mut(left, usize::from(asked.left_entries) * 3),
                right: slice::from_raw_parts_mut(right, usize::from(asked.right_entries) * 3),
                matrix: slice::from_raw_parts_mut(matrix, asked.matrix_bytes as usize),
            })
        }
    };
    let built = kern::build_mini(file, style, codepoints, scratch, index, index_ready, ask);
    let code = match built {
        Ok(kern) => {
            out = MiniKernResult::from(&kern);
            0
        }
        Err(kern::Error::Scratch) => return -1,
        Err(kern::Error::ClassRead) => 1,
        Err(kern::Error::Reserve) => 2,
        Err(kern::Error::RowSeek(row)) => {
            out.failed_row = row;
            3
        }
        Err(kern::Error::RowRead(row)) => {
            out.failed_row = row;
            4
        }
    };
    // SAFETY: the caller's writable result.
    unsafe { result.write(out) };
    code
}

/// # Safety
///
/// `style` is null or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_kern_index_len(style: *const Style) -> usize {
    // SAFETY: the caller's bb_cpfont_style.
    unsafe { style.as_ref() }.map_or(0, kern::index_len)
}

/// # Safety
///
/// `style` is null or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_mini_kern_scratch_len(
    style: *const Style,
    count: usize,
) -> usize {
    // SAFETY: the caller's bb_cpfont_style.
    unsafe { style.as_ref() }.map_or(0, |style| kern::scratch_len(style, count))
}
