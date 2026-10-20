//! Exports for `lib/JsonSax/StreamingJsonParser` (see `rust/include/bembeyaz/json.h`).
//!
//! The C++ side owns the storage of the [`Parser`], which holds no heap blocks. Events go to the
//! C function pointers of a `bb_json_callbacks`; a NULL one skips its event. A callback must not
//! throw: it is called through Rust frames, which do not unwind (the firmware has no exceptions).

use bembeyaz_json::{Events, MAX_NESTING, Parser, TOKEN_BUF_SIZE, Token};
use core::ffi::{c_char, c_void};
use core::mem::{MaybeUninit, align_of, size_of};
use core::slice;

/// `bb_json_parser`: 524 `uint32_t`.
const _: () = assert!(size_of::<Parser>() == 524 * 4 && align_of::<Parser>() <= 4);
/// `BB_JSON_TOKEN_BUF_SIZE` and `BB_JSON_MAX_NESTING` in json.h, which the SDK's readers size by.
const _: () = assert!(TOKEN_BUF_SIZE == 2048 && MAX_NESTING == 32);

type TokenFn = Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize)>;
type EdgeFn = Option<unsafe extern "C" fn(*mut c_void)>;

/// `bb_json_callbacks`, laid out as the SDK's `JsonCallbacks`.
#[repr(C)]
pub struct Callbacks {
    ctx: *mut c_void,
    on_key: TokenFn,
    on_string: TokenFn,
    on_number: TokenFn,
    on_bool: Option<unsafe extern "C" fn(*mut c_void, bool)>,
    on_null: EdgeFn,
    on_object_start: EdgeFn,
    on_object_end: EdgeFn,
    on_array_start: EdgeFn,
    on_array_end: EdgeFn,
}

impl Callbacks {
    fn token(&self, f: TokenFn, token: Token<'_>) {
        if let Some(f) = f {
            // SAFETY: the C callback contract; the pointer is followed by a NUL.
            unsafe {
                f(
                    self.ctx,
                    token.with_nul().as_ptr().cast(),
                    token.bytes().len(),
                )
            };
        }
    }

    fn edge(&self, f: EdgeFn) {
        if let Some(f) = f {
            // SAFETY: the C callback contract.
            unsafe { f(self.ctx) };
        }
    }
}

impl Events for &Callbacks {
    fn key(&mut self, key: Token<'_>) {
        self.token(self.on_key, key);
    }
    fn string(&mut self, value: Token<'_>) {
        self.token(self.on_string, value);
    }
    fn number(&mut self, value: Token<'_>) {
        self.token(self.on_number, value);
    }
    fn boolean(&mut self, value: bool) {
        if let Some(f) = self.on_bool {
            // SAFETY: the C callback contract.
            unsafe { f(self.ctx, value) };
        }
    }
    fn null(&mut self) {
        self.edge(self.on_null);
    }
    fn object_start(&mut self) {
        self.edge(self.on_object_start);
    }
    fn object_end(&mut self) {
        self.edge(self.on_object_end);
    }
    fn array_start(&mut self) {
        self.edge(self.on_array_start);
    }
    fn array_end(&mut self) {
        self.edge(self.on_array_end);
    }
}

/// SAFETY: `parser` was set up by `bb_json_init`.
unsafe fn parser<'a>(parser: *mut c_void) -> Option<&'a mut Parser> {
    unsafe { parser.cast::<Parser>().as_mut() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_json_init(parser: *mut c_void) {
    // SAFETY: storage for a Parser, of any contents.
    if let Some(slot) = unsafe { parser.cast::<MaybeUninit<Parser>>().as_mut() } {
        Parser::init(slot);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_json_reset(parser: *mut c_void) {
    if let Some(parser) = unsafe { self::parser(parser) } {
        parser.reset();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_json_feed(
    parser: *mut c_void,
    callbacks: *const Callbacks,
    data: *const c_char,
    len: usize,
) {
    let (Some(parser), Some(mut callbacks)) = (unsafe { self::parser(parser) }, unsafe {
        callbacks.as_ref()
    }) else {
        return;
    };
    let data = if data.is_null() || len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data.cast::<u8>(), len) }
    };
    parser.feed(data, &mut callbacks);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_json_has_error(parser: *const c_void) -> bool {
    unsafe { parser.cast::<Parser>().as_ref() }.is_some_and(Parser::has_error)
}
