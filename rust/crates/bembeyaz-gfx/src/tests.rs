#![allow(
    clippy::indexing_slicing,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

extern crate std;

use super::{Canvas, Clip, Fill, Orientation};
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
            match next(5) {
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
                _ => {
                    c.fill_rect(i32::MIN, i32::MIN + 1, i32::MAX, i32::MAX, Fill::DarkGray);
                    c.fill_rect(i32::MAX, i32::MAX, i32::MAX, i32::MAX, Fill::Black);
                }
            }
        }
    }
}
