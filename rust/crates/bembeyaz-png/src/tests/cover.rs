//! The cover reader. `test/png_to_bmp` pins it against the converter's own decoder; these tests
//! name what it does.

use super::*;
use crate::cover::{Cover, Error};
use std::boxed::Box;

fn new_cover() -> Box<MaybeUninit<Cover>> {
    Box::new_uninit()
}

struct Read {
    width: usize,
    rows: Vec<Vec<u8>>,
    result: Result<(), Error>,
}

fn read(bytes: &[u8]) -> Result<Read, Error> {
    let mut slot = new_cover();
    let cover = Cover::init(&mut slot);
    let size = bytes.len() as u32;
    cover.open(&mut Mem(bytes), size)?;
    let mut rows = Vec::new();
    let result = cover.decode(&mut Mem(bytes), |gray| {
        rows.push(gray.to_vec());
        true
    });
    Ok(Read {
        width: cover.width() as usize,
        rows,
        result,
    })
}

/// A `width`-pixel, `rows.len()`-row image of raw `rows`, unfiltered, in one IDAT.
fn image(width: u32, depth: u8, color: u8, before: &[Vec<u8>], rows: &[Vec<u8>]) -> Vec<u8> {
    let data = filter_rows(rows, filter_step(depth, color), &[0]);
    let header = ihdr(width, rows.len() as u32, depth, color, 0);
    file(&header, before, &zlib_stored(&data, 65535), usize::MAX)
}

#[test]
fn reads_every_color_type_as_gray() {
    // 25% red, 50% green, 25% blue; the high byte of 16-bit samples; alpha ignored.
    let gray = |w, d, c, row: Vec<u8>| read(&image(w, d, c, &[], &[row])).unwrap().rows;
    assert_eq!(gray(2, 8, 2, vec![100, 200, 40, 0, 0, 255]), [[135, 63]]);
    assert_eq!(gray(1, 16, 2, vec![100, 9, 200, 9, 40, 9]), [[135]]);
    assert_eq!(gray(1, 8, 6, vec![100, 200, 40, 7]), [[135]]);
    assert_eq!(gray(1, 16, 6, vec![100, 1, 200, 2, 40, 3, 4, 5]), [[135]]);
    assert_eq!(gray(2, 8, 4, vec![77, 0, 88, 255]), [[77, 88]]);
    assert_eq!(gray(1, 16, 4, vec![77, 1, 2, 3]), [[77]]);
    assert_eq!(gray(2, 16, 0, vec![200, 1, 30, 2]), [[200, 30]]);
    assert_eq!(gray(3, 8, 0, vec![1, 2, 3]), [[1, 2, 3]]);
    // Packed samples scale to 0..255.
    assert_eq!(gray(4, 2, 0, vec![0b0001_1011]), [[0, 85, 170, 255]]);
    assert_eq!(gray(3, 4, 0, vec![0xF1, 0x80]), [[255, 17, 136]]);
    assert_eq!(
        gray(9, 1, 0, vec![0b1010_0000, 0b1000_0000]),
        [[255, 0, 255, 0, 0, 0, 0, 0, 255]]
    );
}

#[test]
fn reads_indices_past_the_palette_as_entry_0() {
    let plte = chunk(*b"PLTE", &[100, 200, 40, 0, 0, 255]);
    let rows = read(&image(4, 2, 3, &[plte], &[vec![0b0001_1011]]))
        .unwrap()
        .rows;
    assert_eq!(rows, [[135, 63, 135, 135]]);
    // A second PLTE replaces the first, its size too; a long one keeps 256 entries.
    let first = chunk(*b"PLTE", &[0, 0, 0, 50, 50, 50]);
    let second = chunk(*b"PLTE", &[100, 200, 40]);
    let rows = read(&image(2, 8, 3, &[first, second], &[vec![0, 1]]))
        .unwrap()
        .rows;
    assert_eq!(rows, [[135, 135]]);
    let long = chunk(*b"PLTE", &[255; 900]);
    let rows = read(&image(2, 8, 3, &[long], &[vec![255, 0]]))
        .unwrap()
        .rows;
    assert_eq!(rows, [[255, 255]]);
}

#[test]
fn undoes_every_filter() {
    let mut rng = Rng(5);
    for (depth, color, width) in [
        (8, 2, 9),
        (16, 2, 5),
        (16, 6, 4),
        (8, 4, 7),
        (2, 0, 13),
        (16, 0, 6),
        (8, 6, 5),
    ] {
        let pitch = (width * filter_step(depth, color) * 8 / 8).max(1);
        let pitch = if depth < 8 {
            (width * usize::from(depth)).div_ceil(8)
        } else {
            pitch
        };
        let rows = random_rows(&mut rng, 6, pitch);
        let plain = read(&image(width as u32, depth, color, &[], &rows))
            .unwrap()
            .rows;
        for filters in [[1u8], [2], [3], [4]] {
            let data = filter_rows(&rows, filter_step(depth, color), &filters);
            let header = ihdr(width as u32, 6, depth, color, 0);
            let bytes = file(&header, &[], &zlib_stored(&data, 65535), 37);
            let r = read(&bytes).unwrap();
            assert_eq!(r.result, Ok(()));
            assert_eq!(
                r.rows, plain,
                "depth {depth} color {color} filter {filters:?}"
            );
        }
    }
}

#[test]
fn stops_at_an_unknown_filter() {
    let rows = [vec![1, 2], vec![3, 4], vec![5, 6]];
    let mut data = filter_rows(&rows, 1, &[0]);
    data[3] = 5;
    let bytes = file(
        &ihdr(2, 3, 8, 0, 0),
        &[],
        &zlib_stored(&data, 65535),
        usize::MAX,
    );
    let r = read(&bytes).unwrap();
    assert_eq!(r.rows, [[1, 2]]);
    assert_eq!(r.result, Err(Error::UnknownFilter));
}

#[test]
fn checks_the_adler_sum() {
    let rows = [vec![1, 2], vec![3, 4]];
    let mut stream = zlib_stored(&filter_rows(&rows, 1, &[0]), 65535);
    let last = stream.len() - 1;
    stream[last] ^= 1;
    let bytes = file(&ihdr(2, 2, 8, 0, 0), &[], &stream, usize::MAX);
    let r = read(&bytes).unwrap();
    assert_eq!(r.rows, [[1, 2]]);
    assert_eq!(r.result, Err(Error::Corrupt));
}

#[test]
fn reads_data_split_around_other_chunks() {
    let rows = [vec![1, 2, 3], vec![4, 5, 6]];
    let stream = zlib_stored(&filter_rows(&rows, 1, &[0]), 65535);
    let mut bytes = SIGNATURE.to_vec();
    bytes.extend(ihdr(3, 2, 8, 0, 0));
    bytes.extend(chunk(*b"IDAT", &stream[..5]));
    bytes.extend(chunk(*b"tEXt", b"k\0v"));
    bytes.extend(chunk(*b"IDAT", &stream[5..]));
    bytes.extend(chunk(*b"IEND", b""));
    let r = read(&bytes).unwrap();
    assert_eq!(r.result, Ok(()));
    assert_eq!(r.rows, rows);
}

#[test]
fn refuses_what_the_converter_refused() {
    let ok = image(2, 8, 0, &[], &[vec![1, 2]]);
    let opened = |bytes: &[u8]| read(bytes).map(|_| ());
    assert_eq!(opened(&ok), Ok(()));
    assert_eq!(opened(&ok[..7]), Err(Error::NotPng));
    assert_eq!(opened(&ok[..10]), Err(Error::HeaderEnded));
    assert_eq!(opened(&ok[..20]), Err(Error::HeaderEnded));
    let mut not_first = ok.clone();
    not_first[12..16].copy_from_slice(b"IHDX");
    assert_eq!(opened(&not_first), Err(Error::NoHeader));
    for at in [26, 27, 28] {
        // Compression, filter method, interlace.
        let mut bad = ok.clone();
        bad[at] = 1;
        assert_eq!(opened(&bad), Err(Error::Unsupported), "{at}");
    }
    for (depth, color) in [(16, 3), (4, 2), (0, 0), (8, 5)] {
        let bad = image(2, depth, color, &[], &[vec![1, 2]]);
        assert_eq!(opened(&bad), Err(Error::Unsupported), "{depth} {color}");
    }
    let wide = file(&ihdr(2049, 1, 8, 0, 0), &[], &[], 1);
    assert_eq!(opened(&wide), Err(Error::BadSize));
    let empty = file(&ihdr(0, 1, 8, 0, 0), &[], &[], 1);
    assert_eq!(opened(&empty), Err(Error::BadSize));
    let no_data = file(&ihdr(2, 1, 8, 0, 0), &[], &[], 1);
    assert_eq!(opened(&no_data), Err(Error::NoImageData));
}

#[test]
fn decodes_once() {
    let bytes = image(2, 8, 0, &[], &[vec![1, 2]]);
    let mut slot = new_cover();
    let cover = Cover::init(&mut slot);
    assert_eq!(
        cover.decode(&mut Mem(&bytes), |_| true),
        Err(Error::NotOpen)
    );
    cover.open(&mut Mem(&bytes), bytes.len() as u32).unwrap();
    assert_eq!(
        cover.decode(&mut Mem(&bytes), |_| false),
        Err(Error::QuitEarly)
    );
    assert_eq!(
        cover.decode(&mut Mem(&bytes), |_| true),
        Err(Error::NotOpen)
    );
}

/// Damage or a cut anywhere ends in a result, never a panic, with rows of the image's width.
#[test]
fn survives_random_damage() {
    let mut rng = Rng(11);
    for (depth, color, width) in [
        (8u8, 2u8, 7usize),
        (16, 6, 3),
        (1, 3, 17),
        (4, 0, 5),
        (16, 0, 4),
    ] {
        let pitch = if depth < 8 {
            (width * usize::from(depth)).div_ceil(8)
        } else {
            width * filter_step(depth, color)
        };
        let rows = random_rows(&mut rng, 9, pitch);
        let data = filter_rows(&rows, filter_step(depth, color), &[0, 1, 2, 3, 4]);
        let plte = chunk(*b"PLTE", &[9; 30]);
        let good = file(
            &ihdr(width as u32, 9, depth, color, 0),
            &[plte],
            &zlib_stored(&data, 40),
            23,
        );
        for _ in 0..600 {
            let mut bytes = good.clone();
            for _ in 0..=rng.below(3) {
                if bytes.is_empty() {
                    break;
                }
                let at = rng.below(bytes.len());
                match rng.below(3) {
                    0 => bytes[at] ^= 1 << rng.below(8),
                    1 => bytes[at] = rng.byte(),
                    _ => bytes.truncate(at),
                }
            }
            if let Ok(r) = read(&bytes) {
                assert!(r.rows.len() <= 9);
                assert!(r.rows.iter().all(|row| row.len() == r.width));
            }
        }
    }
}
