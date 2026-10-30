// The tests read pixels by index and unwrap the test font: a failure fails the test.
#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

extern crate std;

use super::*;
use purestwhite_app::view::{Home, Time};
use std::vec;
use std::vec::Vec;

/// Noto Serif at 14 px (bembeyaz-font's test data).
fn font() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../bembeyaz-font/testdata/noto-serif-14.cpfont"
    ))
    .unwrap()
}

fn black_count(frame: &[u8]) -> u32 {
    frame.iter().map(|b| (!b).count_ones()).sum()
}

fn home(battery: Option<u8>, time: Option<Time>) -> View {
    View::Home(Home { battery, time })
}

#[test]
fn the_home_screen_draws_its_bar_and_battery_without_a_font() {
    let mut frame = vec![0u8; FRAME_BYTES];
    draw(&home(Some(50), None), &mut frame, None);
    // The bar under the top.
    assert!(is_black(&frame, WIDTH / 2, 48));
    // The battery's outline, half filled: black near its left inside, white near its right.
    let x = WIDTH - 16 - 48 - 4;
    assert!(is_black(&frame, x, 13));
    assert!(is_black(&frame, x + 6, 24));
    assert!(!is_black(&frame, x + 40, 24));
    // The rest of the screen is white.
    assert!(!is_black(&frame, WIDTH / 2, HEIGHT / 2));
}

#[test]
fn a_full_battery_fills_more_than_an_empty_one() {
    let (mut empty, mut full) = (vec![0xFF; FRAME_BYTES], vec![0xFF; FRAME_BYTES]);
    draw(&home(Some(0), None), &mut empty, None);
    draw(&home(Some(100), None), &mut full, None);
    assert!(black_count(&full) > black_count(&empty));
    // No battery reading: no battery.
    let mut none = vec![0xFF; FRAME_BYTES];
    draw(&home(None, None), &mut none, None);
    assert!(black_count(&none) < black_count(&empty));
}

#[test]
fn text_is_drawn_with_a_font() {
    let bytes = font();
    let mut table = [0u8; 4096];
    let mut text = Text::open(&bytes, 0, &mut table).unwrap();
    let view = home(
        Some(87),
        Some(Time {
            hour: 21,
            minute: 30,
        }),
    );
    let (mut plain, mut with_text) = (vec![0xFF; FRAME_BYTES], vec![0xFF; FRAME_BYTES]);
    draw(&view, &mut plain, None);
    draw(&view, &mut with_text, Some(&mut text));
    assert!(black_count(&with_text) > black_count(&plain) + 500);
    // The name is centered: ink on both sides of the middle, on its line.
    let line = HEIGHT / 2 - text.line_height() / 2;
    let left = (0..WIDTH / 2).any(|x| is_black(&with_text, x, line));
    let right = (WIDTH / 2..WIDTH).any(|x| is_black(&with_text, x, line));
    assert!(left && right);
}

#[test]
fn the_sleep_screen_is_a_crescent() {
    let mut frame = vec![0u8; FRAME_BYTES];
    draw(&View::Sleep, &mut frame, None);
    let (cx, cy) = (WIDTH / 2, HEIGHT / 2);
    // Black at the moon's left edge, white where the cut disc took it away.
    assert!(is_black(&frame, cx - 80, cy));
    assert!(!is_black(&frame, cx + 40, cy - 25));
    // White far from the moon.
    assert!(!is_black(&frame, 10, 10));
}

#[test]
fn every_view_covers_the_whole_frame() {
    // Drawn over leftovers, a view leaves none.
    for view in [home(Some(10), None), View::Sleep] {
        let (mut clean, mut dirty) = (vec![0xFF; FRAME_BYTES], vec![0x00; FRAME_BYTES]);
        draw(&view, &mut clean, None);
        draw(&view, &mut dirty, None);
        assert_eq!(clean, dirty);
    }
}

#[test]
fn is_black_reads_what_the_canvas_drew_at_the_corners_and_white_off_the_screen() {
    for (x, y) in [
        (0, 0),
        (WIDTH - 1, 0),
        (0, HEIGHT - 1),
        (WIDTH - 1, HEIGHT - 1),
        (7, 9),
    ] {
        let mut frame = vec![0xFF; FRAME_BYTES];
        assert!(canvas(&mut frame).pixel(x, y, true));
        assert!(is_black(&frame, x, y), "({x}, {y})");
        assert_eq!(black_count(&frame), 1);
    }
    let frame = vec![0u8; FRAME_BYTES];
    assert!(!is_black(&frame, -1, 0));
    assert!(!is_black(&frame, WIDTH, 0));
    assert!(!is_black(&frame, 0, HEIGHT));
}

#[test]
fn a_pixel_set_black_reads_back_black_and_only_it() {
    let mut frame = vec![0xFFu8; FRAME_BYTES];
    for (x, y) in [
        (0, 0),
        (WIDTH - 1, 0),
        (0, HEIGHT - 1),
        (WIDTH - 1, HEIGHT - 1),
        (7, 9),
    ] {
        set_black(&mut frame, x, y);
        assert!(is_black(&frame, x, y), "{x},{y}");
    }
    let black: u32 = frame.iter().map(|b| (!b).count_ones()).sum();
    assert_eq!(black, 5);
    // Off the screen: nothing.
    set_black(&mut frame, -1, 0);
    set_black(&mut frame, WIDTH, HEIGHT);
    assert_eq!(frame.iter().map(|b| (!b).count_ones()).sum::<u32>(), 5);
}
