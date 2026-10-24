#![allow(clippy::indexing_slicing)]

extern crate std;

use super::{Ditherer, Method};
use std::vec;

fn run(method: Method, width: usize, rows: &[&[u8]]) -> std::vec::Vec<std::vec::Vec<u8>> {
    let mut errors = vec![0i16; method.error_len(width)];
    let mut done = 0;
    let mut d = Ditherer {
        method,
        original_thresholds: false,
        errors: &mut errors,
        rows_done: &mut done,
    };
    let bytes = (width * method.bits()).div_ceil(8);
    rows.iter()
        .enumerate()
        .map(|(y, row)| {
            let mut out = vec![0u8; bytes];
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            d.pixels(row, 0, y as i32, &mut out);
            d.next_row();
            out
        })
        .collect()
}

#[test]
fn thresholds_and_native_levels() {
    // 0..45, 45..70, 70..140, 140.. by the panel's thresholds; the top two bits for native.
    assert_eq!(
        run(Method::Threshold, 4, &[&[44, 45, 139, 140]]),
        [[0b00_01_10_11]]
    );
    assert_eq!(
        run(Method::Native, 4, &[&[63, 64, 191, 255]]),
        [[0b00_01_10_11]]
    );
}

#[test]
fn atkinson_carries_error_to_neighbours() {
    // Mid gray: the first pixel rounds down, and the carried error pushes later ones up.
    let rows = run(Method::Atkinson, 4, &[&[100; 4], &[100; 4]]);
    assert_eq!(rows, [[0b10_10_10_10], [0b10_10_10_10]]);
    let one_bit = run(Method::Atkinson1, 8, &[&[128; 8], &[127; 8]]);
    // The first pixel is white; the error it leaves darkens the next ones.
    assert_eq!(one_bit[0][0] & 0xC0, 0x80);
}

#[test]
fn floyd_steinberg_mirrors_odd_rows() {
    let rows = run(Method::FloydSteinberg, 4, &[&[60; 4], &[60; 4], &[60; 4]]);
    assert_eq!(rows.len(), 3);
    // Without error rows, it only quantizes.
    let mut done = 0;
    let mut none = Ditherer {
        method: Method::FloydSteinberg,
        original_thresholds: true,
        errors: &mut [],
        rows_done: &mut done,
    };
    let mut out = [0u8; 1];
    none.pixels(&[42, 43, 212, 213], 0, 0, &mut out);
    assert_eq!(out, [0b00_01_10_11]);
    // 1-bit Atkinson without rows: a plain threshold at 128.
    let mut done = 0;
    let mut plain = Ditherer {
        method: Method::Atkinson1,
        original_thresholds: false,
        errors: &mut [],
        rows_done: &mut done,
    };
    plain.pixels(&[127, 128, 0, 255, 127, 128, 0, 255], 0, 0, &mut out);
    assert_eq!(out, [0b0101_0101]);
}

#[test]
fn pixels_past_the_output_and_errors_are_dropped() {
    let mut errors = [0i16; 3];
    let mut done = 0;
    let mut d = Ditherer {
        method: Method::Atkinson,
        original_thresholds: false,
        errors: &mut errors,
        rows_done: &mut done,
    };
    let mut out = [0u8; 1];
    d.pixels(&[200; 40], 0, 0, &mut out);
    d.next_row();
    d.pixels(&[200; 3], usize::MAX - 1, i32::MIN, &mut out);
    d.reset();
    assert_eq!(*d.rows_done, 0);
}

#[test]
fn next_row_clears_the_finished_row() {
    // floyd_steinberg leaves what odd rows send left in the current row: next_row must clear it.
    for method in [Method::Atkinson, Method::FloydSteinberg, Method::Atkinson1] {
        let mut errors = vec![0i16; method.error_len(4)];
        let mut done = 0;
        let mut d = Ditherer {
            method,
            original_thresholds: false,
            errors: &mut errors,
            rows_done: &mut done,
        };
        let mut out = [0u8; 1];
        for y in 0..5 {
            d.pixels(&[100, 20, 200, 60], 0, y, &mut out);
            d.next_row();
        }
        let rows = if method == Method::FloydSteinberg {
            2
        } else {
            3
        };
        let len = errors.len() / rows;
        // The row that will be current two rows (Atkinson) or one row (Floyd-Steinberg) from now
        // is the one just cleared.
        let cleared = (done as usize + rows - 1) % rows;
        assert!(
            errors[cleared * len..(cleared + 1) * len]
                .iter()
                .all(|&e| e == 0),
            "{method:?}"
        );
    }
}
