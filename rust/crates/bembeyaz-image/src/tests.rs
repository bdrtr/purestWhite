#![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

use super::Probe;

/// The size `bytes` gives fed in pieces of `piece`, and how many bytes the last piece took.
fn probe(bytes: &[u8], piece: usize) -> (Option<(u32, u32)>, usize) {
    let mut p = Probe::default();
    let mut taken = 0;
    for chunk in bytes.chunks(piece) {
        taken = p.feed(chunk);
        if taken < chunk.len() {
            break;
        }
    }
    (p.size(), taken)
}

const PNG: [u8; 24] = [
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 2,
    0x58, 0, 0, 3, 0x20,
];

#[test]
fn png_sizes_come_from_ihdr() {
    for piece in [1, 5, 24] {
        assert_eq!(probe(&PNG, piece).0, Some((600, 800)));
    }
    // The last byte settles it: 23 bytes taken in one piece.
    assert_eq!(probe(&PNG, 24).1, 23);
    let mut bad = PNG;
    bad[13] = b'X';
    assert_eq!(probe(&bad, 24), (None, 13));
}

#[test]
fn jpeg_sizes_come_from_the_frame_header() {
    // SOI, an APP0 segment of 6 bytes, fill bytes, an RST, a DHT (C4, not a frame), then SOF2.
    let jpeg = [
        0xFF, 0xD8, 0xFF, 0xE0, 0, 6, 1, 2, 3, 4, 0xFF, 0xFF, 0xD0, 0xFF, 0xC4, 0, 2, 0xFF, 0xC2,
        0, 11, 8, 0x01, 0x2C, 0x00, 0xC8,
    ];
    for piece in [1, 3, jpeg.len()] {
        assert_eq!(probe(&jpeg, piece).0, Some((200, 300)));
    }
    // SOS before a frame header, a short frame header, or not a JPEG.
    assert_eq!(probe(&[0xFF, 0xD8, 0xFF, 0xDA], 4), (None, 3));
    assert_eq!(probe(&[0xFF, 0xD8, 0xFF, 0xC0, 0, 6], 6), (None, 5));
    assert_eq!(probe(b"GIF89a", 6), (None, 0));
    // 0xFF not followed by SOI.
    assert_eq!(probe(&[0xFF, 0xD9], 2), (None, 1));
}

#[test]
fn settled_probes_take_nothing_more() {
    let mut p = Probe::default();
    assert_eq!(p.feed(&PNG), 23);
    assert_eq!(p.feed(&PNG), 0);
    assert_eq!(p.size(), Some((600, 800)));
}

/// Truncated and damaged headers, fed whole and byte by byte: nothing panics, every feed takes at
/// most what it was given, and a probe that found a size keeps it.
#[test]
fn mini_fuzz() {
    let jpeg = [
        0xFF, 0xD8, 0xFF, 0xE1, 0, 8, 1, 2, 3, 4, 5, 6, 0xFF, 0xC0, 0, 17, 8, 0, 64, 0, 48,
    ];
    let mut seed = 0x2545_F491u32;
    for base in [&PNG[..], &jpeg[..]] {
        for len in 0..=base.len() {
            for _ in 0..64 {
                let mut bytes = base[..len].to_vec();
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                if let Some(b) = bytes.get_mut(seed as usize % len.max(1)) {
                    *b ^= (seed >> 8) as u8;
                }
                let mut whole = Probe::default();
                assert!(whole.feed(&bytes) <= bytes.len());
                let mut single = Probe::default();
                for b in &bytes {
                    assert!(single.feed(core::slice::from_ref(b)) <= 1);
                }
                assert_eq!(whole.size(), single.size());
                if let Some(size) = whole.size() {
                    whole.feed(&bytes);
                    assert_eq!(whole.size(), Some(size));
                }
            }
        }
    }
}
