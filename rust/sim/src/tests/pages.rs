//! The page loop on a card in memory: pages written by `write_bmp`, read back by the storage loop
//! and shown in turn every 30 s of the simulator's clock.

use crate::bmp::write_bmp;
use crate::{Run, Script, run};
use purestwhite_app::Refresh::{Fast, Full};
use purestwhite_app::{LOOP_PAGES, PAGE_MS, Refresh};
use purestwhite_fat::Volume;
use purestwhite_fat::testing::{MemCard, ready, test_card};
use purestwhite_render::{FRAME_BYTES, WIDTH, set_black};

/// Page `n`: white, with a black band 20 rows high at row 100 n.
fn page(n: u16) -> Vec<u8> {
    let mut frame = vec![0xFF; FRAME_BYTES];
    for y in i32::from(n) * 100..i32::from(n) * 100 + 20 {
        for x in 0..WIDTH {
            set_black(&mut frame, x, y);
        }
    }
    frame
}

/// A card with pages 1 to `count`; `None` when mkfs.fat is not there (the test then passes).
fn card(count: u16) -> Option<MemCard> {
    let mut volume = ready(Volume::mount(test_card(4096)?)).unwrap().unwrap();
    ready(volume.mkdir("purestwhite-pages")).unwrap().unwrap();
    for n in 1..=count {
        let mut bytes = Vec::new();
        write_bmp(&page(n), &mut bytes).unwrap();
        let path = format!("purestwhite-pages/{n:03}.bmp");
        let mut file = ready(volume.create(&path)).unwrap().unwrap();
        ready(volume.write(&mut file, &bytes)).unwrap().unwrap();
        ready(volume.close(file)).unwrap().unwrap();
    }
    Some(volume.into_inner())
}

fn run_with(script: &str, card: Option<MemCard>) -> Run {
    run(&Script::parse(script).unwrap(), None, card).unwrap()
}

fn shots(run: &Run) -> Vec<(Refresh, u64)> {
    run.shots.iter().map(|s| (s.refresh, s.at_ms)).collect()
}

#[test]
fn right_shows_the_pages_in_turn_and_left_goes_home() {
    let Some(card) = card(3) else { return };
    let run = run_with(
        "press:right\nwait:30\nwait:30\nwait:30\npress:left",
        Some(card),
    );
    let ms = u64::from(PAGE_MS);
    assert_eq!(
        shots(&run),
        [
            (Full, 0),
            (Full, 0),
            (Fast, ms),
            (Fast, 2 * ms),
            (Fast, 3 * ms),
            (Full, 3 * ms)
        ]
    );
    for (shot, n) in run.shots[1..5].iter().zip([1, 2, 3, 1]) {
        assert!(shot.frame == page(n), "page {n} as written");
    }
    assert!(!run.slept);
}

#[test]
fn after_an_hour_of_pages_the_device_sleeps() {
    let Some(card) = card(2) else { return };
    let run = run_with("press:right\nwait:4000", Some(card));
    // Home, the pages, the sleep screen at the hour.
    assert_eq!(run.shots.len(), 1 + usize::from(LOOP_PAGES) + 1);
    assert_eq!(run.shots.last().unwrap().at_ms, 3_600_000);
    assert!(run.slept);
}

#[test]
fn without_a_card_right_does_nothing() {
    let run = run_with("press:right\nwait:60", None);
    assert_eq!(shots(&run), [(Full, 0)]);
}
