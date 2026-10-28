// The tests index and unwrap directly: a wrong entry fails the test.
#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

use super::*;

const ERASED: Entry = [0xFF; ENTRY];

/// An entry as ESP-IDF writes it, its CRC right.
fn entry(seq: u32, state: u32) -> Entry {
    with_crc(seq, state, seq_crc(seq))
}

fn with_crc(seq: u32, state: u32, crc: u32) -> Entry {
    let mut e = ERASED;
    e[0..4].copy_from_slice(&seq.to_le_bytes());
    e[24..28].copy_from_slice(&state.to_le_bytes());
    e[28..32].copy_from_slice(&crc.to_le_bytes());
    e
}

/// The entries after `selection`'s write.
fn written(mut entries: [Entry; 2], selection: Selection) -> [Entry; 2] {
    if let Some(w) = selection.write {
        entries[w.sector as usize] = w.entry;
    }
    entries
}

/// The sequence number of the entry `selection` writes.
fn written_seq(selection: Selection) -> u32 {
    word(&selection.write.unwrap().entry, 0)
}

#[test]
fn the_crc_is_esp_idfs() {
    // The CRCs ESP-IDF writes into otadata for the first sequence numbers.
    assert_eq!(seq_crc(1), 0x4743_989A);
    assert_eq!(seq_crc(2), 0x55F6_3774);
}

#[test]
fn a_written_entry_is_complete_and_valid() {
    let s = select_other(&[ERASED, ERASED], Slot::Ota1).unwrap();
    let w = s.write.unwrap();
    assert_eq!(w.sector, 0);
    assert_eq!(w.entry, entry(1, STATE_VALID));
    assert!(w.entry[4..24].iter().all(|&b| b == 0xFF));
}

#[test]
fn erased_otadata_gets_the_first_number_selecting_the_other_slot() {
    let to_ota0 = select_other(&[ERASED, ERASED], Slot::Ota1).unwrap();
    assert_eq!((written_seq(to_ota0), to_ota0.booted_state), (1, None));
    let to_ota1 = select_other(&[ERASED, ERASED], Slot::Ota0).unwrap();
    assert_eq!((written_seq(to_ota1), to_ota1.booted_state), (2, None));
}

#[test]
fn the_running_slots_entry_is_reported_and_followed() {
    // Booted from ota_1 by seq 4, pending verify: seq 5 selects ota_0, in the other sector.
    let entries = [entry(3, STATE_VALID), entry(4, STATE_PENDING_VERIFY)];
    let s = select_other(&entries, Slot::Ota1).unwrap();
    assert_eq!(s.booted_state, Some(STATE_PENDING_VERIFY));
    assert_eq!((s.write.unwrap().sector, written_seq(s)), (0, 5));
}

#[test]
fn nothing_is_written_when_the_other_slot_is_already_selected_and_valid() {
    let entries = [entry(5, STATE_VALID), entry(4, STATE_NEW)];
    let s = select_other(&entries, Slot::Ota1).unwrap();
    assert_eq!(
        s,
        Selection {
            booted_state: None,
            write: None
        }
    );
}

#[test]
fn an_entry_for_the_other_slot_not_yet_valid_is_rewritten_valid() {
    // seq 5 selects ota_0 but is new: seq 7 selects it again, valid.
    let entries = [entry(5, STATE_NEW), ERASED];
    let s = select_other(&entries, Slot::Ota1).unwrap();
    assert_eq!((s.write.unwrap().sector, written_seq(s)), (1, 7));
}

#[test]
fn the_higher_number_wins_and_a_tie_goes_to_the_first_sector() {
    let newer_second = [entry(6, STATE_VALID), entry(7, STATE_VALID)];
    let s = select_other(&newer_second, Slot::Ota0).unwrap();
    // seq 7 selects ota_0: the running slot; the next for ota_1 is 8, into sector 0.
    assert_eq!(s.booted_state, Some(STATE_VALID));
    assert_eq!((s.write.unwrap().sector, written_seq(s)), (0, 8));
    let tie = [entry(3, STATE_VALID), entry(3, STATE_NEW)];
    let s = select_other(&tie, Slot::Ota0).unwrap();
    assert_eq!(s.booted_state, Some(STATE_VALID));
    assert_eq!((s.write.unwrap().sector, written_seq(s)), (1, 4));
}

#[test]
fn entries_the_bootloader_ignores_are_ignored() {
    for ignored in [
        entry(9, STATE_INVALID),
        entry(9, STATE_ABORTED),
        with_crc(9, STATE_VALID, seq_crc(9) ^ 1),
        with_crc(u32::MAX, STATE_VALID, seq_crc(u32::MAX)),
    ] {
        let s = select_other(&[ignored, entry(2, STATE_VALID)], Slot::Ota1).unwrap();
        // seq 2 is in force (ota_1); 3 selects ota_0, into sector 0 over the ignored one.
        assert_eq!(s.booted_state, Some(STATE_VALID));
        assert_eq!((s.write.unwrap().sector, written_seq(s)), (0, 3));
    }
}

#[test]
fn the_numbers_running_out_is_an_error() {
    // 0xFFFF_FFFE selects ota_1; the next for ota_0 would be the erased number.
    let entries = [entry(0xFFFF_FFFE, STATE_VALID), ERASED];
    assert_eq!(select_other(&entries, Slot::Ota1), Err(SeqExhausted));
    // 0xFFFF_FFFD selects ota_0; 0xFFFF_FFFE selects ota_1 and is still allowed.
    let entries = [entry(0xFFFF_FFFD, STATE_VALID), ERASED];
    let s = select_other(&entries, Slot::Ota0).unwrap();
    assert_eq!(written_seq(s), 0xFFFF_FFFE);
}

#[test]
fn after_the_write_the_other_slot_is_in_force_and_valid() {
    let starts = [
        [ERASED, ERASED],
        [entry(1, STATE_VALID), ERASED],
        [ERASED, entry(2, STATE_PENDING_VERIFY)],
        [entry(10, STATE_NEW), entry(9, STATE_VALID)],
        [entry(10, STATE_INVALID), entry(11, STATE_ABORTED)],
        [entry(4, STATE_VALID), entry(4, STATE_VALID)],
    ];
    for start in starts {
        for booted in [Slot::Ota0, Slot::Ota1] {
            let s = select_other(&start, booted).unwrap();
            let after = written(start, s);
            let again = select_other(&after, booted).unwrap();
            assert_eq!(again.write, None, "{start:?} from {booted:?}");
            assert_eq!(again.booted_state, None, "{start:?} from {booted:?}");
            // The entry in force before, if any, is kept: the write went to the other sector.
            if let Some(w) = s.write {
                let kept = 1 - w.sector as usize;
                assert_eq!(after[kept], start[kept]);
            }
        }
    }
}

#[test]
fn slots_and_sequence_numbers() {
    assert_eq!(Slot::of_seq(1), Slot::Ota0);
    assert_eq!(Slot::of_seq(2), Slot::Ota1);
    assert_eq!(Slot::of_seq(0), Slot::Ota1);
    assert_eq!(Slot::from_index(1), Some(Slot::Ota1));
    assert_eq!(Slot::from_index(2), None);
    assert_eq!(Slot::Ota0.other(), Slot::Ota1);
}
