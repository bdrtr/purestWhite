extern crate std;

use super::*;
use std::io::Write as _;
use std::string::String;
use std::vec::Vec;
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

struct Built {
    bytes: Vec<u8>,
}

fn build(entries: &[(&str, &[u8], CompressionMethod)], comment: &str) -> Built {
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data, method) in entries {
        let options = SimpleFileOptions::default().compression_method(*method);
        writer.start_file(*name, options).expect("start_file");
        writer.write_all(data).expect("write");
    }
    writer.set_comment(String::from(comment));
    Built {
        bytes: writer.finish().expect("finish").into_inner(),
    }
}

fn sample() -> Built {
    build(
        &[
            (
                "mimetype",
                b"application/epub+zip",
                CompressionMethod::Stored,
            ),
            (
                "OEBPS/content.opf",
                &[b'x'; 300],
                CompressionMethod::Deflated,
            ),
            ("OEBPS/c1.xhtml", b"<p>bir</p>", CompressionMethod::Stored),
            ("OEBPS/c2.xhtml", b"<p>iki</p>", CompressionMethod::Stored),
        ],
        "",
    )
}

fn names(bytes: &[u8]) -> Vec<String> {
    let mut src = bytes;
    let dir = Directory::locate(&mut src).expect("locate");
    let mut out = Vec::new();
    dir.for_each(&mut src, |name, _| {
        out.push(String::from_utf8_lossy(name).into_owned());
        ControlFlow::Continue(())
    })
    .expect("for_each");
    out
}

fn data<'a>(bytes: &'a [u8], name: &str) -> &'a [u8] {
    let mut src = bytes;
    let dir = Directory::locate(&mut src).expect("locate");
    let entry = dir
        .find(&mut src, name.as_bytes(), &mut Cursor::default())
        .expect("find")
        .expect(name);
    let range = dir.data_range(&mut src, &entry).expect("data_range");
    &bytes[range.start as usize..range.end as usize]
}

#[test]
fn walks_finds_and_resolves_entries() {
    let zip = sample();
    assert_eq!(
        names(&zip.bytes),
        [
            "mimetype",
            "OEBPS/content.opf",
            "OEBPS/c1.xhtml",
            "OEBPS/c2.xhtml"
        ]
    );
    assert_eq!(data(&zip.bytes, "mimetype"), b"application/epub+zip");
    assert_eq!(data(&zip.bytes, "OEBPS/c2.xhtml"), b"<p>iki</p>");

    let mut src = zip.bytes.as_slice();
    let dir = Directory::locate(&mut src).unwrap();
    assert_eq!(dir.entries, 4);
    let opf = dir
        .find(&mut src, b"OEBPS/content.opf", &mut Cursor::default())
        .unwrap()
        .unwrap();
    assert_eq!(opf.method, METHOD_DEFLATED);
    assert_eq!(opf.uncompressed_size, 300);
    assert!(opf.compressed_size < 300);
}

#[test]
fn search_resumes_from_the_cursor_and_wraps_once() {
    let zip = sample();
    let mut src = zip.bytes.as_slice();
    let dir = Directory::locate(&mut src).unwrap();
    let mut cursor = Cursor::default();
    for name in [
        "OEBPS/c2.xhtml",
        "mimetype",
        "OEBPS/c1.xhtml",
        "OEBPS/c1.xhtml",
        "OEBPS/c2.xhtml",
    ] {
        let entry = dir.find(&mut src, name.as_bytes(), &mut cursor).unwrap();
        assert!(entry.is_some(), "{name}");
    }
    for missing in ["missing", "OEBPS/c3.xhtml", "mimetyp"] {
        assert_eq!(
            dir.find(&mut src, missing.as_bytes(), &mut cursor).unwrap(),
            None
        );
    }
    // A miss leaves the cursor usable.
    assert!(
        dir.find(&mut src, b"OEBPS/c1.xhtml", &mut cursor)
            .unwrap()
            .is_some()
    );
}

/// Counts the bytes read, to show a resumed search skips earlier entries.
struct Counting<'a> {
    bytes: &'a [u8],
    reads: Vec<u64>,
}

impl Source for Counting<'_> {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), Error> {
        self.reads.push(offset);
        self.bytes.read_exact_at(offset, buf)
    }
}

#[test]
fn consecutive_lookups_do_not_rescan_the_directory() {
    let zip = sample();
    let mut src = Counting {
        bytes: &zip.bytes,
        reads: Vec::new(),
    };
    let dir = Directory::locate(&mut src).unwrap();
    let mut cursor = Cursor::default();
    dir.find(&mut src, b"OEBPS/c1.xhtml", &mut cursor)
        .unwrap()
        .unwrap();
    src.reads.clear();
    dir.find(&mut src, b"OEBPS/c2.xhtml", &mut cursor)
        .unwrap()
        .unwrap();
    // One header and one name read: the next entry was the match.
    assert_eq!(src.reads.len(), 2);
}

#[test]
fn finds_the_end_record_behind_a_long_comment() {
    // The C++ reader only searched the last 1 KiB.
    let comment = "c".repeat(60_000);
    let zip = build(&[("a.txt", b"aaaa", CompressionMethod::Stored)], &comment);
    assert_eq!(data(&zip.bytes, "a.txt"), b"aaaa");
}

#[test]
fn skips_an_end_record_signature_inside_the_comment() {
    // A fake end record in the comment whose directory would start at
    // 0x7FFF_FFFF: the reader must keep searching and find the real one.
    let mut fake = Vec::new();
    fake.extend_from_slice(&EOCD_SIG.to_le_bytes());
    fake.extend_from_slice(&[
        0, 0, 0, 0, 9, 0, 9, 0, 0, 1, 0, 0, 0xFF, 0xFF, 0xFF, 0x7F, 0, 0,
    ]);
    let mut zip = build(&[("a.txt", b"aaaa", CompressionMethod::Stored)], "");
    let len = zip.bytes.len();
    zip.bytes[len - 2..].copy_from_slice(&u16::try_from(fake.len()).unwrap().to_le_bytes());
    zip.bytes.extend_from_slice(&fake);
    assert_eq!(data(&zip.bytes, "a.txt"), b"aaaa");
}

/// A stored-only archive where every size and offset lives in ZIP64 fields,
/// as streaming writers produce it.
fn build_zip64(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let le16 = |out: &mut Vec<u8>, v: u16| out.extend_from_slice(&v.to_le_bytes());
    let le32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    let le64 = |out: &mut Vec<u8>, v: u64| out.extend_from_slice(&v.to_le_bytes());
    let mut out = Vec::new();
    let mut offsets = Vec::new();
    for (name, data) in entries {
        offsets.push(out.len() as u64);
        le32(&mut out, LOCAL_SIG);
        for v in [45, 0, METHOD_STORED, 0, 0] {
            le16(&mut out, v);
        }
        for v in [0, u32::MAX, u32::MAX] {
            le32(&mut out, v);
        }
        le16(&mut out, u16::try_from(name.len()).unwrap());
        le16(&mut out, 20);
        out.extend_from_slice(name.as_bytes());
        le16(&mut out, ZIP64_EXTRA_ID);
        le16(&mut out, 16);
        le64(&mut out, data.len() as u64);
        le64(&mut out, data.len() as u64);
        out.extend_from_slice(data);
    }
    let directory_at = out.len() as u64;
    for ((name, data), offset) in entries.iter().zip(&offsets) {
        le32(&mut out, CENTRAL_SIG);
        for v in [45, 45, 0, METHOD_STORED, 0, 0] {
            le16(&mut out, v);
        }
        for v in [0, u32::MAX, u32::MAX] {
            le32(&mut out, v);
        }
        le16(&mut out, u16::try_from(name.len()).unwrap());
        le16(&mut out, 28);
        for v in [0, 0, 0] {
            le16(&mut out, v);
        }
        le32(&mut out, 0);
        le32(&mut out, u32::MAX);
        out.extend_from_slice(name.as_bytes());
        le16(&mut out, ZIP64_EXTRA_ID);
        le16(&mut out, 24);
        le64(&mut out, data.len() as u64);
        le64(&mut out, data.len() as u64);
        le64(&mut out, *offset);
    }
    let directory_size = out.len() as u64 - directory_at;
    let record_at = out.len() as u64;
    le32(&mut out, ZIP64_EOCD_SIG);
    le64(&mut out, ZIP64_EOCD_LEN - 12);
    for v in [45, 45] {
        le16(&mut out, v);
    }
    for v in [0, 0] {
        le32(&mut out, v);
    }
    for v in [
        entries.len() as u64,
        entries.len() as u64,
        directory_size,
        directory_at,
    ] {
        le64(&mut out, v);
    }
    le32(&mut out, ZIP64_LOCATOR_SIG);
    le32(&mut out, 0);
    le64(&mut out, record_at);
    le32(&mut out, 1);
    le32(&mut out, EOCD_SIG);
    for v in [0, 0, u16::MAX, u16::MAX] {
        le16(&mut out, v);
    }
    for v in [u32::MAX, u32::MAX] {
        le32(&mut out, v);
    }
    le16(&mut out, 0);
    out
}

#[test]
fn reads_zip64_archives() {
    let bytes = build_zip64(&[("big.bin", &[7u8; 1000]), ("small.txt", b"hello")]);
    assert_eq!(names(&bytes), ["big.bin", "small.txt"]);
    assert_eq!(data(&bytes, "big.bin"), &[7u8; 1000][..]);
    assert_eq!(data(&bytes, "small.txt"), b"hello");
    let mut src = bytes.as_slice();
    let dir = Directory::locate(&mut src).unwrap();
    assert_eq!(dir.entries, 2);
}

#[test]
fn rejects_zip64_sentinels_without_a_zip64_record() {
    let mut bytes = build_zip64(&[("a", b"1")]);
    let locator = bytes.len() - 22 - 20;
    bytes[locator] ^= 0xFF;
    assert_eq!(
        Directory::locate(&mut bytes.as_slice()),
        Err(Error::BadEndRecord)
    );
}

#[test]
fn long_names_are_searchable_but_not_listed() {
    let long = "n".repeat(MAX_NAME_LEN + 100);
    let zip = build(
        &[
            (long.as_str(), b"long", CompressionMethod::Stored),
            ("plain", b"plain!", CompressionMethod::Stored),
        ],
        "",
    );
    assert_eq!(names(&zip.bytes), ["plain"]);
    assert_eq!(data(&zip.bytes, &long), b"long");
}

#[test]
fn rejects_files_without_an_end_record() {
    let zeros = std::vec![0u8; 70_000];
    for bytes in [&[][..], &[b'P'; 8][..], &zeros[..64], &zeros[..]] {
        assert_eq!(Directory::locate(&mut { bytes }), Err(Error::NoEndRecord));
    }
}

fn central_entry_offset(bytes: &[u8], name: &str) -> usize {
    let needle: Vec<u8> = [&CENTRAL_SIG.to_le_bytes()[..]].concat();
    let dir = Directory::locate(&mut { bytes }).unwrap();
    (dir.offset as usize..bytes.len())
        .filter(|&i| bytes[i..].starts_with(&needle))
        .find(|&i| {
            let n = u16::from_le_bytes([bytes[i + 28], bytes[i + 29]]) as usize;
            &bytes[i + 46..i + 46 + n] == name.as_bytes()
        })
        .unwrap()
}

#[test]
fn rejects_stored_entries_whose_sizes_differ() {
    let mut zip = sample();
    let at = central_entry_offset(&zip.bytes, "OEBPS/c1.xhtml");
    // Claim more compressed bytes than the entry holds.
    zip.bytes[at + 20..at + 24].copy_from_slice(&99u32.to_le_bytes());
    let mut src = zip.bytes.as_slice();
    let dir = Directory::locate(&mut src).unwrap();
    let entry = dir
        .find(&mut src, b"OEBPS/c1.xhtml", &mut Cursor::default())
        .unwrap()
        .unwrap();
    assert_eq!(
        dir.data_range(&mut src, &entry),
        Err(Error::StoredSizeMismatch)
    );
}

/// A 32-bit field of 0xFFFFFFFF without a ZIP64 extra field is taken at face
/// value, as Python's zipfile does: one odd entry must not break the walk.
#[test]
fn a_sentinel_without_a_zip64_field_keeps_the_directory_readable() {
    let mut zip = sample();
    let at = central_entry_offset(&zip.bytes, "OEBPS/content.opf");
    zip.bytes[at + 24..at + 28].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(names(&zip.bytes).len(), 4);
    let mut src = zip.bytes.as_slice();
    let dir = Directory::locate(&mut src).unwrap();
    let entry = dir
        .find(&mut src, b"OEBPS/content.opf", &mut Cursor::default())
        .unwrap()
        .unwrap();
    assert_eq!(entry.uncompressed_size, u64::from(u32::MAX));
}

/// A ZIP64 field too short for the values it must supply is malformed.
#[test]
fn rejects_a_short_zip64_field() {
    let mut bytes = build_zip64(&[("a", b"1")]);
    let directory = Directory::locate(&mut bytes.as_slice()).unwrap();
    // The central extra field length sits right after the name length.
    let at = directory.offset as usize;
    let extra = at + 46 + 1;
    bytes[extra + 2..extra + 4].copy_from_slice(&8u16.to_le_bytes());
    let mut src = bytes.as_slice();
    assert_eq!(
        Directory::locate(&mut src)
            .unwrap()
            .find(&mut src, b"a", &mut Cursor::default()),
        Err(Error::BadCentralEntry)
    );
}

#[test]
fn rejects_data_that_runs_into_the_directory() {
    let mut zip = sample();
    let at = central_entry_offset(&zip.bytes, "OEBPS/content.opf");
    zip.bytes[at + 20..at + 24].copy_from_slice(&u32::MAX.wrapping_sub(1).to_le_bytes());
    let mut src = zip.bytes.as_slice();
    let dir = Directory::locate(&mut src).unwrap();
    let entry = dir
        .find(&mut src, b"OEBPS/content.opf", &mut Cursor::default())
        .unwrap()
        .unwrap();
    assert_eq!(
        dir.data_range(&mut src, &entry),
        Err(Error::DataOutOfBounds)
    );
}

#[test]
fn a_truncated_directory_is_an_error_not_garbage() {
    let zip = sample();
    let mut src = zip.bytes.as_slice();
    let dir = Directory::locate(&mut src).unwrap();
    // The directory claims to be longer than it is: the walk must stop with
    // an error once it runs past the real entries.
    let longer = Directory {
        size: dir.size + 30,
        entries: dir.entries + 1,
        ..dir
    };
    let mut seen = 0;
    let result = longer.for_each(&mut src, |_, _| {
        seen += 1;
        ControlFlow::Continue(())
    });
    assert_eq!(seen, 4);
    assert_eq!(result, Err(Error::BadCentralEntry));
}

/// Every operation on every truncation and single-byte corruption of a valid
/// archive must return, not panic, and any data range must lie in the file.
#[test]
fn survives_truncation_and_corruption() {
    fn exercise(bytes: &[u8]) {
        let mut src = bytes;
        let Ok(dir) = Directory::locate(&mut src) else {
            return;
        };
        let mut entries = Vec::new();
        let _ = dir.for_each(&mut src, |name, entry| {
            entries.push((name.to_vec(), *entry));
            ControlFlow::Continue(())
        });
        let mut cursor = Cursor::default();
        for (name, entry) in &entries {
            let _ = dir.find(&mut src, name, &mut cursor);
            if let Ok(range) = dir.data_range(&mut src, entry) {
                assert!(range.start <= range.end && range.end <= bytes.len() as u64);
            }
        }
        let _ = dir.find(&mut src, b"missing", &mut cursor);
    }

    for archive in [sample().bytes, build_zip64(&[("a", b"1"), ("bb", b"22")])] {
        for len in 0..archive.len() {
            exercise(&archive[..len]);
        }
        let mut bytes = archive.clone();
        for i in 0..bytes.len() {
            let original = bytes[i];
            for value in [0x00, 0xFF, original ^ 0x01, original ^ 0x80] {
                bytes[i] = value;
                exercise(&bytes);
            }
            bytes[i] = original;
        }
    }
}
