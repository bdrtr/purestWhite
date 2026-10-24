#![allow(clippy::indexing_slicing)]

extern crate std;

use super::{Ditherer, GrayBlock, Line, Method, Quantize, Scale, block_row, line_row, put_levels};
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

const ONE: i32 = 1 << 16;

#[test]
fn block_rows_sample_inside_the_block() {
    // Halving: columns past the block's two valid pixels take its last one, not the padding.
    let block = GrayBlock {
        pixels: &[0, 255, 0, 0],
        stride: 4,
        width: 2,
        height: 1,
        x: 0,
        y: 0,
    };
    let half = Scale {
        x: ONE / 2,
        inv_x: 2 * ONE,
        y: ONE / 2,
        inv_y: 2 * ONE,
    };
    let mut out = [0xAA; 2];
    block_row(&block, &half, 0, (0, 4), (0, 0), Quantize::Thirds, &mut out);
    assert_eq!(out, [0b00_11_11_11, 0xAA]);
}

#[test]
fn bayer_follows_the_screen_position() {
    let block = GrayBlock {
        pixels: &[128; 8],
        stride: 8,
        width: 8,
        height: 1,
        x: 0,
        y: 0,
    };
    let same = Scale {
        x: ONE,
        inv_x: ONE,
        y: ONE,
        inv_y: ONE,
    };
    let mut out = [0; 1];
    block_row(&block, &same, 0, (0, 4), (0, 0), Quantize::Bayer, &mut out);
    assert_eq!(out, [0b01_10_01_10]);
    block_row(&block, &same, 0, (0, 4), (1, 0), Quantize::Bayer, &mut out);
    assert_eq!(out, [0b10_01_10_01]);
    block_row(&block, &same, 0, (1, 5), (0, 0), Quantize::Bayer, &mut out);
    assert_eq!(out, [0b10_01_10_01]);
    // Row 1 of the matrix: offsets 20, -20, 30, -10.
    block_row(&block, &same, 0, (0, 4), (0, 1), Quantize::Bayer, &mut out);
    assert_eq!(out, [0b10_01_10_01]);
}

#[test]
fn levels_put_into_a_row_stay_in_it() {
    let mut row = [0; 2];
    put_levels(&mut row, -1, &[0b11_10_01_00], 4);
    put_levels(&mut row, 6, &[0b11_10_01_00], 4);
    assert_eq!(row, [0b10_01_00_00, 0b00_00_11_10]);
}

#[test]
fn line_rows_step_through_the_source_and_mask_by_position() {
    // Three source pixels over seven columns; alpha 100 shows against thresholds 0, 128, 32, 160.
    let gray = [0, 128, 255];
    let alpha = [100; 3];
    let line = Line {
        gray: &gray,
        alpha: Some(&alpha),
        crop: 0,
        visible: 3,
        dst_width: 7,
    };
    let (mut out, mut mask) = ([0; 2], [0; 1]);
    line_row(
        &line,
        (0, 7),
        (0, 0),
        Quantize::Quarters,
        &mut out,
        Some(&mut mask),
    );
    // Columns read pixels 0, 0, 0, 1, 1, 2, 2.
    assert_eq!(out, [0b00_00_00_10, 0b10_11_11_00]);
    assert_eq!(mask, [0b1010_1010]);
    // From column 1: the source steps and the thresholds follow the column.
    line_row(
        &line,
        (1, 5),
        (0, 0),
        Quantize::Quarters,
        &mut out,
        Some(&mut mask),
    );
    assert_eq!(out[0], 0b00_00_10_10);
    assert_eq!(mask, [0b0101_0000]);
}
