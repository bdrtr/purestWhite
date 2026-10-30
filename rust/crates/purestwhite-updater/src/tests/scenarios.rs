//! The recovery mode's scenarios: what the updater writes, notes and leaves for each card and
//! slot, and whether the firmware then resets into the C++ firmware or sleeps.

use super::fixtures::{Run, TestCard, Unreadable, card_with, digest, line, named, run, slot_with};
use crate::releases::Release;
use purestwhite_ota::image::CHIP_ESP32S3;
use purestwhite_ota::plan::InSlot;
use purestwhite_ota::testing::{FakeFlash, descriptor, good_image, image};
use std::format;
use std::string::String;
use std::vec::Vec;

/// Two releases the updater knows, and an image it does not.
struct Images {
    a: Vec<u8>,
    b: Vec<u8>,
    unknown: Vec<u8>,
}

fn images() -> Images {
    Images {
        a: good_image(20_000),
        b: good_image(30_000),
        unknown: good_image(25_000),
    }
}

impl Images {
    fn known(&self) -> [Release; 2] {
        [("test a", digest(&self.a)), ("test b", digest(&self.b))]
    }
}

/// The line of a write of `image` that went through.
fn wrote_line(plan: &str, image: &[u8]) -> String {
    std::format!("{plan}: {} sectors 0 ms", image.len().div_ceil(4096))
}

/// Whether the slot holds `image`, written by the updater, and its run says so.
fn installed(run: &Run<FakeFlash>, image: &[u8]) -> bool {
    run.done.wrote
        && run.done.after == InSlot::Image(digest(image))
        && run.flash.mem[..image.len()] == *image
}

#[test]
fn the_same_release_is_written_over_itself() {
    let i = images();
    let Some(card) = card_with(Some(&i.a)) else {
        return;
    };
    let r = run(slot_with(&i.a), card, &i.known(), false);
    assert_eq!(
        r.lines,
        [
            line("slot", &i.a, "test a"),
            line("card", &i.a, "test a"),
            wrote_line("Rewrite", &i.a),
            "installed: same digest".into(),
        ]
    );
    assert!(installed(&r, &i.a));
    assert!(!r.flash.erases.is_empty(), "the write path ran");
    // A sector each when the slot is read twice and the card twice, once written.
    let sectors = i.a.len().div_ceil(4096);
    assert!(r.feeds > 4 * sectors, "the watchdog is fed between pieces");
    assert!(r.done.resets(false));
}

#[test]
fn a_slot_the_bootloader_refused_is_repaired() {
    let i = images();
    let Some(card) = card_with(Some(&i.a)) else {
        return;
    };
    let r = run(cut_short(&i.a), card, &i.known(), true);
    // What emu/baremetal-repair.txt waits for, from the device's own cut-short slot.
    assert_eq!(
        r.lines,
        [
            "slot: Image(Checksum)".into(),
            line("card", &i.a, "test a"),
            wrote_line("Repair", &i.a),
            "installed: same digest".into(),
        ]
    );
    assert!(installed(&r, &i.a));
    assert!(
        r.done.resets(true),
        "it wrote the image the bootloader refused"
    );
}

/// A run that writes nothing: its last line, what it left in the slot, and whether the firmware
/// then resets.
fn refused(
    slot: FakeFlash,
    release: &[u8],
    known: &[Release],
    fell_back: bool,
) -> Option<Run<FakeFlash>> {
    let card = card_with(Some(release))?;
    let r = run(slot, card, known, fell_back);
    assert!(
        r.flash.erases.is_empty(),
        "nothing is erased: {:?}",
        r.lines
    );
    assert!(!r.done.wrote);
    Some(r)
}

#[test]
fn a_release_the_updater_does_not_know_is_refused() {
    let i = images();
    let Some(r) = refused(slot_with(&i.a), &i.unknown, &i.known(), false) else {
        return;
    };
    assert_eq!(r.lines[1], line("card", &i.unknown, "unknown"));
    assert_eq!(r.lines[2], "refused: Unknown");
    assert!(r.done.resets(false));
}

#[test]
fn another_release_in_the_slot_is_left_alone() {
    let i = images();
    let Some(r) = refused(slot_with(&i.b), &i.a, &i.known(), false) else {
        return;
    };
    assert_eq!(r.lines[0], line("slot", &i.b, "test b"));
    assert_eq!(r.lines[2], "refused: Another");
    assert_eq!(r.done.after, InSlot::Image(digest(&i.b)));
    assert!(r.done.resets(false), "the other release still boots");
}

#[test]
fn a_broken_slot_the_bootloader_did_not_refuse_is_left_alone() {
    let i = images();
    let mut slot = slot_with(&i.a);
    slot.mem[5000..9000].fill(0xFF);
    let Some(r) = refused(slot, &i.a, &i.known(), false) else {
        return;
    };
    assert_eq!(r.lines[2], "refused: Unconfirmed");
    assert_eq!(r.done.after, InSlot::Broken);
    assert!(
        !r.done.resets(false),
        "it sleeps: a reset would not boot that slot"
    );
}

#[test]
fn a_slot_that_passes_after_a_fall_back_is_left_alone() {
    let i = images();
    let Some(r) = refused(slot_with(&i.a), &i.a, &i.known(), true) else {
        return;
    };
    assert_eq!(r.lines[2], "refused: Disagree");
    assert!(
        !r.done.resets(true),
        "it sleeps rather than reset back into itself"
    );
}

#[test]
fn a_release_too_big_for_the_slot_is_refused() {
    let i = images();
    let big = good_image(super::SLOT);
    let Some(r) = refused(slot_with(&i.a), &big, &i.known(), false) else {
        return;
    };
    assert_eq!(r.lines[1..], ["card: Image(TooLarge)"]);
    assert!(r.done.resets(false));
}

#[test]
fn a_card_without_the_release_changes_nothing() {
    let i = images();
    let Some(card) = card_with(None) else { return };
    let r = run(slot_with(&i.a), card, &i.known(), false);
    assert_eq!(
        r.lines,
        [line("slot", &i.a, "test a"), "card: File(NotFound)".into()]
    );
    assert!(r.flash.erases.is_empty());
    assert!(r.done.resets(false), "the C++ firmware is still there");
}

#[test]
fn no_card_changes_nothing() {
    let i = images();
    let r = run(slot_with(&i.a), TestCard(None), &i.known(), false);
    assert_eq!(
        r.lines,
        [line("slot", &i.a, "test a"), "card: Missing".into()]
    );
    assert!(r.flash.erases.is_empty());
    assert!(r.done.resets(false));
}

#[test]
fn an_unreadable_slot_is_not_written() {
    let i = images();
    let Some(card) = card_with(Some(&i.a)) else {
        return;
    };
    let r = run(Unreadable(slot_with(&i.a)), card, &i.known(), true);
    assert_eq!(
        r.lines,
        [
            "slot: read at 0: FakeError".into(),
            line("card", &i.a, "test a"),
            String::from("refused: Unreadable"),
        ]
    );
    assert!(r.flash.0.erases.is_empty());
    assert_eq!(r.done.after, InSlot::Unreadable);
    assert!(!r.done.resets(true));
}

/// A write cut short (here the flash fails at the third sector) leaves no image that passes, and
/// the updater sleeps; after it the bootloader refuses the slot and falls back to the updater,
/// which repairs it.
#[test]
fn a_write_cut_short_is_repaired_on_the_next_run() {
    let i = images();
    let Some(card) = card_with(Some(&i.a)) else {
        return;
    };
    let mut slot = slot_with(&i.a);
    slot.fail_write_at = Some(2 * 4096);
    let first = run(slot, card, &i.known(), false);
    assert_eq!(first.lines[2], "Rewrite: Write(Flash(FakeError))");
    assert_eq!(
        first.lines.len(),
        3,
        "no image to note after: {:?}",
        first.lines
    );
    assert_eq!(first.done.after, InSlot::Broken);
    assert!(!first.done.resets(false), "it sleeps");

    let mut slot = first.flash;
    slot.fail_write_at = None;
    slot.erases.clear();
    let Some(card) = card_with(Some(&i.a)) else {
        return;
    };
    let second = run(slot, card, &i.known(), true);
    assert_eq!(second.lines[2], wrote_line("Repair", &i.a));
    assert!(installed(&second, &i.a));
    assert!(second.done.resets(true));
}

/// A slot of `image` with its second and third sectors erased: a write cut short.
fn cut_short(image: &[u8]) -> FakeFlash {
    let mut slot = slot_with(image);
    slot.mem[5000..9000].fill(0xFF);
    slot
}

/// An image the size of `good_image(20_000)` (the tests' release a) with other data.
fn same_size_other() -> Vec<u8> {
    let first = descriptor("0.0.3", "arduino-lib-builder");
    let data: Vec<u8> = (0..20_000usize).map(|i| (i * 3) as u8).collect();
    image(CHIP_ESP32S3, &[&first, &[1, 2, 3, 4, 5], &data], 1)
}

/// The card swapped between the passes: the second writes another image than the first read. The
/// slot then holds an image that passes, but not the one planned, and the updater did not write
/// what it planned: after a fall back it sleeps rather than reset into it.
#[test]
fn a_card_changed_between_the_passes_is_not_taken_as_written() {
    let i = images();
    let other = same_size_other();
    assert_eq!(other.len(), i.a.len());
    let (Some(mut card), Some(changed)) = (card_with(Some(&i.a)), card_with(Some(&other))) else {
        return;
    };
    card.0.as_mut().unwrap().then = Some(changed.0.unwrap().bytes);
    let r = run(cut_short(&i.a), card, &i.known(), true);
    assert_eq!(r.lines[1], line("card", &i.a, "test a"));
    let after = format!("installed: {}", named(&other, "unknown"));
    assert_eq!(r.lines[2..], ["Repair: Changed".into(), after]);
    assert_eq!(r.done.after, InSlot::Image(digest(&other)));
    assert!(!r.done.wrote);
    assert!(!r.done.resets(true));
}

#[test]
fn a_card_that_stops_answering_is_not_ready() {
    let i = images();
    let Some(mut card) = card_with(Some(&i.a)) else {
        return;
    };
    card.0.as_mut().unwrap().stall_after = Some(0);
    let r = run(slot_with(&i.a), card, &i.known(), false);
    assert_eq!(r.lines[1..], ["card: not ready"]);
    assert!(r.flash.erases.is_empty());
    assert!(r.done.resets(false));
}

/// A card that stops answering during the second pass leaves the slot as far as it was written,
/// and the updater sleeps.
#[test]
fn a_card_that_stops_answering_while_written_leaves_no_image() {
    let i = images();
    // Its last sector erased, so that it passes only once written whole.
    let mut slot = slot_with(&i.a);
    slot.mem[4 * 4096..i.a.len()].fill(0xFF);
    let Some(probe) = card_with(Some(&i.a)) else {
        return;
    };
    let first_pass = run(cut_short(&i.a), probe, &[], true).card.0.unwrap().reads;
    let Some(mut card) = card_with(Some(&i.a)) else {
        return;
    };
    // The second pass finds the file, writes its first piece, then waits for the card forever.
    card.0.as_mut().unwrap().stall_after = Some(first_pass + 2);
    let r = run(slot, card, &i.known(), true);
    assert_eq!(r.lines[2..], ["Repair: not ready"]);
    assert!(!r.flash.erases.is_empty(), "it stopped while writing");
    assert_eq!(r.done.after, InSlot::Broken);
    assert!(!r.done.resets(true));
}
