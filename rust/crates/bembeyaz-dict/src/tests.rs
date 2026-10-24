#![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

extern crate std;

use super::*;
use std::string::String;
use std::vec::Vec;

struct Mem(Vec<u8>);

impl Source for Mem {
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> Result<usize, Fail> {
        let rest = self.0.get(pos as usize..).ok_or(Fail::Seek)?;
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        Ok(n)
    }
}

/// An `.idx` of `words` (sorted as the reader compares them), definition i at offset i * 10, size
/// i + 1; and its sidecar.
fn index(words: &[&str]) -> (Mem, Mem, Header) {
    let mut idx = Vec::new();
    for (i, w) in words.iter().enumerate() {
        idx.extend_from_slice(w.as_bytes());
        idx.push(0);
        idx.extend_from_slice(&(i as u32 * 10).to_be_bytes());
        idx.extend_from_slice(&(i as u32 + 1).to_be_bytes());
    }
    let mut samples = Vec::new();
    samples.extend_from_slice(&0u32.to_le_bytes());
    let mut scan = Scan::new(8, idx.len() as u32);
    for chunk in idx.chunks(7) {
        assert!(scan.feed(chunk, |at| {
            samples.extend_from_slice(&at.to_le_bytes());
            true
        }));
    }
    let counts = Header {
        sample_count: scan.samples,
        source_size: idx.len() as u32,
        entry_count: scan.entries,
    };
    let mut sidecar = header_bytes(QIDX_MAGIC, &counts).to_vec();
    sidecar.extend_from_slice(&samples);
    (Mem(idx), Mem(sidecar), counts)
}

#[test]
fn ifo_facts() {
    assert_eq!(
        ifo(b"version=2.4.2\nidxoffsetbits=64\nsametypesequence=h\n"),
        Ifo {
            offsets64: true,
            html: true
        }
    );
    assert_eq!(
        ifo(b"idxoffsetbits= +64x\nsametypesequence=x"),
        Ifo {
            offsets64: true,
            html: true
        }
    );
    assert_eq!(
        ifo(b"idxoffsetbits=32\nsametypesequence=hm\n"),
        Ifo::default()
    );
    assert_eq!(ifo(b"idxoffsetbits=128"), Ifo::default());
    // Read as a C string: nothing after a NUL counts.
    assert_eq!(ifo(b"bookname=x\0idxoffsetbits=64"), Ifo::default());
}

#[test]
fn sidecars_sample_every_256th_entry() {
    let words: Vec<String> = (0..1000).map(|i| std::format!("w{i:04}")).collect();
    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let (_, sidecar, counts) = index(&refs);
    assert_eq!(counts.entry_count, 1000);
    // Entries 0, 256, 512 and 768.
    assert_eq!(counts.sample_count, 4);
    let raw: [u8; HEADER_LEN] = sidecar.0[..HEADER_LEN]
        .try_into()
        .unwrap_or([0; HEADER_LEN]);
    assert_eq!(header(&raw, QIDX_MAGIC), Some(counts));
    assert_eq!(header(&raw, SIDX_MAGIC), None);
    // Entry 256 starts 256 entries of 6 + 8 bytes in.
    assert_eq!(
        &sidecar.0[HEADER_LEN + 4..HEADER_LEN + 8],
        &(256u32 * 14).to_le_bytes()
    );
}

#[test]
fn lookups_bisect_then_scan() {
    let words: Vec<String> = (0..1000).map(|i| std::format!("w{i:04}")).collect();
    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let (mut idx, mut sidecar, counts) = index(&refs);
    let mut buf = [0u8; WORD_MAX + 1];
    let mut word = Word::new(&mut buf);
    for sampled in [0, counts.sample_count] {
        let mut index = Index {
            source: &mut idx,
            sidecar: &mut sidecar,
            source_size: counts.source_size,
            sample_count: sampled,
            entry_count: counts.entry_count,
        };
        let hit = index.locate(b"W0700", &mut word);
        assert!(hit.found && hit.offset == 7000 && hit.size == 701);
        assert_eq!(word.as_bytes(), b"w0700");
        assert!(!index.locate(b"w07000", &mut word).found);
        let by_ordinal = index.locate_ordinal(999, &mut word);
        assert!(by_ordinal.found && by_ordinal.offset == 9990);
        assert!(!index.locate_ordinal(1000, &mut word).found);
    }
    // An entry count below the entries there (a damaged header) still bounds ordinals.
    let mut index = Index {
        source: &mut idx,
        sidecar: &mut sidecar,
        source_size: counts.source_size,
        sample_count: counts.sample_count,
        entry_count: 500,
    };
    assert!(index.locate_ordinal(499, &mut word).found);
    assert!(!index.locate_ordinal(500, &mut word).found);
}

#[test]
fn words_are_cleaned_and_stemmed() {
    let mut out = [0u8; 32];
    let n = clean_word("“Garage”.".as_bytes(), &mut out);
    assert_eq!(&out[..n], b"garage");
    // U+2042 (E2 81 82) is General Punctuation too.
    let n = clean_word("\u{2042}Word\u{2042}".as_bytes(), &mut out);
    assert_eq!(&out[..n], b"word");
    let n = clean_word(b"--", &mut out);
    assert_eq!(n, 0);
    let stems = |w: &str| {
        let mut v = Vec::new();
        stem_variants(w.as_bytes(), |s| {
            let mut t = w.as_bytes()[..s.stem].to_vec();
            t.extend_from_slice(s.ending);
            v.push(String::from_utf8(t).unwrap_or_default());
        });
        v
    };
    assert_eq!(stems("stories"), ["story", "stori", "storie"]);
    assert_eq!(stems("stopped"), ["stopp", "stoppe", "stop"]);
    assert_eq!(stems("running"), ["runn", "runne", "run"]);
    assert_eq!(stems("cat’s"), ["cat", "cat’"]);
    assert!(stems("s").is_empty());
}

/// Truncated and damaged indexes and sidecars: nothing panics.
#[test]
fn mini_fuzz() {
    let words: Vec<String> = (0..600).map(|i| std::format!("k{i:03}")).collect();
    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let (idx, sidecar, counts) = index(&refs);
    let mut seed = 0x9E37_79B9u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut buf = [0u8; WORD_MAX + 1];
    let mut word = Word::new(&mut buf);
    for _ in 0..500 {
        let mut a = Mem(idx.0[..next() as usize % (idx.0.len() + 1)].to_vec());
        let mut b = Mem(sidecar.0.clone());
        for _ in 0..4 {
            if !b.0.is_empty() {
                let at = next() as usize % b.0.len();
                b.0[at] = next() as u8;
            }
            if !a.0.is_empty() {
                let at = next() as usize % a.0.len();
                a.0[at] = next() as u8;
            }
        }
        let mut index = Index {
            source: &mut a,
            sidecar: &mut b,
            source_size: counts.source_size,
            sample_count: next() % 4,
            entry_count: next() % 700,
        };
        let _ = index.locate(words[next() as usize % 600].as_bytes(), &mut word);
        let _ = index.locate_ordinal(next() % 800, &mut word);
    }
}

/// A source that fails from a position on.
struct Failing(Vec<u8>, u32);

impl Source for Failing {
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> Result<usize, Fail> {
        if pos >= self.1 {
            return Err(Fail::Read);
        }
        let end = (self.1 as usize).min(self.0.len());
        let rest = self.0.get(pos as usize..end).ok_or(Fail::Seek)?;
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        Ok(n)
    }
}

#[test]
fn cut_words_end_at_a_failed_read_or_the_end() {
    // A 300-byte word whose rest cannot be read: kept as cut, as the old reader did; a short word
    // cut off by a failed read is no word.
    let mut long = Failing(std::vec![b'a'; 300], 280);
    let mut buf = [0u8; WORD_MAX + 1];
    let mut word = Word::new(&mut buf);
    assert_eq!(word.read(&mut long, 0), Some(280));
    assert_eq!(word.as_bytes().len(), WORD_MAX);
    let mut short = Failing(std::vec![b'a'; 300], 100);
    assert_eq!(word.read(&mut short, 0), None);
    // The same at the end of the file.
    assert_eq!(word.read(&mut Mem(std::vec![b'a'; 300]), 0), Some(300));
    assert_eq!(word.read(&mut Mem(std::vec![b'a'; 100]), 0), None);
}

/// Stem variants never repeat, for any word made of the endings' letters.
#[test]
fn stem_variants_never_repeat() {
    let pieces: [&[u8]; 9] = [
        b"s",
        b"e",
        b"i",
        b"y",
        b"d",
        b"n",
        b"g",
        b"'",
        "\u{2019}".as_bytes(),
    ];
    let mut seed = 0x1234_5679u32;
    for _ in 0..200_000 {
        let mut word = Vec::new();
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let mut bits = seed;
        for _ in 0..=(bits % 7) {
            bits /= 7;
            word.extend_from_slice(pieces[(bits % 9) as usize]);
            bits /= 9;
        }
        let mut variants: Vec<Vec<u8>> = Vec::new();
        stem_variants(&word, |v| {
            let mut t = word[..v.stem].to_vec();
            t.extend_from_slice(v.ending);
            assert!(!variants.contains(&t), "{word:?}");
            variants.push(t);
        });
    }
}
