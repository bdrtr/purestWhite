//! Exports for `lib/KOReaderSync/KOReaderSyncMessages` (see
//! `rust/include/bembeyaz/kosync_messages.h`). Output goes to the caller's write or field callback,
//! so no Rust buffer crosses the boundary.

use bembeyaz_kosync::messages::{
    Metadata, Position, Received, Update, create_user_body, parse_progress, progress_body,
};
use core::ffi::{c_char, c_int, c_void};

/// kosync_messages.h's `bb_kosync_text`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Text {
    data: *const c_char,
    len: usize,
}

impl Text {
    /// The bytes; none for NULL.
    ///
    /// # Safety
    /// `data` is NULL or `len` readable bytes that outlive `'a`.
    unsafe fn bytes<'a>(self) -> &'a [u8] {
        // SAFETY: as documented.
        unsafe { crate::view(self.data.cast(), self.len) }
    }
}

/// kosync_messages.h's `bb_kosync_update`.
#[repr(C)]
pub struct BbUpdate {
    document: Text,
    progress: Text,
    percentage: f32,
    has_metadata: bool,
    filename: Text,
    title: Text,
    authors: Text,
    extra_json: Text,
    isbn: Text,
    asin: Text,
    series: Text,
    has_series_index: bool,
    series_index: f32,
    has_position: bool,
    pct_q: u32,
    spine: u16,
    page: u16,
    pages: u16,
    has_para: bool,
    para: u16,
    xpath: Text,
}

/// kosync_messages.h's `bb_kosync_received`.
#[repr(C)]
pub struct BbReceived {
    percentage: f32,
    timestamp: i64,
    has_position: bool,
    pct_q: u32,
    spine: u16,
    page: u16,
    pages: u16,
    has_para: bool,
    para: u16,
}

type Write = unsafe extern "C" fn(*const c_char, usize, *mut c_void);
type Field = unsafe extern "C" fn(c_int, *const c_char, usize, *mut c_void);

/// Hands `bytes` to `write`.
///
/// # Safety
/// `write` takes `context`.
unsafe fn emit(bytes: &[u8], write: Option<Write>, context: *mut c_void) {
    if let Some(write) = write {
        // SAFETY: `bytes` is readable for the call; the caller vouches for write and context.
        unsafe { write(bytes.as_ptr().cast(), bytes.len(), context) };
    }
}

/// # Safety
/// Each text is NULL or readable; `write` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_create_user_body(
    username: Text,
    md5_password: Text,
    write: Option<Write>,
    context: *mut c_void,
) {
    // SAFETY: as documented.
    let body = unsafe { create_user_body(username.bytes(), md5_password.bytes()) };
    // SAFETY: as documented.
    unsafe { emit(&body, write, context) };
}

/// The update `u` describes.
///
/// # Safety
/// Its texts are NULL or readable for `'a`.
unsafe fn update_of<'a>(u: &'a BbUpdate) -> Update<'a> {
    // SAFETY: as documented.
    unsafe {
        Update {
            document: u.document.bytes(),
            progress: u.progress.bytes(),
            percentage: u.percentage,
            metadata: u.has_metadata.then(|| Metadata {
                filename: u.filename.bytes(),
                title: u.title.bytes(),
                authors: u.authors.bytes(),
                extra_json: u.extra_json.bytes(),
                isbn: u.isbn.bytes(),
                asin: u.asin.bytes(),
                series: u.series.bytes(),
                series_index: u.has_series_index.then_some(u.series_index),
            }),
            position: u.has_position.then(|| Position {
                pct_q: u.pct_q,
                spine: u.spine,
                page: u.page,
                pages: u.pages,
                para: u.has_para.then_some(u.para),
                xpath: u.xpath.bytes(),
            }),
        }
    }
}

/// Nothing for a NULL `update`.
///
/// # Safety
/// `update` is NULL or readable, its texts NULL or readable; `write` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_progress_body(
    update: *const BbUpdate,
    extended_metadata: bool,
    rich_progress: bool,
    write: Option<Write>,
    context: *mut c_void,
) {
    // SAFETY: as documented.
    let Some(update) = (unsafe { update.as_ref() }) else {
        return;
    };
    // SAFETY: as documented.
    let body = progress_body(
        &unsafe { update_of(update) },
        extended_metadata,
        rich_progress,
    );
    // SAFETY: as documented.
    unsafe { emit(&body, write, context) };
}

/// kosync_messages.h's `BB_KOSYNC_FIELD_*`.
const FIELD_PROGRESS: c_int = 0;
const FIELD_DEVICE: c_int = 1;
const FIELD_DEVICE_ID: c_int = 2;
const FIELD_XPATH: c_int = 3;

/// # Safety
/// `body` is NULL or `len` readable bytes; `out` is NULL or writable; `field` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_parse_progress(
    body: *const c_char,
    len: usize,
    rich_progress: bool,
    out: *mut BbReceived,
    field: Option<Field>,
    context: *mut c_void,
) -> *const c_char {
    // SAFETY: as documented.
    let body = unsafe { Text { data: body, len }.bytes() };
    let received = match parse_progress(body, rich_progress) {
        Ok(received) => received,
        Err(error) => return error.c_name().as_ptr(),
    };
    // SAFETY: as documented.
    unsafe { hand_over(&received, out, field, context) };
    core::ptr::null()
}

/// `received` into `out` and `field`.
///
/// # Safety
/// As `bb_kosync_parse_progress`.
unsafe fn hand_over(
    received: &Received,
    out: *mut BbReceived,
    field: Option<Field>,
    context: *mut c_void,
) {
    let text = |id: c_int, bytes: &[u8]| {
        if let Some(field) = field {
            // SAFETY: `bytes` is readable for the call; the caller vouches for field and context.
            unsafe { field(id, bytes.as_ptr().cast(), bytes.len(), context) };
        }
    };
    text(FIELD_PROGRESS, &received.progress);
    text(FIELD_DEVICE, &received.device);
    text(FIELD_DEVICE_ID, &received.device_id);
    let position = received.position.as_ref();
    if let Some(p) = position {
        text(FIELD_XPATH, &p.xpath);
    }
    // SAFETY: as documented.
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    *out = BbReceived {
        percentage: received.percentage,
        timestamp: received.timestamp,
        has_position: position.is_some(),
        pct_q: position.map_or(0, |p| p.pct_q),
        spine: position.map_or(0, |p| p.spine),
        page: position.map_or(0, |p| p.page),
        pages: position.map_or(0, |p| p.pages),
        has_para: position.is_some_and(|p| p.para.is_some()),
        para: position.and_then(|p| p.para).unwrap_or(0),
    };
}

// kosync_messages.h's structs, as GCC lays them out on the ESP32-S3 (32-bit) and on the host
// (64-bit), measured with offsetof.
const _: () = {
    use core::mem::offset_of;
    #[cfg(target_pointer_width = "32")]
    let update = (116, 108, 104, 100, 92, 84, 24);
    #[cfg(target_pointer_width = "64")]
    let update = (200, 184, 176, 172, 164, 156, 40);
    assert!(
        size_of::<BbUpdate>() == update.0
            && offset_of!(BbUpdate, xpath) == update.1
            && offset_of!(BbUpdate, para) == update.2
            && offset_of!(BbUpdate, pages) == update.3
            && offset_of!(BbUpdate, pct_q) == update.4
            && offset_of!(BbUpdate, series_index) == update.5
            && offset_of!(BbUpdate, filename) == update.6
    );
    assert!(
        size_of::<BbReceived>() == 40
            && offset_of!(BbReceived, timestamp) == 8
            && offset_of!(BbReceived, has_position) == 16
            && offset_of!(BbReceived, pct_q) == 20
            && offset_of!(BbReceived, has_para) == 30
            && offset_of!(BbReceived, para) == 32
    );
};
