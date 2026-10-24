#![allow(
    clippy::indexing_slicing,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::too_many_lines
)]

extern crate std;

use super::{
    Bitmap, Canvas, Clip, Corners, Fill, Frame, Ink, Orientation, Picture, Placement, Plane,
    Region, Widen,
};
use std::vec;
use std::vec::Vec;

const OPEN: Clip = Clip {
    left: 0,
    top: 0,
    right: 32767,
    bottom: 32767,
};

/// A 16x8 panel, all white.
fn panel() -> Vec<u8> {
    vec![0xFF; 2 * 8]
}

fn canvas(buf: &mut [u8], orientation: Orientation) -> Canvas<'_> {
    Canvas {
        buf,
        stride: 2,
        origin_y: 0,
        rows: 8,
        panel_width: 16,
        panel_height: 8,
        orientation,
        clip: OPEN,
    }
}

/// The panel as rows of '#' (black) and '.'.
fn picture(buf: &[u8]) -> Vec<std::string::String> {
    buf.chunks(2)
        .map(|row| {
            (0..16)
                .map(|x| {
                    if row[x / 8] & (0x80 >> (x % 8)) == 0 {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect()
}

#[test]
fn orientations_map_logical_pixels_onto_the_panel() {
    for (orientation, expected) in [
        (Orientation::LandscapeCounterClockwise, (1, 2)),
        (Orientation::LandscapeClockwise, (14, 5)),
        (Orientation::Portrait, (2, 6)),
        (Orientation::PortraitInverted, (13, 1)),
    ] {
        let mut buf = panel();
        let mut c = canvas(&mut buf, orientation);
        assert_eq!(c.to_panel(1, 2), expected);
        assert!(c.pixel(1, 2, true));
        let (x, y) = expected;
        assert_eq!(buf[y as usize * 2 + x as usize / 8] & (0x80 >> (x % 8)), 0);
    }
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    assert!(!c.pixel(16, 0, true));
    // The clip's left and top edges keep out negative coordinates.
    assert!(c.pixel(-1, 0, true));
    c.clip.right = 4;
    assert!(c.pixel(5, 0, true));
    assert_eq!(buf, panel());
}

#[test]
fn lines_and_rectangles() {
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    assert_eq!(c.line(0, 0, 5, 3, true), 0);
    assert_eq!(c.rect(8, 1, 6, 5, 2, true), 0);
    assert_eq!(c.line(14, 7, 17, 7, true), 2);
    assert_eq!(
        picture(&buf),
        [
            "#...............",
            ".##.....######..",
            "...##...######..",
            ".....#..##..##..",
            "........######..",
            "........######..",
            "................",
            "..............##",
        ]
    );
}

#[test]
fn fills_are_clipped_and_dithered() {
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    c.fill_rect(1, 0, 6, 2, Fill::Black);
    c.fill_rect(3, 2, 12, 2, Fill::DarkGray);
    c.fill_rect(-2, 4, 30, 2, Fill::LightGray);
    c.clip = Clip {
        left: 0,
        top: 6,
        right: 9,
        bottom: 7,
    };
    c.fill_rect(0, 0, 16, 8, Fill::Black);
    c.fill_rect(0, 0, 0, 8, Fill::Black);
    assert_eq!(
        picture(&buf),
        [
            ".######.........",
            ".######.........",
            "....#.#.#.#.#.#.",
            "...#.#.#.#.#.#..",
            "#.#.#.#.#.#.#.#.",
            "................",
            "#########.......",
            "................",
        ]
    );
    // White over black, through a strip of rows 2 to 3.
    let mut strip = vec![0u8; 2 * 2];
    let mut c = Canvas {
        origin_y: 2,
        rows: 2,
        ..canvas(&mut strip, Orientation::LandscapeCounterClockwise)
    };
    c.fill_rect(4, 0, 4, 8, Fill::White);
    assert!(c.pixel(0, 2, false));
    assert!(c.pixel(0, 0, false));
    assert_eq!(strip, [0x8F, 0x00, 0x0F, 0x00]);
}

#[test]
fn dither_follows_logical_coordinates_in_every_orientation() {
    for orientation in [
        Orientation::Portrait,
        Orientation::LandscapeClockwise,
        Orientation::PortraitInverted,
        Orientation::LandscapeCounterClockwise,
    ] {
        for fill in [Fill::LightGray, Fill::DarkGray] {
            let mut fast = panel();
            let mut c = canvas(&mut fast, orientation);
            let (w, h) = c.screen_size();
            c.fill_rect(1, 1, w - 3, h - 2, fill);
            let mut slow = panel();
            let mut c = canvas(&mut slow, orientation);
            for y in 1..h - 1 {
                for x in 1..w - 2 {
                    let dark = match fill {
                        Fill::LightGray => x % 2 == 0 && y % 2 == 0,
                        _ => (x + y) % 2 == 0,
                    };
                    c.pixel(x, y, dark);
                }
            }
            assert_eq!(fast, slow, "{orientation:?} {fill:?}");
        }
    }
}

#[test]
fn rounded_shapes() {
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    assert_eq!(c.rounded_rect(0, 0, 8, 8, 1, 3, Corners::ALL, true), 0);
    let top = Corners::TOP_LEFT | Corners::TOP_RIGHT;
    c.fill_rounded_rect(9, 0, 7, 5, 3, top, Fill::Black);
    c.arc(2, 9, 7, 1, -1, 1, Fill::Black);
    assert_eq!(
        picture(&buf),
        [
            "...##.......#...",
            ".######...#####.",
            ".#....#...#####.",
            "##....##.#######",
            "##....##.#######",
            ".#....#..#......",
            ".######..##.....",
            "...##.....##....",
        ]
    );
    // A radius of zero is a plain rectangle, which counts pixels off the panel.
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    assert_eq!(c.rounded_rect(14, 6, 4, 2, 1, 0, Corners::ALL, true), 6);
}

#[test]
fn masked_corners() {
    let mut buf = vec![0u8; 2 * 8];
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    assert_eq!(c.mask_corners(0, 0, 8, 6, 3, Fill::White), 0);
    assert_eq!(c.mask_corners(10, 0, 8, 8, 3, Fill::LightGray), 6);
    assert_eq!(c.mask_corners(0, 0, 8, 8, 0, Fill::White), 0);
    assert_eq!(
        picture(&buf),
        [
            "..####..###.####",
            ".######.##.#####",
            "################",
            "################",
            ".######.########",
            "..####..########",
            "################",
            "##########..####",
        ]
    );
}

#[test]
fn polygons() {
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    // A triangle, and a bookmark ribbon with a notch, partly off the right edge.
    c.fill_polygon(&[0, 6, 0], &[0, 3, 6], Fill::Black);
    c.fill_polygon(&[10, 18, 18, 14, 10], &[0, 0, 7, 4, 7], Fill::Black);
    // Fewer than three points, and mismatched lengths cut to the shorter.
    c.fill_polygon(&[0, 15], &[7, 7], Fill::Black);
    c.fill_polygon(&[0, 15, 15, 0], &[7, 7, 7], Fill::Black);
    // As in the old code, an edge counts on a row only when one end lies above it, so the top row
    // of each shape stays empty.
    assert_eq!(
        picture(&buf),
        [
            "................",
            "###.......######",
            "#####.....######",
            "#######...######",
            "#####.....######",
            "###.......###..#",
            "#.........##....",
            "..........#.....",
        ]
    );
}

/// A 4x2 picture: black, dark, light, white in each row; bottom-up unless `top_down`.
fn four_levels(top_down: bool) -> (Picture, [[u8; 1]; 2]) {
    let pic = Picture {
        width: 4,
        height: 2,
        top_down,
        one_bit: false,
    };
    (pic, [[0b0001_1011]; 2])
}

fn place(x: i32, y: i32, plane: Plane) -> Placement {
    Placement {
        x,
        y,
        max_width: 0,
        max_height: 0,
        crop_x: 0.0,
        crop_y: 0.0,
        white_transparent: false,
        plane,
        absolute_gray: false,
    }
}

#[test]
fn pictures() {
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    let (pic, rows) = four_levels(true);
    let mut next = rows.iter();
    let mut row = [0u8; 1];
    let drawn = c.picture(&pic, &place(1, 1, Plane::Bw), &mut row, |r| {
        next.next().is_some_and(|src| {
            r.copy_from_slice(src);
            true
        })
    });
    assert_eq!(drawn.map(|d| (d.width, d.height)), Ok((4, 2)));
    // Shrunk to half, off the right edge; the third row fails to read.
    let (pic, _) = four_levels(false);
    let rows = [[0u8; 1]; 2];
    let mut next = rows.iter();
    let mut fit = place(14, 4, Plane::Bw);
    fit.max_width = 2;
    let drawn = c.picture(&pic, &fit, &mut row, |r| {
        next.next().is_some_and(|src| {
            r.copy_from_slice(src);
            true
        })
    });
    assert_eq!(drawn.map(|d| (d.scale, d.width, d.height)), Ok((0.5, 2, 1)));
    // A bottom-up picture whose first row is below the screen still draws the rows that come
    // back up onto it (the old renderer drew nothing).
    let mut other = panel();
    let mut c2 = canvas(&mut other, Orientation::LandscapeCounterClockwise);
    let mut reads = 0;
    let partly = c2.picture(&pic, &place(0, 7, Plane::Bw), &mut row, |r| {
        reads += 1;
        r.fill(0);
        true
    });
    assert_eq!((partly.map(|d| d.height), reads), (Ok(2), 2));
    assert_eq!(picture(&other)[7], "####............");
    let tall = Picture {
        height: 3,
        top_down: true,
        ..pic
    };
    let mut next = rows.iter();
    let failed = c.picture(&tall, &place(0, 5, Plane::Bw), &mut row, |r| {
        next.next().is_some_and(|src| {
            r.copy_from_slice(src);
            true
        })
    });
    assert_eq!(failed, Err(2));
    assert_eq!(
        picture(&buf),
        [
            "................",
            ".###............",
            ".###............",
            "................",
            "..............##",
            "####............",
            "####............",
            "................",
        ]
    );
}

#[test]
fn gray_planes() {
    // Each level's ink in the LSB and MSB planes, as differences and as absolute levels.
    for (plane, absolute, expected) in [
        (Plane::GrayLsb, false, ".#.."),
        (Plane::GrayMsb, false, ".##."),
        (Plane::GrayLsb, true, "#.#."),
        (Plane::GrayMsb, true, "##.."),
    ] {
        let mut buf = vec![0xFFu8; 2 * 8];
        // Ink that writes white shows on black.
        if !absolute {
            buf.fill(0);
        }
        let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
        let (pic, rows) = four_levels(true);
        let mut next = rows.iter();
        let mut row = [0u8; 1];
        let mut at = place(0, 0, plane);
        at.absolute_gray = absolute;
        assert!(
            c.picture(&pic, &at, &mut row, |r| {
                next.next().is_some_and(|src| {
                    r.copy_from_slice(src);
                    true
                })
            })
            .is_ok()
        );
        let first = picture(&buf).swap_remove(0);
        let shown: std::string::String = if absolute {
            first.chars().take(4).collect()
        } else {
            // Written pixels turned white.
            first
                .chars()
                .take(4)
                .map(|p| if p == '.' { '#' } else { '.' })
                .collect()
        };
        assert_eq!(shown, expected, "{plane:?} {absolute}");
    }
}

#[test]
fn icons_and_inversion() {
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    // A 3x3 icon with ink on its first row and its diagonal: rows run right to left.
    let bits = [0b0001_1111, 0b1011_1111, 0b1101_1111];
    assert_eq!(c.icon(&bits, 0, 0, 3), 0);
    assert_eq!(c.icon(&bits, 14, 0, 3), 3);
    c.invert_rect(4, 4, 10, 2);
    c.invert_rect(6, 5, 2, 1);
    assert_eq!(
        picture(&buf),
        [
            "..#.............",
            ".##............#",
            "#.#...........#.",
            "................",
            "....##########..",
            "....##..######..",
            "................",
            "................",
        ]
    );
}

#[test]
fn glyphs() {
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    // A 3x3 1-bit "L", upright, then rotated 90 degrees clockwise (glyph x up, glyph y right).
    let ell = [0b1001_0011, 0b1000_0000];
    let upright = Frame {
        x: 0,
        y: 0,
        dx_x: 1,
        dx_y: 0,
        dy_x: 0,
        dy_y: 1,
    };
    c.glyph(
        &Bitmap {
            bits: &ell,
            width: 3,
            height: 3,
            two_bit: false,
        },
        &upright,
        Plane::Bw,
        true,
    );
    let rotated = Frame {
        x: 5,
        y: 2,
        dx_x: 0,
        dx_y: -1,
        dy_x: 1,
        dy_y: 0,
    };
    c.glyph(
        &Bitmap {
            bits: &ell,
            width: 3,
            height: 3,
            two_bit: false,
        },
        &rotated,
        Plane::Bw,
        true,
    );
    // A 2-bit glyph (white, light, dark, black) in each plane (the gray planes only set bits, so
    // nothing shows on white), and clipped at the panel's edge.
    let levels = [0b0001_1011];
    for (y, plane) in [(4, Plane::Bw), (5, Plane::GrayMsb), (6, Plane::GrayLsb)] {
        let at = Frame { y, ..upright };
        c.glyph(
            &Bitmap {
                bits: &levels,
                width: 4,
                height: 1,
                two_bit: true,
            },
            &at,
            plane,
            true,
        );
    }
    let edge = Frame { x: 14, ..upright };
    c.glyph(
        &Bitmap {
            bits: &ell,
            width: 3,
            height: 3,
            two_bit: false,
        },
        &edge,
        Plane::Bw,
        true,
    );
    // Frames that are not an orthogonal unit rotation draw nothing.
    for skewed in [
        Frame { dx_y: 1, ..upright },
        Frame { dx_x: 2, ..upright },
        Frame {
            dy_x: 1,
            dy_y: 0,
            ..upright
        },
        Frame { dx_x: 0, ..upright },
    ] {
        let at = Frame {
            x: 8,
            y: 4,
            ..skewed
        };
        c.glyph(
            &Bitmap {
                bits: &ell,
                width: 3,
                height: 3,
                two_bit: false,
            },
            &at,
            Plane::Bw,
            true,
        );
    }
    assert_eq!(
        picture(&buf),
        [
            "#......#......#.",
            "#......#......#.",
            "###..###......##",
            "................",
            ".###............",
            "................",
            "................",
            "................",
        ]
    );
}

#[test]
fn scaled_glyphs() {
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    // A 4x4 1-bit box outline, halved (thin strokes survive) and grown to 6x6.
    let box_bits = [0b1111_1001, 0b1001_1111];
    let glyph = Bitmap {
        bits: &box_bits,
        width: 4,
        height: 4,
        two_bit: false,
    };
    assert_eq!(c.reduced_glyph(&glyph, (1, 2), (0, 0), true), 0);
    assert_eq!(c.resampled_glyph(&glyph, 384, (4, 0)), 0);
    // A 2x2 2-bit glyph of light gray at 3/4 draws nothing (no majority of black), and off the
    // right edge counts the pixels lost.
    let light = Bitmap {
        bits: &[0b0101_0101],
        width: 2,
        height: 2,
        two_bit: true,
    };
    assert_eq!(c.reduced_glyph(&light, (3, 4), (12, 0), true), 0);
    assert_eq!(c.reduced_glyph(&glyph, (1, 1), (14, 4), true), 6);
    // Out of range sizes and factors draw nothing.
    let huge = Bitmap {
        width: 5000,
        ..glyph
    };
    assert_eq!(c.reduced_glyph(&huge, (1, 2), (0, 0), true), 0);
    assert_eq!(c.resampled_glyph(&glyph, 0, (0, 0)), 0);
    assert_eq!(
        picture(&buf),
        [
            "##..######......",
            "##..######......",
            "....##...#......",
            "....##...#......",
            "....##..##....##",
            "....######....#.",
            "..............#.",
            "..............##",
        ]
    );
}

#[test]
fn regions() {
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    c.fill_rect(3, 1, 6, 3, Fill::Black);
    // Pixels 3..9 touch bytes 0 and 1; aligned, the same two bytes.
    let touched = c.region(3, 1, 6, 3, Widen::Touched);
    assert_eq!(
        touched,
        Some(Region {
            byte_x: 0,
            y: 1,
            bytes: 2,
            rows: 3
        })
    );
    assert_eq!(c.region(3, 1, 6, 3, Widen::Aligned), touched);
    let region = touched.unwrap_or(Region {
        byte_x: 0,
        y: 0,
        bytes: 0,
        rows: 0,
    });
    let mut copy = [0u8; 6];
    assert!(!c.read_region(&region, &mut copy[..5]));
    assert!(c.read_region(&region, &mut copy));
    assert_eq!(copy, [0xE0, 0x7F, 0xE0, 0x7F, 0xE0, 0x7F]);
    // Back in two rows lower.
    let lower = Region { y: 4, ..region };
    assert!(c.write_region(&lower, &copy));
    assert_eq!(picture(&buf)[5], "...######.......");
    // Off the panel, empty, or not all in the rows held: nothing.
    let c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    assert_eq!(c.region(16, 0, 4, 4, Widen::Touched), None);
    assert_eq!(c.region(0, 0, 0, 4, Widen::Aligned), None);
    let mut strip = vec![0u8; 2 * 2];
    let c = Canvas {
        origin_y: 2,
        rows: 2,
        ..canvas(&mut strip, Orientation::LandscapeCounterClockwise)
    };
    assert!(!c.read_region(&region, &mut copy));
    let empty = Canvas {
        buf: &mut [],
        ..canvas(&mut [], Orientation::LandscapeCounterClockwise)
    };
    assert!(!empty.read_region(&region, &mut copy));
}

#[test]
fn level_rows_stay_on_the_panel_and_in_the_strip() {
    let ink = Ink {
        plane: Plane::Bw,
        absolute: false,
    };
    // Twelve black levels from column 10 of a 16-wide panel: six land, none wrap to the next row.
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeCounterClockwise);
    c.level_row(&[0; 3], 12, 10, 3, ink);
    assert_eq!(picture(&buf)[3], "..........######");
    assert_eq!(picture(&buf)[4], "................");
    // Portrait: the row runs up the panel's column 2 (logical y) from panel row 7 - x; a strip of
    // panel rows 2 and 3 keeps two of them.
    let mut strip = vec![0xFFu8; 2 * 2];
    let mut c = Canvas {
        origin_y: 2,
        rows: 2,
        ..canvas(&mut strip, Orientation::Portrait)
    };
    c.level_row(&[0; 2], 8, 0, 2, ink);
    assert_eq!(strip, [0xDF, 0xFF, 0xDF, 0xFF]);
    // Rotated 180°: a row starting two columns left of the screen runs leftwards on the panel
    // from past its right edge; those two columns land nowhere (not in the row below).
    let mut buf = panel();
    let mut c = canvas(&mut buf, Orientation::LandscapeClockwise);
    c.level_row(&[0; 1], 4, -2, 5, ink);
    assert_eq!(picture(&buf)[2], "..............##");
    assert_eq!(picture(&buf)[3], "................");
}

/// Random canvases, strips, clips and shapes, including extreme fill rectangles: nothing panics.
#[test]
fn mini_fuzz() {
    let mut seed = 0x1234_5678u32;
    let mut next = move |n: u32| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed % n
    };
    let pick = |n: u32, next: &mut dyn FnMut(u32) -> u32| next(n) as i32;
    for _ in 0..2000 {
        let width = 1 + pick(40, &mut next);
        let height = 1 + pick(30, &mut next);
        let stride = (width as usize).div_ceil(8);
        let origin = pick(height as u32, &mut next);
        let rows = 1 + pick((height - origin) as u32, &mut next);
        let mut buf = vec![0xA5u8; stride * rows as usize];
        let orientation = [
            Orientation::Portrait,
            Orientation::LandscapeClockwise,
            Orientation::PortraitInverted,
            Orientation::LandscapeCounterClockwise,
        ][pick(4, &mut next) as usize];
        let mut c = Canvas {
            buf: &mut buf,
            stride,
            origin_y: origin,
            rows,
            panel_width: width,
            panel_height: height,
            orientation,
            clip: Clip {
                left: pick(60, &mut next) - 20,
                top: pick(60, &mut next) - 20,
                right: pick(80, &mut next) - 10,
                bottom: pick(80, &mut next) - 10,
            },
        };
        for _ in 0..8 {
            let v = |next: &mut dyn FnMut(u32) -> u32| pick(140, next) - 50;
            match next(14) {
                0 => {
                    c.pixel(v(&mut next), v(&mut next), next(2) == 0);
                }
                1 => {
                    c.line(v(&mut next), v(&mut next), v(&mut next), v(&mut next), true);
                }
                2 => {
                    c.rect(
                        v(&mut next),
                        v(&mut next),
                        v(&mut next),
                        v(&mut next),
                        pick(5, &mut next),
                        false,
                    );
                }
                3 => {
                    let fill = [Fill::Black, Fill::White, Fill::LightGray, Fill::DarkGray]
                        [pick(4, &mut next) as usize];
                    c.fill_rect(v(&mut next), v(&mut next), v(&mut next), v(&mut next), fill);
                }
                5 => {
                    let r = pick(30, &mut next) - 5;
                    c.arc(
                        r,
                        v(&mut next),
                        v(&mut next),
                        1,
                        -1,
                        pick(8, &mut next) - 2,
                        Fill::Black,
                    );
                }
                6 => {
                    let r = pick(30, &mut next) - 5;
                    let (x, y, w, h) = (v(&mut next), v(&mut next), v(&mut next), v(&mut next));
                    c.rounded_rect(x, y, w, h, pick(5, &mut next), r, Corners::ALL, true);
                }
                7 => {
                    let r = pick(30, &mut next) - 5;
                    let (x, y, w, h) = (v(&mut next), v(&mut next), v(&mut next), v(&mut next));
                    #[allow(clippy::cast_possible_truncation)]
                    let corners = Corners(next(16) as u8);
                    c.fill_rounded_rect(x, y, w, h, r, corners, Fill::DarkGray);
                    c.mask_corners(x, y, w, h, r, Fill::White);
                }
                10 => {
                    let pic = Picture {
                        width: pick(30, &mut next),
                        height: pick(30, &mut next),
                        top_down: next(2) == 0,
                        one_bit: next(3) == 0,
                    };
                    #[allow(clippy::cast_precision_loss)]
                    let at = Placement {
                        max_width: pick(40, &mut next) - 5,
                        max_height: pick(40, &mut next) - 5,
                        crop_x: pick(140, &mut next) as f32 / 100.0 - 0.2,
                        crop_y: pick(140, &mut next) as f32 / 100.0 - 0.2,
                        white_transparent: next(2) == 0,
                        absolute_gray: next(2) == 0,
                        ..place(v(&mut next), v(&mut next), Plane::GrayMsb)
                    };
                    let mut row = vec![0x5Au8; pick(10, &mut next) as usize];
                    let mut left = pick(40, &mut next);
                    let _ = c.picture(&pic, &at, &mut row, |_| {
                        left -= 1;
                        left >= 0
                    });
                    let bits = [0x3Cu8; 9];
                    c.icon(&bits, v(&mut next), v(&mut next), pick(30, &mut next) - 3);
                    c.invert_rect(v(&mut next), v(&mut next), v(&mut next), v(&mut next));
                }
                11 => {
                    let bits = [0xA7u8; 64];
                    let axes = [(1, 0, 0, 1), (0, -1, 1, 0), (-1, 0, 0, -1), (0, 1, 1, 0)]
                        [pick(4, &mut next) as usize];
                    let frame = Frame {
                        x: v(&mut next),
                        y: v(&mut next),
                        dx_x: axes.0,
                        dx_y: axes.1,
                        dy_x: axes.2,
                        dy_y: axes.3,
                    };
                    let plane =
                        [Plane::Bw, Plane::GrayLsb, Plane::GrayMsb][pick(3, &mut next) as usize];
                    let (w, h) = (pick(30, &mut next) - 2, pick(30, &mut next) - 2);
                    c.glyph(
                        &Bitmap {
                            bits: &bits,
                            width: w,
                            height: h,
                            two_bit: next(2) == 0,
                        },
                        &frame,
                        plane,
                        next(2) == 0,
                    );
                    // A skewed frame breaks the contract and draws nothing.
                    let skewed = Frame { dx_y: 1, ..frame };
                    c.glyph(
                        &Bitmap {
                            bits: &bits,
                            width: w,
                            height: h,
                            two_bit: true,
                        },
                        &skewed,
                        plane,
                        true,
                    );
                    let far = Frame {
                        x: i32::MAX,
                        y: i32::MIN,
                        ..frame
                    };
                    c.glyph(
                        &Bitmap {
                            bits: &bits,
                            width: w,
                            height: h,
                            two_bit: true,
                        },
                        &far,
                        plane,
                        true,
                    );
                }
                12 => {
                    let bits = [0x6Du8; 128];
                    let glyph = Bitmap {
                        bits: &bits,
                        width: pick(40, &mut next) - 4,
                        height: pick(40, &mut next) - 4,
                        two_bit: next(2) == 0,
                    };
                    let at = (v(&mut next), v(&mut next));
                    let fraction = (pick(5, &mut next) - 1, pick(5, &mut next) - 1);
                    c.reduced_glyph(&glyph, fraction, at, next(2) == 0);
                    c.resampled_glyph(&glyph, pick(2000, &mut next) - 100, at);
                    c.resampled_glyph(&glyph, i32::MAX, (i32::MAX, i32::MIN));
                }
                13 => {
                    let widen = if next(2) == 0 {
                        Widen::Aligned
                    } else {
                        Widen::Touched
                    };
                    let (x, y, w, h) = (v(&mut next), v(&mut next), v(&mut next), v(&mut next));
                    let mut copy = vec![0u8; pick(300, &mut next) as usize];
                    for r in [
                        c.region(x, y, w, h, widen),
                        c.region(i32::MIN, i32::MAX, i32::MAX, i32::MAX, widen),
                        c.region(i32::MAX, i32::MIN, i32::MAX, 1, widen),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let _ = c.read_region(&r, &mut copy);
                        let _ = c.write_region(&r, &copy);
                    }
                    let wild = Region {
                        byte_x: usize::MAX,
                        y: usize::MAX,
                        bytes: usize::MAX,
                        rows: usize::MAX,
                    };
                    let _ = c.read_region(&wild, &mut copy);
                    let _ = c.write_region(&wild, &copy);
                }
                8 => {
                    // Extreme positions; the work grows with the radius, so that stays small.
                    c.arc(9, i32::MAX, i32::MIN, -1, -1, i32::MIN, Fill::White);
                    c.fill_rounded_rect(
                        i32::MIN,
                        i32::MAX,
                        i32::MAX,
                        3,
                        9,
                        Corners::ALL,
                        Fill::Black,
                    );
                    c.rounded_rect(
                        i32::MAX,
                        i32::MIN,
                        i32::MAX,
                        i32::MAX,
                        i32::MAX,
                        9,
                        Corners::ALL,
                        false,
                    );
                    c.mask_corners(i32::MAX, i32::MIN, i32::MIN, i32::MAX, 9, Fill::DarkGray);
                }
                9 => {
                    let n = 3 + next(4) as usize;
                    let xs: Vec<i32> = (0..n).map(|_| v(&mut next)).collect();
                    let ys: Vec<i32> = (0..n).map(|_| v(&mut next)).collect();
                    c.fill_polygon(&xs, &ys, Fill::White);
                    c.fill_polygon(
                        &[i32::MIN, i32::MAX, 0],
                        &[i32::MAX, i32::MIN, 3],
                        Fill::Black,
                    );
                    c.fill_polygon(&[0, 5, 0], &[i32::MIN, i32::MIN + 1, -1], Fill::Black);
                }
                _ => {
                    c.fill_rect(i32::MIN, i32::MIN + 1, i32::MAX, i32::MAX, Fill::DarkGray);
                    c.fill_rect(i32::MAX, i32::MAX, i32::MAX, i32::MAX, Fill::Black);
                }
            }
        }
    }
}
