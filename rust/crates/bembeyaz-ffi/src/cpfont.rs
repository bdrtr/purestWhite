//! Exports for lib/EpdFont's `SdCardFont` (see `rust/include/bembeyaz/cpfont.h`).

use bembeyaz_font::Source;
use bembeyaz_font::cpfont::{self, Header, IntervalFault, Style};
use bembeyaz_font::glyph::{self, Glyph};
use bembeyaz_font::intervals;
use bembeyaz_font::kern::{self, MiniKern};
use core::ffi::{c_int, c_void};
use core::ptr::null_mut;
use core::slice;

// cpfont.h's bb_cpfont_style, bb_cpfont_header, bb_cpfont_interval_fault,
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
            && size_of::<IntervalFault>() == 12
            && offset_of!(IntervalFault, span) == 8
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

// cpfont.h's bb_cpfont_glyph (EpdGlyph's layout).
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Glyph>() == 16
            && align_of::<Glyph>() == 4
            && offset_of!(Glyph, advance_x) == 2
            && offset_of!(Glyph, left) == 4
            && offset_of!(Glyph, top) == 6
            && offset_of!(Glyph, data_length) == 8
            && offset_of!(Glyph, data_offset) == 12
    );
};

type GlyphIndex = unsafe extern "C" fn(*mut c_void, usize) -> u32;
type OnGlyph = unsafe extern "C" fn(*mut c_void, usize, *const Glyph);

/// # Safety
///
/// `file` is a `bb_cpfont_file`, `style` readable, `seeks` and `failed` null or writable; the
/// callbacks take the arguments cpfont.h documents.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_cpfont_read_glyphs(
    file: *mut File,
    style: *const Style,
    two_bit: bool,
    count: usize,
    index: Option<GlyphIndex>,
    on_glyph: Option<OnGlyph>,
    context: *mut c_void,
    seeks: *mut u32,
    failed: *mut u32,
) -> c_int {
    // SAFETY: the caller's arguments, as documented above.
    let (Some(file), Some(style), Some(index), Some(on_glyph)) = (
        unsafe { file.as_mut() },
        unsafe { style.as_ref() },
        index,
        on_glyph,
    ) else {
        return -1;
    };
    // SAFETY (both closures): the caller's callbacks.
    let indices = (0..count).map(|i| unsafe { index(context, i) });
    let result = glyph::read_glyphs(file, style, two_bit, indices, |i, g| unsafe {
        on_glyph(context, i, &g)
    });
    let (code, made, at) = match result {
        Ok(made) => (0, made, 0),
        Err(glyph::Error::Seek(at)) => (1, 0, at),
        Err(glyph::Error::Read(at)) => (2, 0, at),
    };
    // SAFETY: the caller's writable outputs, if given.
    unsafe {
        if !seeks.is_null() {
            seeks.write(made);
        }
        if !failed.is_null() {
            failed.write(at);
        }
    }
    code
}

/// # Safety
///
/// `file` is a `bb_cpfont_file`, `style` readable, `out` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_read_glyph(
    file: *mut File,
    style: *const Style,
    two_bit: bool,
    index: u32,
    out: *mut Glyph,
) -> c_int {
    // SAFETY: the caller's arguments, as documented above.
    let (Some(file), Some(style), false) = (
        unsafe { file.as_mut() },
        unsafe { style.as_ref() },
        out.is_null(),
    ) else {
        return -1;
    };
    // SAFETY: the caller's writable glyph.
    match glyph::read_glyphs(file, style, two_bit, [index], |_, g| unsafe {
        out.write(g)
    }) {
        Ok(_) => 0,
        Err(glyph::Error::Seek(_)) => 1,
        Err(glyph::Error::Read(_)) => 2,
    }
}

/// # Safety
///
/// `file` is a `bb_cpfont_file`, `style` readable; `glyphs` holds `glyph_count` writable
/// records, `order` `count` values and `arena` `arena_len` writable bytes (each may be null when
/// its length is 0); `seeks` is null or writable.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_cpfont_read_bitmaps(
    file: *mut File,
    style: *const Style,
    glyphs: *mut Glyph,
    glyph_count: usize,
    order: *const u32,
    count: usize,
    arena: *mut u8,
    arena_len: usize,
    seeks: *mut u32,
) -> c_int {
    // SAFETY: the caller's arguments, as documented above.
    let (Some(file), Some(style)) = (unsafe { file.as_mut() }, unsafe { style.as_ref() }) else {
        return -1;
    };
    if (glyphs.is_null() && glyph_count != 0)
        || (order.is_null() && count != 0)
        || (arena.is_null() && arena_len != 0)
    {
        return -1;
    }
    // SAFETY: as documented above; empty slices need no memory.
    let (glyphs, order, arena) = unsafe {
        (
            if glyph_count == 0 {
                &mut [][..]
            } else {
                slice::from_raw_parts_mut(glyphs, glyph_count)
            },
            if count == 0 {
                &[][..]
            } else {
                slice::from_raw_parts(order, count)
            },
            if arena_len == 0 {
                &mut [][..]
            } else {
                slice::from_raw_parts_mut(arena, arena_len)
            },
        )
    };
    let (code, made) = match glyph::read_bitmaps(file, style, glyphs, order, arena) {
        Ok(made) => (0, made),
        Err(glyph::BitmapError::Arena(_)) => (-1, 0),
        Err(glyph::BitmapError::Seek(_)) => (1, 0),
        Err(glyph::BitmapError::Read(_)) => (2, 0),
    };
    if !seeks.is_null() {
        // SAFETY: the caller's writable count.
        unsafe { seeks.write(made) };
    }
    code
}

/// cpfont.h's bb_cpfont_interval_table.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IntervalTable {
    entries: *mut u8,
    count: u32,
    compact: bool,
    shared: bool,
}

/// cpfont.h's bb_cpfont_interval_load.
#[repr(C)]
#[derive(Default)]
pub struct IntervalLoad {
    record: u32,
    first: u32,
    last: u32,
    bytes: u32,
    fault: IntervalFault,
    owner: u8,
    compact: bool,
}

const _: () = {
    use core::mem::offset_of;
    let pointer = size_of::<usize>();
    assert!(
        offset_of!(IntervalTable, count) == pointer
            && offset_of!(IntervalTable, compact) == pointer + 4
            && offset_of!(IntervalTable, shared) == pointer + 5
            && size_of::<IntervalTable>() == pointer * 2 + if pointer == 8 { 0 } else { 4 }
            && size_of::<IntervalLoad>() == 32
            && offset_of!(IntervalLoad, bytes) == 12
            && offset_of!(IntervalLoad, fault) == 16
            && offset_of!(IntervalLoad, owner) == 28
            && offset_of!(IntervalLoad, compact) == 29
    );
};

impl IntervalTable {
    // SAFETY: `entries` holds `count` records of its form for 'a, or is null.
    unsafe fn view<'a>(self) -> Option<intervals::Table<'a>> {
        if self.entries.is_null() {
            return None;
        }
        let bytes = intervals::Table::bytes(self.count, self.compact);
        // SAFETY: as above; an empty table needs no memory.
        let entries = if bytes == 0 {
            &[][..]
        } else {
            unsafe { slice::from_raw_parts(self.entries, bytes) }
        };
        Some(intervals::Table {
            entries,
            compact: self.compact,
        })
    }
}

type ReserveTable = unsafe extern "C" fn(*mut c_void, usize) -> *mut u8;

/// # Safety
///
/// As cpfont.h's `bb_cpfont_load_intervals` says: `file` is a `bb_cpfont_file`, `style`
/// readable, `earlier` null or `earlier_count` pointers that are null or loaded tables, `table`
/// and `result` writable; `reserve` returns null or memory for the records it was asked for.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_cpfont_load_intervals(
    file: *mut File,
    style: *const Style,
    earlier: *const *const IntervalTable,
    earlier_count: usize,
    table: *mut IntervalTable,
    reserve: Option<ReserveTable>,
    context: *mut c_void,
    result: *mut IntervalLoad,
) -> c_int {
    // SAFETY: the caller's arguments, as documented above.
    let (Some(file), Some(style), Some(reserve), false, false, false) = (
        unsafe { file.as_mut() },
        unsafe { style.as_ref() },
        reserve,
        table.is_null(),
        result.is_null(),
        earlier.is_null() && earlier_count != 0,
    ) else {
        return -1;
    };
    const STYLES: usize = cpfont::MAX_STYLES as usize;
    let mut owners: [Option<IntervalTable>; STYLES] = [None; STYLES];
    for (k, owner) in owners.iter_mut().enumerate().take(earlier_count) {
        // SAFETY: the caller's array of table pointers.
        *owner = unsafe { (*earlier.add(k)).as_ref() }.copied();
    }
    // SAFETY: the caller's loaded tables.
    let views = owners.map(|o| o.and_then(|t| unsafe { t.view() }));
    let mut out = IntervalLoad {
        owner: u8::MAX,
        ..IntervalLoad::default()
    };
    let mut entries = null_mut();
    let ask = |_: bool, bytes: usize| {
        // SAFETY: the caller's callback.
        entries = unsafe { reserve(context, bytes) };
        if entries.is_null() {
            return None;
        }
        // SAFETY: memory for the bytes asked for; an empty table needs none.
        Some(if bytes == 0 {
            &mut [][..]
        } else {
            unsafe { slice::from_raw_parts_mut(entries, bytes) }
        })
    };
    let loaded = intervals::load(file, style, &views[..earlier_count.min(STYLES)], ask);
    let own = |compact: bool| IntervalTable {
        entries,
        count: style.interval_count,
        compact,
        shared: false,
    };
    let code = match loaded {
        Ok(intervals::Loaded::Shared(k)) => {
            // Only a loaded earlier table can be shared.
            let Some(mut shared) = owners.get(k).copied().flatten() else {
                return -1;
            };
            shared.shared = true;
            // SAFETY: the caller's writable table.
            unsafe { table.write(shared) };
            out.owner = u8::try_from(k).unwrap_or(u8::MAX);
            out.compact = shared.compact;
            0
        }
        Ok(intervals::Loaded::Own(compact)) => {
            // SAFETY: the caller's writable table.
            unsafe { table.write(own(compact)) };
            out.compact = compact;
            0
        }
        Err(intervals::Error::Seek) => 1,
        Err(intervals::Error::CheckRead(j)) => {
            out.record = j;
            2
        }
        Err(intervals::Error::Invalid(j, first, last, fault)) => {
            out.record = j;
            out.first = first;
            out.last = last;
            out.fault = fault;
            3
        }
        Err(intervals::Error::SeekBack) => 4,
        Err(intervals::Error::Reserve(compact)) => {
            out.compact = compact;
            5
        }
        Err(intervals::Error::CompactRead(j)) => {
            // The reserved memory goes to the table, so the caller frees it with the style.
            // SAFETY: the caller's writable table.
            unsafe { table.write(own(true)) };
            out.record = j;
            out.compact = true;
            6
        }
        Err(intervals::Error::FullRead) => {
            // SAFETY: the caller's writable table; as above.
            unsafe { table.write(own(false)) };
            7
        }
    };
    #[allow(clippy::cast_possible_truncation)]
    {
        out.bytes = intervals::Table::bytes(style.interval_count, out.compact) as u32;
    }
    // SAFETY: the caller's writable result.
    unsafe { result.write(out) };
    code
}

/// # Safety
///
/// `table` is null or a loaded `bb_cpfont_interval_table`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_find_glyph(table: *const IntervalTable, codepoint: u32) -> i32 {
    // SAFETY: the caller's loaded table.
    let found = unsafe { table.as_ref() }
        .and_then(|t| unsafe { t.view() })
        .and_then(|t| t.find(codepoint));
    found.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1)
}
