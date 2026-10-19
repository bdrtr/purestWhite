//! Exports for `lib/XmlParserUtils/XmlParser.h` (see `rust/include/bembeyaz/xml.h`).
//!
//! A parser lives in a Rust heap block that `bb_xml_new` hands out and `bb_xml_free` takes back.
//! Strings passed to the handlers end with a NUL and stay valid until the handler returns.

use alloc::alloc::{Layout, alloc, dealloc};
use alloc::vec::Vec;
use bembeyaz_xml::{Error, Flow, Handler, Parser, StartTag, Str};
use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use core::slice;

/// The callbacks of `bb_xml_handlers`; each returns false to stop parsing.
#[repr(C)]
pub struct Handlers {
    start_element:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const *const c_char, u64) -> bool>,
    end_element: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> bool>,
    text: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> bool>,
    comment: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> bool>,
    processing_instruction:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> bool>,
    cdata_start: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    cdata_end: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    skipped_entity: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> bool>,
}

/// `BB_XML_DEFAULT_LIMIT` in xml.h.
pub(crate) const DEFAULT_LIMIT: usize = 4 * 1024 * 1024;

pub struct XmlParser {
    parser: Parser,
    /// The `atts` array of the start tag being reported: names and values, then NULL.
    atts: Vec<*const c_char>,
}

struct Callbacks<'a> {
    handlers: &'a Handlers,
    context: *mut c_void,
    atts: &'a mut Vec<*const c_char>,
    /// The `atts` array could not grow: the parse fails with NoMemory, not Aborted.
    out_of_memory: bool,
}

fn flow(go_on: bool) -> Flow {
    if go_on {
        Flow::Continue(())
    } else {
        Flow::Break(())
    }
}

pub(crate) fn c(s: Str<'_>) -> *const c_char {
    s.with_nul().as_ptr().cast()
}

/// Fills `atts` with a start tag's names and values, then NULL, as expat's handlers take them.
/// False when it cannot grow.
pub(crate) fn fill_atts(atts: &mut Vec<*const c_char>, tag: &StartTag<'_>) -> bool {
    atts.clear();
    let attributes = tag.attributes();
    if atts.try_reserve(2 * attributes.len() + 1).is_err() {
        return false;
    }
    for (name, value) in attributes.iter() {
        atts.push(c(name));
        atts.push(c(value));
    }
    atts.push(ptr::null());
    true
}

/// The code `feed`'s result maps to, with a stop for lack of memory reported as such.
pub(crate) fn result_code(result: Result<(), Error>, out_of_memory: bool) -> c_int {
    match result {
        Ok(()) => 0,
        Err(Error::Aborted) if out_of_memory => code(Error::NoMemory),
        Err(e) => code(e),
    }
}

impl Handler for Callbacks<'_> {
    fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
        let Some(f) = self.handlers.start_element else {
            return Flow::Continue(());
        };
        if !fill_atts(self.atts, tag) {
            self.out_of_memory = true;
            return Flow::Break(());
        }
        // SAFETY: NUL-terminated strings and a NULL-terminated array, alive during the call.
        flow(unsafe {
            f(
                self.context,
                c(tag.name()),
                self.atts.as_ptr(),
                tag.offset(),
            )
        })
    }

    fn end_element(&mut self, name: Str<'_>) -> Flow {
        // SAFETY: as above.
        self.handlers.end_element.map_or(Flow::Continue(()), |f| {
            flow(unsafe { f(self.context, c(name)) })
        })
    }

    fn text(&mut self, text: &[u8]) -> Flow {
        self.handlers.text.map_or(Flow::Continue(()), |f| {
            flow(unsafe { f(self.context, text.as_ptr().cast(), text.len()) })
        })
    }

    fn comment(&mut self, text: Str<'_>) -> Flow {
        self.handlers.comment.map_or(Flow::Continue(()), |f| {
            flow(unsafe { f(self.context, c(text)) })
        })
    }

    fn processing_instruction(&mut self, target: Str<'_>, data: Str<'_>) -> Flow {
        self.handlers
            .processing_instruction
            .map_or(Flow::Continue(()), |f| {
                flow(unsafe { f(self.context, c(target), c(data)) })
            })
    }

    fn cdata_start(&mut self) -> Flow {
        self.handlers
            .cdata_start
            .map_or(Flow::Continue(()), |f| flow(unsafe { f(self.context) }))
    }

    fn cdata_end(&mut self) -> Flow {
        self.handlers
            .cdata_end
            .map_or(Flow::Continue(()), |f| flow(unsafe { f(self.context) }))
    }

    fn skipped_entity(&mut self, name: Str<'_>) -> Flow {
        self.handlers
            .skipped_entity
            .map_or(Flow::Continue(()), |f| {
                flow(unsafe { f(self.context, c(name)) })
            })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bb_xml_new(limit: usize) -> *mut XmlParser {
    let layout = Layout::new::<XmlParser>();
    // SAFETY: XmlParser is not zero-sized.
    let block = unsafe { alloc(layout) }.cast::<XmlParser>();
    if !block.is_null() {
        // SAFETY: a fresh block of the right layout.
        unsafe {
            block.write(XmlParser {
                parser: Parser::new(limit),
                atts: Vec::new(),
            });
        }
    }
    block
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xml_free(parser: *mut XmlParser) {
    if !parser.is_null() {
        // SAFETY: from bb_xml_new, not freed yet.
        unsafe {
            ptr::drop_in_place(parser);
            dealloc(parser.cast(), Layout::new::<XmlParser>());
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xml_parse(
    parser: *mut XmlParser,
    data: *const c_char,
    len: usize,
    last: bool,
    handlers: *const Handlers,
    context: *mut c_void,
) -> c_int {
    // SAFETY: from bb_xml_new and not in use; handlers is a bb_xml_handlers or NULL.
    let (Some(parser), Some(handlers)) = (unsafe { parser.as_mut() }, unsafe { handlers.as_ref() })
    else {
        return code(Error::NoMemory);
    };
    let data = if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: len bytes at data.
        unsafe { slice::from_raw_parts(data.cast::<u8>(), len) }
    };
    let mut callbacks = Callbacks {
        handlers,
        context,
        atts: &mut parser.atts,
        out_of_memory: false,
    };
    let result = parser.parser.feed(data, last, &mut callbacks);
    result_code(result, callbacks.out_of_memory)
}

pub(crate) fn code(e: Error) -> c_int {
    c_int::try_from(e.code()).unwrap_or(c_int::MAX)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xml_skip_undefined_entities(parser: *mut XmlParser) {
    // SAFETY: from bb_xml_new, not parsing.
    if let Some(p) = unsafe { parser.as_mut() } {
        p.parser.skip_undefined_entities();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xml_error_offset(parser: *const XmlParser) -> u64 {
    // SAFETY: from bb_xml_new.
    unsafe { parser.as_ref() }.map_or(0, |p| p.parser.error_offset())
}

#[unsafe(no_mangle)]
pub extern "C" fn bb_xml_error_message(code: c_int) -> *const c_char {
    let message = match u32::try_from(code) {
        Ok(0) => c"no error",
        Ok(code) => Error::from_code(code).map_or(c"unknown error", Error::message),
        Err(_) => c"unknown error",
    };
    message.as_ptr()
}
