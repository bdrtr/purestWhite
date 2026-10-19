//! The part of expat's API the FreeInk SDK calls (`libs/network/Opds` and `CatalogList`, which
//! leave the XML library to the firmware), over bembeyaz-xml. Declared in
//! `rust/include/expat.h`.
//!
//! What it does not do: protocol encodings (`XML_ParserCreate` with an encoding returns NULL),
//! suspending (`XML_StopParser` always stops for good), and line numbers (0).

use crate::xml::{c, fill_atts, result_code};
use alloc::alloc::{Layout, alloc, dealloc};
use alloc::vec::Vec;
use bembeyaz_xml::{Error, Flow, Handler, Parser, StartTag, Str};
use core::cell::Cell;
use core::ffi::{c_char, c_int, c_ulong, c_void};
use core::ptr;
use core::slice;

type StartHandler = unsafe extern "C" fn(*mut c_void, *const c_char, *const *const c_char);
type EndHandler = unsafe extern "C" fn(*mut c_void, *const c_char);
type TextHandler = unsafe extern "C" fn(*mut c_void, *const c_char, c_int);

/// `XML_STATUS_ERROR` and `XML_STATUS_OK`.
const STATUS_ERROR: c_int = 0;
const STATUS_OK: c_int = 1;
/// `XML_ERROR_INVALID_ARGUMENT`.
const INVALID_ARGUMENT: c_int = 41;

pub struct ExpatParser {
    parser: Parser,
    user_data: *mut c_void,
    start: Option<StartHandler>,
    end: Option<EndHandler>,
    text: Option<TextHandler>,
    /// What `XML_GetBuffer` hands out.
    buffer: Vec<u8>,
    atts: Vec<*const c_char>,
    error: c_int,
    /// Set by `XML_StopParser`, which a handler may call while the parser runs: the only field
    /// it touches, which `feed` borrows on its own.
    stopped: Cell<bool>,
}

struct Callbacks<'a> {
    user_data: *mut c_void,
    start: Option<StartHandler>,
    end: Option<EndHandler>,
    text: Option<TextHandler>,
    atts: &'a mut Vec<*const c_char>,
    stopped: &'a Cell<bool>,
    out_of_memory: bool,
}

impl Callbacks<'_> {
    fn flow(&self) -> Flow {
        if self.stopped.get() {
            Flow::Break(())
        } else {
            Flow::Continue(())
        }
    }
}

impl Handler for Callbacks<'_> {
    fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
        let Some(f) = self.start else {
            return self.flow();
        };
        if !fill_atts(self.atts, tag) {
            self.out_of_memory = true;
            return Flow::Break(());
        }
        // SAFETY: NUL-terminated strings and a NULL-terminated array, alive during the call.
        unsafe { f(self.user_data, c(tag.name()), self.atts.as_ptr()) };
        self.flow()
    }

    fn end_element(&mut self, name: Str<'_>) -> Flow {
        if let Some(f) = self.end {
            // SAFETY: as above.
            unsafe { f(self.user_data, c(name)) };
        }
        self.flow()
    }

    fn text(&mut self, text: &[u8]) -> Flow {
        if let Some(f) = self.text {
            for piece in text.chunks(c_int::MAX as usize) {
                let len = c_int::try_from(piece.len()).unwrap_or(c_int::MAX);
                // SAFETY: len bytes at piece.
                unsafe { f(self.user_data, piece.as_ptr().cast(), len) };
            }
        }
        self.flow()
    }
}

/// Parses `data` with the handlers of `parser`, whose fields are borrowed one by one so that a
/// handler may call `XML_StopParser`.
///
/// SAFETY: `parser` is from `XML_ParserCreate` and not freed; `data` does not alias its fields
/// other than `buffer`.
unsafe fn feed(parser: *mut ExpatParser, data: &[u8], last: bool) -> c_int {
    let code = {
        // SAFETY: disjoint fields of a live parser.
        let (inner, atts, stopped) = unsafe {
            (
                &mut (*parser).parser,
                &mut (*parser).atts,
                &(*parser).stopped,
            )
        };
        let mut callbacks = Callbacks {
            // SAFETY: plain reads of a live parser.
            user_data: unsafe { (*parser).user_data },
            start: unsafe { (*parser).start },
            end: unsafe { (*parser).end },
            text: unsafe { (*parser).text },
            atts,
            stopped,
            out_of_memory: false,
        };
        let result = inner.feed(data, last, &mut callbacks);
        result_code(result, callbacks.out_of_memory)
    };
    // SAFETY: the borrows above have ended.
    unsafe { (*parser).error = code };
    if code == 0 { STATUS_OK } else { STATUS_ERROR }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ParserCreate(encoding: *const c_char) -> *mut ExpatParser {
    if !encoding.is_null() {
        return ptr::null_mut();
    }
    let layout = Layout::new::<ExpatParser>();
    // SAFETY: ExpatParser is not zero-sized.
    let block = unsafe { alloc(layout) }.cast::<ExpatParser>();
    if !block.is_null() {
        // SAFETY: a fresh block of the right layout.
        unsafe {
            block.write(ExpatParser {
                parser: Parser::new(crate::xml::DEFAULT_LIMIT),
                user_data: ptr::null_mut(),
                start: None,
                end: None,
                text: None,
                buffer: Vec::new(),
                atts: Vec::new(),
                error: 0,
                stopped: Cell::new(false),
            });
        }
    }
    block
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ParserFree(parser: *mut ExpatParser) {
    if !parser.is_null() {
        // SAFETY: from XML_ParserCreate, not freed yet.
        unsafe {
            ptr::drop_in_place(parser);
            dealloc(parser.cast(), Layout::new::<ExpatParser>());
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetUserData(parser: *mut ExpatParser, user_data: *mut c_void) {
    if !parser.is_null() {
        // SAFETY: from XML_ParserCreate. A field write, no reference to the whole parser: a
        // handler may call this while the parser runs (it applies from the next XML_Parse).
        unsafe { (*parser).user_data = user_data };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetElementHandler(
    parser: *mut ExpatParser,
    start: Option<StartHandler>,
    end: Option<EndHandler>,
) {
    if !parser.is_null() {
        // SAFETY: as above.
        unsafe {
            (*parser).start = start;
            (*parser).end = end;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetCharacterDataHandler(
    parser: *mut ExpatParser,
    handler: Option<TextHandler>,
) {
    if !parser.is_null() {
        // SAFETY: as above.
        unsafe { (*parser).text = handler };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_Parse(
    parser: *mut ExpatParser,
    s: *const c_char,
    len: c_int,
    is_final: c_int,
) -> c_int {
    if parser.is_null() {
        return STATUS_ERROR;
    }
    let Ok(len) = usize::try_from(len) else {
        // SAFETY: from XML_ParserCreate.
        unsafe { (*parser).error = INVALID_ARGUMENT };
        return STATUS_ERROR;
    };
    if s.is_null() && len != 0 {
        // SAFETY: as above.
        unsafe { (*parser).error = INVALID_ARGUMENT };
        return STATUS_ERROR;
    }
    let data = if len == 0 {
        &[]
    } else {
        // SAFETY: len bytes at s, which are the caller's.
        unsafe { slice::from_raw_parts(s.cast::<u8>(), len) }
    };
    // SAFETY: from XML_ParserCreate; data is not the parser's.
    unsafe { feed(parser, data, is_final != 0) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetBuffer(parser: *mut ExpatParser, len: c_int) -> *mut c_void {
    // SAFETY: from XML_ParserCreate, not parsing.
    let Some(p) = (unsafe { parser.as_mut() }) else {
        return ptr::null_mut();
    };
    let Ok(len) = usize::try_from(len) else {
        p.error = INVALID_ARGUMENT;
        return ptr::null_mut();
    };
    p.buffer.clear();
    if len > crate::xml::DEFAULT_LIMIT || p.buffer.try_reserve_exact(len).is_err() {
        p.error = c_int::try_from(Error::NoMemory.code()).unwrap_or(1);
        return ptr::null_mut();
    }
    p.buffer.resize(len, 0);
    p.buffer.as_mut_ptr().cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ParseBuffer(
    parser: *mut ExpatParser,
    len: c_int,
    is_final: c_int,
) -> c_int {
    if parser.is_null() {
        return STATUS_ERROR;
    }
    // SAFETY: from XML_ParserCreate; `buffer` is a field feed leaves alone.
    let buffer = unsafe { &(*parser).buffer };
    let Some(data) = usize::try_from(len).ok().and_then(|len| buffer.get(..len)) else {
        // SAFETY: as above.
        unsafe { (*parser).error = INVALID_ARGUMENT };
        return STATUS_ERROR;
    };
    // SAFETY: as above.
    unsafe { feed(parser, data, is_final != 0) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_StopParser(parser: *mut ExpatParser, _resumable: u8) -> c_int {
    if parser.is_null() {
        return STATUS_ERROR;
    }
    // SAFETY: from XML_ParserCreate; `stopped` is a Cell that feed only reads through `&`.
    unsafe { (*parser).stopped.set(true) };
    STATUS_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetErrorCode(parser: *const ExpatParser) -> c_int {
    if parser.is_null() {
        return 0;
    }
    // SAFETY: from XML_ParserCreate; a field read.
    unsafe { (*parser).error }
}

#[unsafe(no_mangle)]
pub extern "C" fn XML_ErrorString(code: c_int) -> *const c_char {
    if code == INVALID_ARGUMENT {
        return c"invalid argument".as_ptr();
    }
    crate::xml::bb_xml_error_message(code)
}

#[unsafe(no_mangle)]
pub extern "C" fn XML_GetCurrentLineNumber(_parser: *const ExpatParser) -> c_ulong {
    0
}
