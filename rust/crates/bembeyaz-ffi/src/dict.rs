//! Exports for src/util/Dictionary (see `rust/include/bembeyaz/dict.h`).

use bembeyaz_dict::{
    Fail, HEADER_LEN, Header, IDX_SUFFIX, Index, Location, QIDX_MAGIC, SIDX_MAGIC, SYN_SUFFIX,
    Scan, Source, WORD_MAX, Word,
};
use core::ffi::c_void;
use core::mem::MaybeUninit;

/// dict.h's `bb_dict_read_fn`.
type ReadFn = unsafe extern "C" fn(file: *mut c_void, pos: u32, buf: *mut u8, len: usize) -> i32;

/// dict.h's `bb_dict_source`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CSource {
    read: Option<ReadFn>,
    file: *mut c_void,
}

impl Source for CSource {
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> Result<usize, Fail> {
        let read = self.read.ok_or(Fail::Read)?;
        // SAFETY: dict.h's contract for `read` and `file`; `buf` is writable for its length.
        let n = unsafe { read(self.file, pos, buf.as_mut_ptr(), buf.len()) };
        match n {
            -1 => Err(Fail::Seek),
            _ => usize::try_from(n)
                .map(|n| n.min(buf.len()))
                .map_err(|_| Fail::Read),
        }
    }
}

/// dict.h's `bb_dict_index`.
#[repr(C)]
pub struct CIndex {
    source: CSource,
    sidecar: CSource,
    source_size: u32,
    sample_count: u32,
    entry_count: u32,
}

/// dict.h's `bb_dict_location`.
#[repr(C)]
pub struct CLocation {
    offset: u32,
    size: u32,
    found: bool,
    read_error: bool,
}

// dict.h's layouts and values; Dictionary.cpp checks the layouts too.
const P: usize = size_of::<usize>();
const _: () = assert!(
    size_of::<CSource>() == 2 * P
        && size_of::<CIndex>() == (4 * P + 12).next_multiple_of(P)
        && core::mem::offset_of!(CIndex, entry_count) == 4 * P + 8
        && size_of::<CLocation>() == 12
        && size_of::<Scan>() <= 6 * 4
        && align_of::<Scan>() <= 4
        && !core::mem::needs_drop::<Scan>()
        && WORD_MAX == 255
        && HEADER_LEN == 24
        && QIDX_MAGIC == 0x5844_4951
        && SIDX_MAGIC == 0x5844_4953
        && IDX_SUFFIX == 8
        && SYN_SUFFIX == 4
);

fn location(at: Location) -> CLocation {
    CLocation {
        offset: at.offset,
        size: at.size,
        found: at.found,
        read_error: at.read_error,
    }
}

/// # Safety
/// `text` is NULL or holds `len` bytes; `offsets64` and `html` are writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_ifo(
    text: *const u8,
    len: usize,
    offsets64: *mut bool,
    html: *mut bool,
) {
    // SAFETY: as documented.
    let facts = bembeyaz_dict::ifo(unsafe { crate::view(text, len) });
    // SAFETY: as documented.
    unsafe {
        offsets64.write(facts.offsets64);
        html.write(facts.html);
    }
}

/// # Safety
/// `raw` holds 24 bytes; `samples`, `size` and `entries` are writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_header(
    raw: *const [u8; HEADER_LEN],
    magic: u32,
    samples: *mut u32,
    size: *mut u32,
    entries: *mut u32,
) -> bool {
    // SAFETY: as documented.
    let Some(h) = (unsafe { raw.as_ref() }).and_then(|raw| bembeyaz_dict::header(raw, magic))
    else {
        return false;
    };
    // SAFETY: as documented.
    unsafe {
        samples.write(h.sample_count);
        size.write(h.source_size);
        entries.write(h.entry_count);
    }
    true
}

/// # Safety
/// `out` is writable for 24 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_header_bytes(
    magic: u32,
    samples: u32,
    size: u32,
    entries: u32,
    out: *mut [u8; HEADER_LEN],
) {
    let counts = Header {
        sample_count: samples,
        source_size: size,
        entry_count: entries,
    };
    // SAFETY: as documented.
    if let Some(out) = unsafe { out.as_mut() } {
        *out = bembeyaz_dict::header_bytes(magic, &counts);
    }
}

/// # Safety
/// `scan` is NULL or a `bb_dict_scan`, of any contents.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_scan_init(
    scan: *mut MaybeUninit<Scan>,
    suffix: u32,
    source_size: u32,
) {
    // SAFETY: as documented.
    if let Some(slot) = unsafe { scan.as_mut() } {
        slot.write(Scan::new(suffix, source_size));
    }
}

/// # Safety
/// `scan` is NULL or a `bb_dict_scan` that `bb_dict_scan_init` set up; `bytes` is NULL or holds
/// `len` bytes; `sample` is callable with `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_scan_feed(
    scan: *mut Scan,
    bytes: *const u8,
    len: usize,
    sample: Option<unsafe extern "C" fn(context: *mut c_void, offset: u32) -> bool>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    let (Some(scan), Some(sample)) = (unsafe { scan.as_mut() }, sample) else {
        return false;
    };
    // SAFETY: as documented.
    let bytes = unsafe { crate::view(bytes, len) };
    // SAFETY: as documented.
    scan.feed(bytes, |offset| unsafe { sample(context, offset) })
}

/// # Safety
/// `scan` is a `bb_dict_scan` that `bb_dict_scan_init` set up; `entries` and `samples` are
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_scan_counts(
    scan: *const Scan,
    entries: *mut u32,
    samples: *mut u32,
) {
    // SAFETY: as documented.
    if let Some(scan) = unsafe { scan.as_ref() } {
        // SAFETY: as documented.
        unsafe {
            entries.write(scan.entries);
            samples.write(scan.samples);
        }
    }
}

/// The index `c` describes.
fn index<'a>(
    c: &CIndex,
    source: &'a mut CSource,
    sidecar: &'a mut CSource,
) -> Index<'a, CSource, CSource> {
    Index {
        source,
        sidecar,
        source_size: c.source_size,
        sample_count: c.sample_count,
        entry_count: c.entry_count,
    }
}

/// # Safety
/// `idx` is valid (its sources as dict.h describes); `target` is NULL or holds `len` bytes;
/// `headword` is writable for 256 bytes, apart from the rest (NULL finds nothing); `out` is
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_locate(
    idx: *const CIndex,
    target: *const u8,
    len: usize,
    headword: *mut [u8; WORD_MAX + 1],
    out: *mut CLocation,
) {
    // SAFETY: as documented.
    let (Some(c), Some(headword)) = (unsafe { idx.as_ref() }, unsafe { headword.as_mut() }) else {
        // SAFETY: as documented.
        unsafe { out.write(location(Location::default())) };
        return;
    };
    let (mut source, mut sidecar) = (c.source, c.sidecar);
    let mut word = Word::new(headword);
    // SAFETY: as documented.
    let at =
        index(c, &mut source, &mut sidecar).locate(unsafe { crate::view(target, len) }, &mut word);
    // SAFETY: as documented.
    unsafe { out.write(location(at)) };
}

/// # Safety
/// As `bb_dict_locate`, and `syn` is valid too.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_locate_synonym(
    syn: *const CIndex,
    idx: *const CIndex,
    target: *const u8,
    len: usize,
    headword: *mut [u8; WORD_MAX + 1],
    out: *mut CLocation,
) {
    // SAFETY: as documented.
    let (Some(s), Some(i), Some(headword)) =
        (unsafe { syn.as_ref() }, unsafe { idx.as_ref() }, unsafe {
            headword.as_mut()
        })
    else {
        // SAFETY: as documented.
        unsafe { out.write(location(Location::default())) };
        return;
    };
    let (mut syn_source, mut syn_sidecar, mut source, mut sidecar) =
        (s.source, s.sidecar, i.source, i.sidecar);
    let mut word = Word::new(headword);
    let at = bembeyaz_dict::locate_synonym(
        &mut index(s, &mut syn_source, &mut syn_sidecar),
        &mut index(i, &mut source, &mut sidecar),
        // SAFETY: as documented.
        unsafe { crate::view(target, len) },
        &mut word,
    );
    // SAFETY: as documented.
    unsafe { out.write(location(at)) };
}

/// # Safety
/// `word` is NULL or holds `len` bytes; `out` is NULL or writable for `out_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_clean_word(
    word: *const u8,
    len: usize,
    out: *mut u8,
    out_len: usize,
) -> usize {
    // SAFETY: as documented.
    let (word, out) = unsafe { (crate::view(word, len), crate::view_mut(out, out_len)) };
    bembeyaz_dict::clean_word(word, out)
}

/// # Safety
/// `word` is NULL or holds `len` bytes; `variant` is callable with `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dict_stem_variants(
    word: *const u8,
    len: usize,
    variant: Option<
        unsafe extern "C" fn(
            context: *mut c_void,
            stem: usize,
            ending: *const u8,
            ending_len: usize,
        ),
    >,
    context: *mut c_void,
) {
    let Some(variant) = variant else {
        return;
    };
    // SAFETY: as documented.
    let word = unsafe { crate::view(word, len) };
    bembeyaz_dict::stem_variants(word, |v| {
        // SAFETY: as documented; the ending is static.
        unsafe { variant(context, v.stem, v.ending.as_ptr(), v.ending.len()) };
    });
}

use bembeyaz_dict::dictzip::{self, Fail as ZipFail, Slice};

/// dict.h's `bb_dictzip_file`.
#[repr(C)]
pub struct CZipFile {
    read: Option<unsafe extern "C" fn(file: *mut c_void, buf: *mut u8, len: usize) -> i32>,
    seek: Option<unsafe extern "C" fn(file: *mut c_void, pos: u32) -> bool>,
    position: Option<unsafe extern "C" fn(file: *mut c_void) -> u32>,
    size: u32,
    file: *mut c_void,
}

impl dictzip::File for &CZipFile {
    fn read_exact(&mut self, buf: &mut [u8]) -> bool {
        let Some(read) = self.read else {
            return false;
        };
        // SAFETY: dict.h's contract for `read` and `file`; `buf` is writable for its length.
        let n = unsafe { read(self.file, buf.as_mut_ptr(), buf.len()) };
        usize::try_from(n).is_ok_and(|n| n == buf.len())
    }

    fn seek(&mut self, pos: u32) -> bool {
        // SAFETY: dict.h's contract for `seek` and `file`.
        self.seek
            .is_some_and(|seek| unsafe { seek(self.file, pos) })
    }

    fn position(&self) -> u32 {
        // SAFETY: dict.h's contract for `position` and `file`.
        self.position
            .map_or(0, |position| unsafe { position(self.file) })
    }

    fn size(&self) -> u32 {
        self.size
    }
}

/// dict.h's `bb_dictzip_header`.
#[repr(C)]
pub struct CZipHeader {
    data_offset: u32,
    total_size: u32,
    chunk_length: u16,
}

const _: () = assert!(size_of::<CZipHeader>() == 12 && size_of::<CZipFile>() == 5 * P);

/// dict.h's BB_DICTZIP_* results.
fn code(result: Result<(), ZipFail>) -> u8 {
    match result {
        Ok(()) => 0,
        Err(ZipFail::Read) => 1,
        Err(ZipFail::Corrupt) => 2,
        Err(ZipFail::Memory) => 3,
    }
}

/// # Safety
/// `file` is valid as dict.h describes, read from the file's start; `table` is callable with
/// `context` and returns NULL or room for the offsets it was asked for, used by nothing else until
/// this returns; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dictzip_parse(
    file: *const CZipFile,
    table: Option<unsafe extern "C" fn(context: *mut c_void, len: usize) -> *mut u32>,
    context: *mut c_void,
    out: *mut CZipHeader,
) -> u8 {
    // SAFETY: as documented.
    let (Some(mut f), Some(table)) = (unsafe { file.as_ref() }, table) else {
        return code(Err(ZipFail::Read));
    };
    let result = dictzip::parse(&mut f, |len| {
        // SAFETY: as documented.
        let at = unsafe { table(context, len) };
        // SAFETY: as documented: NULL or `len` writable offsets.
        (!at.is_null()).then(|| unsafe { core::slice::from_raw_parts_mut(at, len) })
    });
    if let Ok(h) = result {
        // SAFETY: as documented.
        unsafe {
            out.write(CZipHeader {
                data_offset: h.data_offset,
                total_size: h.total_size,
                chunk_length: h.chunk_length,
            });
        }
    }
    code(result.map(|_| ()))
}

/// # Safety
/// `header` is valid; `table` is NULL or holds `len` offsets; `slice` is callable with `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dictzip_slices(
    header: *const CZipHeader,
    table: *const u32,
    len: usize,
    offset: u32,
    size: u32,
    slice: Option<
        unsafe extern "C" fn(
            context: *mut c_void,
            offset: u32,
            size: u32,
            skip: u32,
            take: u32,
        ) -> u8,
    >,
    context: *mut c_void,
) -> u8 {
    // SAFETY: as documented.
    let (Some(h), Some(slice)) = (unsafe { header.as_ref() }, slice) else {
        return code(Err(ZipFail::Read));
    };
    let header = dictzip::Header {
        data_offset: h.data_offset,
        total_size: h.total_size,
        chunk_length: h.chunk_length,
    };
    // SAFETY: as documented.
    let table = unsafe { crate::view(table, len) };
    code(dictzip::slices(&header, table, offset, size, |s: Slice| {
        // SAFETY: as documented.
        match unsafe { slice(context, s.offset, s.size, s.skip, s.take) } {
            0 => Ok(()),
            1 => Err(ZipFail::Read),
            3 => Err(ZipFail::Memory),
            _ => Err(ZipFail::Corrupt),
        }
    }))
}
