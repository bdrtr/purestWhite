//! Exports for `lib/InflateStream` (see `rust/include/bembeyaz/inflate.h`).
//!
//! The C++ side owns the memory: it hands in a block for [`BbInflate`] (and
//! one for the window when streaming) from the heap or the lent framebuffer,
//! and frees it without calling back, since nothing here needs dropping.

use bembeyaz_inflate::{Error, Format, Inflater, Input, Status, WINDOW_SIZE, Window};
use core::ffi::{c_char, c_int, c_void};
use core::mem::{self, MaybeUninit};
use core::{ptr, slice};

pub const BB_INFLATE_MORE: c_int = 0;
pub const BB_INFLATE_DONE: c_int = 1;

/// Errors map to -1, -2, ... in the order of [`Error::ALL`].
fn status(error: Error) -> c_int {
    let index = Error::ALL.iter().position(|&e| e == error).unwrap_or(0);
    -c_int::try_from(index + 1).unwrap_or(c_int::MAX)
}

type FillFn = unsafe extern "C" fn(context: *mut c_void, data: *mut *const u8) -> usize;

/// Input from one buffer or from a C fill callback.
struct CInput {
    data: *const u8,
    len: usize,
    fill: Option<FillFn>,
    context: *mut c_void,
}

impl CInput {
    const EMPTY: Self = Self {
        data: ptr::null(),
        len: 0,
        fill: None,
        context: ptr::null_mut(),
    };
}

impl Input for CInput {
    fn fill_buf(&mut self) -> &[u8] {
        if self.len == 0
            && let Some(fill) = self.fill
        {
            let mut data = ptr::null();
            let len = unsafe { fill(self.context, &raw mut data) };
            if len == 0 || data.is_null() {
                self.fill = None; // the end of the input; never ask again
            } else {
                self.data = data;
                self.len = len;
            }
        }
        if self.len == 0 {
            return &[];
        }
        // Valid until the next fill, per the fill contract.
        unsafe { slice::from_raw_parts(self.data, self.len) }
    }

    fn consume(&mut self, n: usize) {
        let n = n.min(self.len);
        self.data = self.data.wrapping_add(n);
        self.len -= n;
    }
}

enum Mode {
    /// `start` is where the first read wrote; each read must continue there.
    OneShot {
        start: *mut u8,
        written: usize,
    },
    Streaming(Window<'static>),
}

pub struct BbInflate {
    inflater: Inflater,
    mode: Mode,
    input: CInput,
}

// The C++ side frees the block without running a destructor.
const _: () = assert!(!mem::needs_drop::<BbInflate>());
// BB_INFLATE_WINDOW_SIZE in inflate.h.
const _: () = assert!(WINDOW_SIZE == 32768);

#[unsafe(no_mangle)]
pub extern "C" fn bb_inflate_state_size() -> usize {
    mem::size_of::<BbInflate>()
}

#[unsafe(no_mangle)]
pub extern "C" fn bb_inflate_state_align() -> usize {
    mem::align_of::<BbInflate>()
}

/// Sets up a raw DEFLATE stream in `state`. A null `window` selects one-shot
/// mode. Returns `state`, or null when a block is too small or misaligned.
///
/// # Safety
/// `state` and `window` must be writable for their lengths and outlive the
/// stream; both may hold anything.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_inflate_init(
    state: *mut c_void,
    state_len: usize,
    window: *mut u8,
    window_len: usize,
) -> *mut BbInflate {
    let state = state.cast::<BbInflate>();
    if state.is_null() || !state.is_aligned() || state_len < mem::size_of::<BbInflate>() {
        return ptr::null_mut();
    }
    let mode = if window.is_null() {
        Mode::OneShot {
            start: ptr::null_mut(),
            written: 0,
        }
    } else if window_len < WINDOW_SIZE {
        return ptr::null_mut();
    } else {
        // Lent framebuffer or fresh heap: initialise it before referencing it.
        Mode::Streaming(Window::new(unsafe {
            window.write_bytes(0, WINDOW_SIZE);
            &mut *window.cast::<[u8; WINDOW_SIZE]>()
        }))
    };
    // Field by field: the ~10 KiB decoder must not pass through the stack.
    unsafe {
        let inflater = (&raw mut (*state).inflater).cast::<MaybeUninit<Inflater>>();
        Inflater::init(&mut *inflater, Format::Deflate);
        (&raw mut (*state).mode).write(mode);
        (&raw mut (*state).input).write(CInput::EMPTY);
    }
    state
}

/// The whole input in one buffer, valid for the life of the stream.
///
/// # Safety
/// `stream` comes from [`bb_inflate_init`]; `data` is readable for `len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_inflate_set_source(
    stream: *mut BbInflate,
    data: *const u8,
    len: usize,
) {
    let stream = unsafe { &mut *stream };
    stream.input = CInput {
        data,
        len: if data.is_null() { 0 } else { len },
        ..CInput::EMPTY
    };
}

/// Input pulled on demand: `fill` points `*data` at the next bytes and
/// returns their count, valid until the next call; 0 ends the input.
///
/// # Safety
/// `stream` comes from [`bb_inflate_init`]; `fill` honours that contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_inflate_set_fill(
    stream: *mut BbInflate,
    fill: Option<FillFn>,
    context: *mut c_void,
) {
    let stream = unsafe { &mut *stream };
    stream.input = CInput {
        fill,
        context,
        ..CInput::EMPTY
    };
}

/// Declares a zlib header and Adler-32 trailer; call before the first read.
///
/// # Safety
/// `stream` comes from [`bb_inflate_init`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_inflate_set_zlib(stream: *mut BbInflate) {
    unsafe { (*stream).inflater.set_format(Format::Zlib) };
}

/// Fills `dest` until `max` bytes or the end of the stream and stores the
/// count in `*produced`, which on an error is the output decoded before it.
/// Returns `BB_INFLATE_MORE`, `BB_INFLATE_DONE` or a negative error. In
/// one-shot mode each `dest` must follow the previous one.
///
/// # Safety
/// `stream` comes from [`bb_inflate_init`]; `dest` is writable for `max`
/// bytes (it may be uninitialised), and in one-shot mode the earlier output
/// in front of it is still intact.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_inflate_read(
    stream: *mut BbInflate,
    dest: *mut u8,
    max: usize,
    produced: *mut usize,
) -> c_int {
    let BbInflate {
        inflater,
        mode,
        input,
    } = unsafe { &mut *stream };
    let (n, result) = if max == 0 {
        // Nothing to write; only reports whether the stream has ended.
        match mode {
            Mode::OneShot { .. } => inflater.decompress_into(input, &mut [], 0),
            Mode::Streaming(window) => window.read(inflater, input, &mut []),
        }
    } else if dest.is_null() {
        (0, Err(Error::BadOutput))
    } else {
        match mode {
            Mode::OneShot { start, written } => {
                if start.is_null() {
                    *start = dest;
                }
                // Back-references resolve in the bytes in front of `dest`, so
                // they must be this stream's earlier output.
                match written
                    .checked_add(max)
                    .filter(|_| dest == start.wrapping_add(*written))
                {
                    Some(len) => {
                        // The decoder takes an initialised slice; once per
                        // entry in practice, so the zeroing is cheap.
                        unsafe { dest.write_bytes(0, max) };
                        let out = unsafe { slice::from_raw_parts_mut(*start, len) };
                        let (n, result) = inflater.decompress_into(input, out, *written);
                        *written += n;
                        (n, result)
                    }
                    None => (0, Err(Error::BadOutput)),
                }
            }
            Mode::Streaming(window) => {
                let out = unsafe { slice::from_raw_parts_mut(dest.cast::<MaybeUninit<u8>>(), max) };
                window.read_uninit(inflater, input, out)
            }
        }
    };
    unsafe { *produced = n };
    match result {
        Ok(Status::Done) => BB_INFLATE_DONE,
        Ok(Status::More) => BB_INFLATE_MORE,
        Err(error) => status(error),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bb_inflate_status_str(code: c_int) -> *const c_char {
    let error = usize::try_from(-i64::from(code) - 1)
        .ok()
        .and_then(|i| Error::ALL.get(i));
    let text = match (code, error) {
        (BB_INFLATE_MORE, _) => c"more",
        (BB_INFLATE_DONE, _) => c"done",
        (_, Some(error)) => error.message(),
        (_, None) => c"unknown",
    };
    text.as_ptr()
}
