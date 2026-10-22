//! Exports for src/ClippingStore (see `rust/include/bembeyaz/clippings.h`).

use crate::io::{FileView, SinkView};
use crate::{room, view, view_mut};
use bembeyaz_clippings::{self as clip, Field, ID_LEN, Load, LoadError, Record, Replacement, Save};
use core::ffi::{c_char, c_int, c_void};

// Record must match bb_clip_record in clippings.h.
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Record>() == 168
            && offset_of!(Record, timestamp) == 16
            && offset_of!(Record, sync_revision) == 32
            && offset_of!(Record, id) == 40
            && offset_of!(Record, pending_upload) == 105
            && offset_of!(Record, chapter) == 106
            && offset_of!(Record, text_offset) == 156
            && offset_of!(Record, text_length) == 160
            && clip::TEXT_MAX == 4096
            && Field::Path as u8 == 2
    );
};

/// clippings.h's `bb_clip_target`.
#[repr(C)]
pub struct TargetView {
    context: *mut c_void,
    text: Option<unsafe extern "C" fn(*mut c_void, u8, usize) -> *mut c_char>,
    begin: Option<unsafe extern "C" fn(*mut c_void, u16) -> bool>,
    record: Option<unsafe extern "C" fn(*mut c_void, *const Record)>,
}

impl Load for &TargetView {
    fn text(&mut self, field: Field, len: usize) -> Option<&mut [u8]> {
        let f = self.text?;
        // SAFETY: the caller's callback gives `len` bytes of room.
        unsafe { room(f(self.context, field as u8, len).cast(), len) }
    }
    fn begin(&mut self, count: u16) -> bool {
        // SAFETY: the caller's callback.
        self.begin
            .is_some_and(|f| unsafe { f(self.context, count) })
    }
    fn record(&mut self, record: &Record) {
        if let Some(f) = self.record {
            // SAFETY: the caller's callback reads the record.
            unsafe { f(self.context, record) };
        }
    }
}

/// # Safety
///
/// `file` and `target` are null or as clippings.h describes; `buf` holds `buf_len` writable
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_clip_load(
    file: *const FileView,
    buf: *mut u8,
    buf_len: usize,
    max_count: u16,
    target: *const TargetView,
) -> c_int {
    // SAFETY: as documented.
    let (Some(mut f), Some(mut t)) = (unsafe { file.as_ref() }, unsafe { target.as_ref() }) else {
        return 1;
    };
    // SAFETY: the caller's buffer.
    let buf = unsafe { view_mut(buf, buf_len) };
    match clip::load(&mut f, buf, max_count, &mut t) {
        Ok(()) => 0,
        Err(LoadError::Damaged) => 1,
        Err(LoadError::NoRoom) => 2,
    }
}

/// clippings.h's `bb_clip_records`.
#[repr(C)]
pub struct RecordsView {
    context: *mut c_void,
    record: Option<unsafe extern "C" fn(*mut c_void, u16, *mut Record)>,
}

struct Records<'a>(
    &'a RecordsView,
    unsafe extern "C" fn(*mut c_void, u16, *mut Record),
);

impl Save for Records<'_> {
    fn record(&mut self, index: u16, out: &mut Record) {
        // SAFETY: the caller's callback fills the record.
        unsafe { (self.1)(self.0.context, index, out) };
    }
}

/// # Safety
///
/// `out`, `source` and `records` are null or as clippings.h describes; `header` and
/// `header_len` are null or hold 3 strings and their lengths; `text` is null or holds
/// `text_len` bytes; `offsets` holds `count` writable values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_clip_save(
    out: *const SinkView,
    source: *const FileView,
    header: *const [*const c_char; 3],
    header_len: *const [usize; 3],
    count: u16,
    records: *const RecordsView,
    replacement: u16,
    text: *const c_char,
    text_len: usize,
    offsets: *mut u32,
) -> bool {
    // SAFETY: as documented.
    let (Some(mut out), Some(names), Some(lens), Some(records)) = (unsafe {
        (
            out.as_ref(),
            header.as_ref(),
            header_len.as_ref(),
            records.as_ref(),
        )
    }) else {
        return false;
    };
    let Some(record) = records.record else {
        return false;
    };
    let mut strings: [&[u8]; 3] = [&[]; 3];
    for (s, (&name, &len)) in strings.iter_mut().zip(names.iter().zip(lens)) {
        // SAFETY: the caller's strings.
        *s = unsafe { view(name.cast::<u8>(), len) };
    }
    // SAFETY: as documented.
    let mut source = unsafe { source.as_ref() };
    let new = (!text.is_null()).then(|| Replacement {
        index: replacement,
        // SAFETY: the caller's text.
        text: unsafe { view(text.cast::<u8>(), text_len) },
    });
    // SAFETY: the caller's offsets.
    let offsets = unsafe { view_mut(offsets, usize::from(count)) };
    clip::save(
        &mut out,
        source.as_mut(),
        strings,
        count,
        &mut Records(records, record),
        new.as_ref(),
        offsets,
    )
}

/// # Safety
///
/// `journal` is null or as io.h describes; `offset` is null or readable and writable; `id` is
/// null or holds 65 writable bytes; `known` reads a zero-ended id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_clip_next_deletion(
    journal: *const FileView,
    offset: *mut u32,
    id: *mut [u8; ID_LEN],
    context: *mut c_void,
    known: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> bool>,
) -> bool {
    // SAFETY: as documented.
    let (Some(mut f), Some(offset), Some(id), Some(known)) =
        (unsafe { (journal.as_ref(), offset.as_mut(), id.as_mut(), known) })
    else {
        return false;
    };
    clip::next_deletion(&mut f, offset, id, |id| {
        // SAFETY: the caller's callback; the id ends in a zero.
        unsafe { known(context, id.as_ptr().cast()) }
    })
}
