// The tests unwrap and index what must be there: a failure fails the test.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use super::*;
use purestwhite_app::Refresh::{Fast, Full};

mod pages;

/// Noto Serif at 14 px (bembeyaz-font's test data).
fn font() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../crates/bembeyaz-font/testdata/noto-serif-14.cpfont"
    ))
    .unwrap()
}

fn scenario(name: &str) -> Script {
    let path = format!("{}/scenarios/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    Script::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn refreshes(run: &Run) -> Vec<Refresh> {
    run.shots.iter().map(|s| s.refresh).collect()
}

#[test]
fn every_scenario_runs() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios");
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let name = name.strip_suffix(".txt").unwrap().to_owned();
        assert!(run(&scenario(&name), None, None).is_ok(), "{name}");
        names.push(name);
    }
    names.sort();
    // Each scenario also has its test below.
    assert_eq!(names, ["home", "idle", "keys"]);
}

#[test]
fn the_home_scenario_shows_home_its_updates_and_the_sleep_screen() {
    let run = run(&scenario("home"), None, None).unwrap();
    // Home; the battery; the time; the same battery draws nothing; a new reading; sleep.
    assert_eq!(refreshes(&run), [Full, Fast, Fast, Fast, Full]);
    assert!(run.slept);
}

#[test]
fn the_idle_scenario_sleeps_ten_minutes_after_the_last_press() {
    let run = run(&scenario("idle"), None, None).unwrap();
    assert_eq!(refreshes(&run), [Full, Fast, Full]);
    let at: Vec<u64> = run.shots.iter().map(|s| s.at_ms).collect();
    assert_eq!(at, [0, 1_198_000, 1_199_000]);
    assert!(run.slept);
}

#[test]
fn a_long_wait_without_keys_sleeps_at_ten_minutes() {
    let run = run(&Script::parse("wait:3600").unwrap(), None, None).unwrap();
    assert_eq!(run.shots.last().unwrap().at_ms, 600_000);
    assert!(run.slept);
}

#[test]
fn the_keys_scenario_stays_on_home_and_awake() {
    let run = run(&scenario("keys"), None, None).unwrap();
    assert_eq!(refreshes(&run), [Full, Fast]);
    assert!(!run.slept);
}

#[test]
fn a_run_ends_at_the_sleep_request() {
    let script = Script::parse("sleep\nbattery:10\nbattery:20").unwrap();
    let run = run(&script, None, None).unwrap();
    assert_eq!(refreshes(&run), [Full, Full]);
    assert!(run.slept);
}

#[test]
fn text_from_the_font_adds_ink_to_every_screen() {
    let font = font();
    let (plain, with_text) = (
        run(&scenario("home"), None, None).unwrap(),
        run(&scenario("home"), Some(&font), None).unwrap(),
    );
    let ink = |shot: &Shot| shot.frame.iter().map(|b| (!b).count_ones()).sum::<u32>();
    for (a, b) in plain.shots.iter().zip(&with_text.shots) {
        assert!(ink(b) > ink(a));
    }
}

#[test]
fn a_shot_is_written_as_an_upright_png() {
    let run = run(&scenario("home"), None, None).unwrap();
    let mut bytes = Vec::new();
    write_png(&run.shots[0], &mut bytes).unwrap();
    let decoder = png::Decoder::new(bytes.as_slice());
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((info.width, info.height), (480, 800));
    // The bar under the top is black, the middle of an empty home screen white.
    assert_eq!(pixels[48 * 480 + 240], 0);
    assert_eq!(pixels[400 * 480 + 240], 255);
}

#[test]
fn shots_are_named_by_order_and_refresh_and_only_they_are_cleared() {
    assert_eq!(shot_name(0, Full), "0001-full.png");
    assert_eq!(shot_name(41, Refresh::Half), "0042-half.png");
    let dir = std::env::temp_dir().join(format!("pw-sim-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in [
        "0001-full.png",
        "0002-fast.png",
        "notes.png",
        "0003-full.txt",
        "01-full.png",
    ] {
        std::fs::write(dir.join(name), b"").unwrap();
    }
    assert_eq!(clear_shots(&dir).unwrap(), 2);
    let mut left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(left, ["0003-full.txt", "01-full.png", "notes.png"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_broken_font_is_an_error() {
    assert!(matches!(
        run(&scenario("home"), Some(&[0u8; 16]), None),
        Err(RunError::Font(_))
    ));
}
