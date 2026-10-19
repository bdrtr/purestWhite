//! Exports for `lib/Epub/Epub/css/CssParser` (see `rust/include/bembeyaz/css.h`).
//!
//! The C++ side owns the storage of [`Rules`] and [`Tokenizer`]; the heap blocks the rules point
//! to are Rust's, freed by `bb_css_rules_free`. Styles and ancestors are written straight into
//! the C++ structs, whose layout both sides check.

use bembeyaz_css::cache::{self, Load, Source, Status};
use bembeyaz_css::{Ancestor, Parse, Query, Rules, Stop, Style, Tokenizer, parse_inline};
use core::ffi::{c_char, c_int, c_void};
use core::mem::{MaybeUninit, align_of, size_of};
use core::slice;

/// `bb_css_rules`: twelve pointers.
const _: () = assert!(
    size_of::<Rules>() <= 12 * size_of::<usize>() && align_of::<Rules>() <= align_of::<usize>()
);
/// `bb_css_tokenizer`: 549 `uint32_t`.
const _: () = assert!(size_of::<Tokenizer>() == 549 * 4 && align_of::<Tokenizer>() <= 4);
/// `BB_CSS_CACHE_VERSION` in css.h.
const _: () = assert!(cache::VERSION == 18);

unsafe fn bytes<'a>(data: *const c_char, len: usize) -> &'a [u8] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data.cast(), len) }
    }
}

/// SAFETY: `rules` was set up by `bb_css_rules_init` and is not in use elsewhere.
unsafe fn rules<'a>(rules: *mut c_void) -> Option<&'a mut Rules> {
    unsafe { rules.cast::<Rules>().as_mut() }
}

unsafe fn rules_ref<'a>(rules: *const c_void) -> Option<&'a Rules> {
    unsafe { rules.cast::<Rules>().as_ref() }
}

/// SAFETY: `tokenizer` was set up by `bb_css_tokenizer_init`.
unsafe fn tokenizer<'a>(tokenizer: *mut c_void) -> Option<&'a mut Tokenizer> {
    unsafe { tokenizer.cast::<Tokenizer>().as_mut() }
}

fn write_style(out: *mut c_void, style: &Style) {
    if !out.is_null() {
        // SAFETY: a CssStyle, whose layout matches Style; it need not hold a valid one.
        unsafe { out.cast::<Style>().write(*style) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_rules_init(rules: *mut c_void) {
    if !rules.is_null() {
        // SAFETY: storage for a Rules, of any contents; Rules::new() is a few words.
        unsafe { rules.cast::<Rules>().write(Rules::new()) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_rules_free(rules: *mut c_void) {
    if let Some(rules) = unsafe { self::rules(rules) } {
        rules.clear();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_rules_len(rules: *const c_void) -> usize {
    unsafe { rules_ref(rules) }.map_or(0, Rules::len)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_rules_has_first_letter(rules: *const c_void) -> bool {
    unsafe { rules_ref(rules) }.is_some_and(Rules::has_first_letter_rules)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_rules_stopped(rules: *const c_void) -> c_int {
    match unsafe { rules_ref(rules) }.and_then(Rules::stopped) {
        None => 0,
        Some(Stop::Limit) => 1,
        Some(Stop::OutOfMemory) => 2,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_tokenizer_init(tokenizer: *mut c_void) {
    // SAFETY: storage for a Tokenizer, of any contents.
    if let Some(slot) = unsafe { tokenizer.cast::<MaybeUninit<Tokenizer>>().as_mut() } {
        Tokenizer::init(slot);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_feed(
    rules: *mut c_void,
    tokenizer: *mut c_void,
    data: *const c_char,
    len: usize,
) {
    if let (Some(rules), Some(tokenizer)) =
        unsafe { (self::rules(rules), self::tokenizer(tokenizer)) }
    {
        tokenizer.feed(rules, unsafe { bytes(data, len) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_finish(rules: *mut c_void, tokenizer: *mut c_void) -> c_int {
    let (Some(rules), Some(tokenizer)) = (unsafe { self::rules(rules) }, unsafe {
        self::tokenizer(tokenizer)
    }) else {
        return 1;
    };
    match tokenizer.finish(rules) {
        Parse::Complete => 0,
        Parse::Partial => 1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_tokenizer_truncated(tokenizer: *const c_void) -> bool {
    unsafe { tokenizer.cast::<Tokenizer>().as_ref() }.is_some_and(Tokenizer::truncated)
}

#[repr(C)]
pub struct BbCssQuery {
    tag: *const c_char,
    tag_len: usize,
    classes: *const c_char,
    classes_len: usize,
    id: *const c_char,
    id_len: usize,
    ancestors: *const Ancestor,
    ancestor_count: usize,
    first_letter: bool,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_resolve(
    rules: *const c_void,
    query: *const BbCssQuery,
    out: *mut c_void,
) {
    let (Some(rules), Some(q)) = (unsafe { rules_ref(rules) }, unsafe { query.as_ref() }) else {
        write_style(out, &Style::DEFAULT);
        return;
    };
    let ancestors = if q.ancestors.is_null() || q.ancestor_count == 0 {
        &[][..]
    } else {
        // SAFETY: CssAncestor has Ancestor's layout (checked in CssParser.cpp).
        unsafe { slice::from_raw_parts(q.ancestors, q.ancestor_count) }
    };
    let query = Query {
        tag: unsafe { bytes(q.tag, q.tag_len) },
        classes: unsafe { bytes(q.classes, q.classes_len) },
        id: unsafe { bytes(q.id, q.id_len) },
        ancestors,
        first_letter: q.first_letter,
    };
    write_style(out, &rules.resolve(&query));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_parse_inline(style: *const c_char, len: usize, out: *mut c_void) {
    write_style(out, &parse_inline(unsafe { bytes(style, len) }));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_make_ancestor(
    tag: *const c_char,
    tag_len: usize,
    classes: *const c_char,
    classes_len: usize,
    id: *const c_char,
    id_len: usize,
    out: *mut Ancestor,
) {
    let ancestor = unsafe {
        Ancestor::new(
            bytes(tag, tag_len),
            bytes(classes, classes_len),
            bytes(id, id_len),
        )
    };
    if !out.is_null() {
        // SAFETY: a CssAncestor, whose layout matches Ancestor.
        unsafe { out.write(ancestor) };
    }
}

type ReadFn = unsafe extern "C" fn(context: *mut c_void, buf: *mut u8, len: usize) -> bool;
type RemainingFn = unsafe extern "C" fn(context: *mut c_void) -> usize;
type SkipFn = unsafe extern "C" fn(context: *mut c_void, len: usize) -> bool;
type WriteFn = unsafe extern "C" fn(context: *mut c_void, data: *const u8, len: usize) -> bool;

#[repr(C)]
pub struct BbCssSource {
    context: *mut c_void,
    read: Option<ReadFn>,
    remaining: Option<RemainingFn>,
    skip: Option<SkipFn>,
}

/// A `BbCssSource` whose callbacks are all set.
struct Callbacks {
    context: *mut c_void,
    read: ReadFn,
    remaining: RemainingFn,
    skip: SkipFn,
}

unsafe fn callbacks(source: *const BbCssSource) -> Option<Callbacks> {
    let source = unsafe { source.as_ref() }?;
    Some(Callbacks {
        context: source.context,
        read: source.read?,
        remaining: source.remaining?,
        skip: source.skip?,
    })
}

impl Source for Callbacks {
    fn read(&mut self, buf: &mut [u8]) -> bool {
        unsafe { (self.read)(self.context, buf.as_mut_ptr(), buf.len()) }
    }

    fn remaining(&mut self) -> usize {
        unsafe { (self.remaining)(self.context) }
    }

    fn skip(&mut self, len: usize) -> bool {
        unsafe { (self.skip)(self.context, len) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_cache_inspect(source: *const BbCssSource) -> c_int {
    let Some(mut source) = (unsafe { callbacks(source) }) else {
        return 2;
    };
    match cache::inspect(&mut source) {
        Status::Complete => 0,
        Status::Partial => 1,
        Status::Invalid => 2,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_cache_load(
    rules: *mut c_void,
    source: *const BbCssSource,
) -> c_int {
    let (Some(rules), Some(mut source)) =
        (unsafe { self::rules(rules) }, unsafe { callbacks(source) })
    else {
        return 2;
    };
    match cache::load(rules, &mut source) {
        Load::Complete => 0,
        Load::LowMemory => 1,
        Load::Invalid => 2,
        Load::StaleVersion => 3,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_css_cache_save(
    rules: *const c_void,
    complete: bool,
    write: Option<WriteFn>,
    context: *mut c_void,
) -> bool {
    let (Some(rules), Some(write)) = (unsafe { rules_ref(rules) }, write) else {
        return false;
    };
    cache::save(rules, complete, |data| unsafe {
        write(context, data.as_ptr(), data.len())
    })
}
