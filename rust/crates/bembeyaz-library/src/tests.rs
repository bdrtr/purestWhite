#![allow(clippy::unwrap_used)]

use super::{
    author_key, clean_person_name, fold, folded_group_initial, matches_query, surname_key,
};
use std::string::String;
use std::vec::Vec;

fn s(f: fn(&[u8], &mut Vec<u8>) -> Result<(), super::OutOfMemory>, text: &str) -> String {
    let mut out = Vec::new();
    f(text.as_bytes(), &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn folds_both_forms_alike() {
    assert_eq!(s(fold, "L'\u{c9}n\u{e9}ide"), "l'eneide");
    assert_eq!(s(fold, "Le\u{301}nei\u{308}de"), "leneide");
    assert_eq!(
        s(fold, "  S\u{f8}ren -- Kierkegaard!! "),
        "soren kierkegaard"
    );
    assert_eq!(
        s(fold, "\u{3a3}\u{3bf}\u{3c6}\u{3af}\u{3b1} 1984"),
        "\u{3a3}\u{3bf}\u{3c6}\u{3b9}\u{3b1} 1984"
    );
    let mut cut = Vec::new();
    fold(b"abc\xe2\x82", &mut cut).unwrap();
    assert_eq!(cut, b"abc");
    assert_eq!(folded_group_initial("\u{e9}mile".as_bytes()), 0xE9);
    assert_eq!(folded_group_initial(b"1984"), 0);
}

#[test]
fn people() {
    assert_eq!(s(clean_person_name, "Austen, Jane"), "Jane Austen");
    assert_eq!(
        s(clean_person_name, "George Sand [Sand, George]"),
        "George Sand"
    );
    assert_eq!(s(clean_person_name, "Herbert G_ Wells_"), "Herbert G Wells");
    assert_eq!(s(clean_person_name, "H_G_ Wells"), "H.G Wells");
    assert_eq!(s(clean_person_name, "Smith, John, Jr."), "Smith, John, Jr");
    assert_eq!(s(author_key, "Lu, Xun"), s(author_key, "Xun Lu"));
    assert_eq!(s(author_key, "Wollstonecraft, Mary"), "mary wollsto");
    assert_eq!(s(surname_key, "Herman Melville"), "melville herman");
    assert!(matches_query(b"wuthering heights emily bronte", b"wut emi"));
    assert!(matches_query(b"l'eneide", b"eneide"));
    assert!(!matches_query(b"wuthering", b"heights"));
    assert!(matches_query(b"x", b""));
}

#[test]
fn any_bytes_never_panic() {
    let mut seed = 0x1234_5678_u32;
    for _ in 0..3000 {
        let mut text = Vec::new();
        for _ in 0..(seed % 40) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            text.push(seed.to_le_bytes()[0]);
        }
        let mut out = Vec::new();
        let _ = fold(&text, &mut out);
        let _ = clean_person_name(&text, &mut out);
        let _ = author_key(&text, &mut out);
        let _ = surname_key(&text, &mut out);
        let _ = folded_group_initial(&text);
        let _ = matches_query(&text, &out);
    }
}

#[test]
fn cut_and_corrupted_samples_never_panic() {
    let samples: [&[u8]; 4] = [
        "L'\u{c9}n\u{e9}ide".as_bytes(),
        "Wollstonecraft, Mary [W., M.]; Shelley".as_bytes(),
        "S\u{f8}ren Kierkegaard \u{4e2d}\u{6587} 1984".as_bytes(),
        "Herbert G_ Wells_ (ed.)".as_bytes(),
    ];
    let mut seed = 0x00DD_BA11_u32;
    for sample in samples {
        for end in 0..=sample.len() {
            for flip in 0..sample.len() {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let mut text = sample.get(..end).unwrap().to_vec();
                if let Some(byte) = text.get_mut(flip) {
                    *byte = seed.to_le_bytes()[0];
                }
                let mut out = Vec::new();
                let _ = fold(&text, &mut out);
                let _ = author_key(&text, &mut out);
                let _ = clean_person_name(&text, &mut out);
                let _ = surname_key(&text, &mut out);
                let _ = folded_group_initial(&text);
                let _ = matches_query(&out, &text);
            }
        }
    }
}

mod format {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::cast_possible_truncation
    )]

    use crate::format::{
        Header, Identity, Order, Record, Source, Validity, layout_sections, ordinal_for_row,
        path_hash, read_blob_field, read_name, read_path, read_record, recent_rows_for,
        record_offset, validate,
    };
    use std::vec::Vec;

    struct File {
        bytes: Vec<u8>,
        failed: bool,
    }

    impl Source for File {
        fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool {
            let at = offset as usize;
            if let Some(src) = self.bytes.get(at..at + buf.len()) {
                buf.copy_from_slice(src);
                true
            } else {
                self.failed = true;
                false
            }
        }
        fn fail(&mut self) {
            self.failed = true;
        }
    }

    /// One book "a.epub" in folder "/dir" with author "Me".
    fn index() -> (Header, File) {
        let mut h = Header {
            magic: *b"CLX1",
            format_version: 2,
            fold_version: 4,
            book_count: 1,
            folder_count: 1,
            ..Header::default()
        };
        let folders = [&[4u8][..], b"/dir"].concat();
        let mut names = 7u64.to_le_bytes().to_vec();
        names.extend_from_slice(b"a.epub");
        names.extend_from_slice(&[2, b'M', b'e']);
        layout_sections(&mut h, folders.len() as u32, names.len() as u32);
        let mut bytes = std::vec![0u8; h.self_size as usize];
        let header: [u8; 64] = unsafe { core::mem::transmute(h) };
        bytes[..64].copy_from_slice(&header);
        bytes[512..512 + folders.len()].copy_from_slice(&folders);
        let rec = record_offset(&h, 0) as usize;
        bytes[rec + 4..rec + 8].copy_from_slice(&1234u32.to_le_bytes());
        bytes[rec + 12] = 6; // name length
        bytes[rec + 13] = 200; // fold length, clamped to 96
        let names_at = h.name_start as usize;
        bytes[names_at..names_at + names.len()].copy_from_slice(&names);
        (
            h,
            File {
                bytes,
                failed: false,
            },
        )
    }

    #[test]
    fn reads_a_small_index() {
        let (h, mut f) = index();
        assert_eq!(validate(&h, f.bytes.len() as u64), Validity::Ok);
        assert_eq!(
            validate(&h, f.bytes.len() as u64 + 1),
            Validity::SizeMismatch
        );
        assert_eq!(ordinal_for_row(&h, Order::TitleAsc, 0, &mut f), 0);
        assert_eq!(ordinal_for_row(&h, Order::TitleAsc, 1, &mut f), 0xFFFF);
        let mut r = unsafe { core::mem::zeroed::<Record>() };
        assert!(read_record(&h, 0, &mut f, &mut r));
        assert_eq!(r.fold_len, 96);
        let mut out = Vec::new();
        assert!(read_name(&h, &r, &mut f, &mut out).unwrap());
        assert_eq!(out, b"a.epub");
        assert!(read_blob_field(&h, &r, 0, &mut f, &mut out).unwrap());
        assert_eq!(out, b"Me");
        assert!(!read_blob_field(&h, &r, 1, &mut f, &mut out).unwrap());
        assert!(read_path(&h, &r, &mut f, &mut out).unwrap());
        assert_eq!(out, b"/dir/a.epub");
        let mut rows = [0u16; 2];
        let books = [
            Identity {
                path_hash: 7,
                file_size: 1234,
            },
            Identity {
                path_hash: 8,
                file_size: 0,
            },
        ];
        assert!(recent_rows_for(&h, &books, &mut rows, &mut f).unwrap());
        assert_eq!(rows, [0, 0xFFFF]);
        assert_eq!(path_hash(b""), 14_695_981_039_346_656_037);
        assert!(!f.failed);
    }

    #[test]
    fn damaged_indexes_never_panic() {
        let (_, base) = index();
        let mut seed = 0x0BAD_1DEA_u32;
        for i in 0..base.bytes.len() {
            if i % 3 != 0 && i > 1200 {
                continue;
            }
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let mut bytes = base.bytes.clone();
            bytes[i] = seed.to_le_bytes()[0];
            let h: Header = unsafe {
                core::mem::transmute::<[u8; 64], Header>(bytes[..64].try_into().unwrap())
            };
            let mut f = File {
                bytes,
                failed: false,
            };
            let _ = validate(&h, f.bytes.len() as u64);
            for row in 0..3 {
                let _ = ordinal_for_row(&h, Order::AuthorDesc, row, &mut f);
            }
            let mut r = unsafe { core::mem::zeroed::<Record>() };
            let _ = read_record(&h, 0, &mut f, &mut r);
            let mut out = Vec::new();
            let _ = read_name(&h, &r, &mut f, &mut out);
            let _ = read_blob_field(&h, &r, 2, &mut f, &mut out);
            let _ = read_path(&h, &r, &mut f, &mut out);
            let mut rows = [0u16; 1];
            let _ = recent_rows_for(
                &h,
                &[Identity {
                    path_hash: 7,
                    file_size: 0,
                }],
                &mut rows,
                &mut f,
            );
        }
    }
}
