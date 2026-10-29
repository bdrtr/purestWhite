use super::*;

const A: [u8; DIGEST] = [0xA1; DIGEST];
const B: [u8; DIGEST] = [0xB2; DIGEST];

#[test]
fn a_known_release_is_written_over_itself_when_the_bootloader_booted_the_updater_by_choice() {
    assert_eq!(plan(&A, true, InSlot::Image(A), false), Plan::Rewrite);
}

#[test]
fn a_slot_the_bootloader_refused_but_that_reads_as_passing_is_left_alone() {
    for slot in [InSlot::Image(A), InSlot::Image(B)] {
        assert_eq!(plan(&A, true, slot, true), Plan::Refuse(Refusal::Disagree));
    }
}

#[test]
fn a_broken_slot_is_written_over_only_once_the_bootloader_refused_it() {
    assert_eq!(plan(&A, true, InSlot::Broken, true), Plan::Repair);
    assert_eq!(
        plan(&A, true, InSlot::Broken, false),
        Plan::Refuse(Refusal::Unconfirmed)
    );
}

#[test]
fn a_slot_holding_another_image_that_passes_is_left_alone() {
    assert_eq!(
        plan(&A, true, InSlot::Image(B), false),
        Plan::Refuse(Refusal::Another)
    );
}

#[test]
fn an_unknown_image_is_never_written_whatever_the_slot_holds() {
    for slot in [InSlot::Image(A), InSlot::Broken, InSlot::Unreadable] {
        for fell_back in [false, true] {
            assert_eq!(
                plan(&A, false, slot, fell_back),
                Plan::Refuse(Refusal::Unknown)
            );
        }
    }
}

#[test]
fn a_slot_that_cannot_be_read_is_not_written() {
    for fell_back in [false, true] {
        assert_eq!(
            plan(&A, true, InSlot::Unreadable, fell_back),
            Plan::Refuse(Refusal::Unreadable)
        );
    }
}

#[test]
fn the_updater_resets_into_a_slot_that_passes_unless_the_bootloader_refused_it_and_it_wrote_nothing()
 {
    let image = InSlot::Image(A);
    assert!(resets(image, false, false));
    assert!(resets(image, true, false));
    assert!(resets(image, true, true));
    assert!(!resets(image, false, true));
    for after in [InSlot::Broken, InSlot::Unreadable] {
        for (wrote, fell_back) in [(false, false), (true, false), (false, true), (true, true)] {
            assert!(
                !resets(after, wrote, fell_back),
                "{after:?} {wrote} {fell_back}"
            );
        }
    }
}
