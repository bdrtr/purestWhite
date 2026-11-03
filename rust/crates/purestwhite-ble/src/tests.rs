//! Advertisements as remotes send them, read for what makes them HID, and a scan's tally.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::advert::{HID_SERVICE, KEYBOARD_APPEARANCE};
use crate::{Advert, SEEN_MOST, Seen};
use std::string::ToString;
use std::vec::Vec;

/// AD structures from (type, value) pairs.
fn ad(parts: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (kind, value) in parts {
        out.push(u8::try_from(value.len() + 1).unwrap());
        out.push(*kind);
        out.extend_from_slice(value);
    }
    out
}

const FLAGS: (u8, &[u8]) = (0x01, &[0x06]);

#[test]
fn a_remote_lists_the_hid_service_or_the_keyboard_appearance() {
    let service = ad(&[FLAGS, (0x03, &[0x0F, 0x18, 0x12, 0x18])]);
    assert!(Advert::read(&service).is_hid());
    let incomplete = ad(&[(0x02, &HID_SERVICE.to_le_bytes())]);
    assert!(Advert::read(&incomplete).is_hid());
    let keyboard = ad(&[
        FLAGS,
        (0x19, &KEYBOARD_APPEARANCE.to_le_bytes()),
        (0x09, b"Turner"),
    ]);
    let read = Advert::read(&keyboard);
    assert!(read.is_hid() && read.named);
    let mouse = ad(&[(0x19, &0x03C2u16.to_le_bytes()), (0x03, &[0x0F, 0x18])]);
    assert!(
        !Advert::read(&mouse).is_hid(),
        "a mouse, with the battery service only"
    );
}

#[test]
fn the_hid_service_in_its_128_bit_form_counts_too() {
    let mut uuid = [
        0xFB, 0x34, 0x9B, 0x5F, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x12, 0x18, 0x00,
        0x00,
    ];
    assert!(Advert::read(&ad(&[(0x07, &uuid)])).is_hid());
    uuid[0] ^= 1;
    assert!(!Advert::read(&ad(&[(0x07, &uuid)])).is_hid());
}

#[test]
fn broken_data_ends_the_reading_without_a_fault() {
    let mut data = ad(&[(0x19, &KEYBOARD_APPEARANCE.to_le_bytes())]);
    data.extend_from_slice(&[9, 0x03, 0x12]);
    assert!(
        Advert::read(&data).is_hid(),
        "what came before the broken structure counts"
    );
    assert_eq!(Advert::read(&[0, 0x03, 0x12, 0x18]), Advert::default());
    assert_eq!(Advert::read(&[]), Advert::default());
    assert_eq!(Advert::read(&[1]), Advert::default());
}

#[test]
fn a_scan_counts_each_advertiser_once_with_its_scan_response_and_best_signal() {
    let mut seen = Seen::new();
    let remote = (1, [1, 2, 3, 4, 5, 6]);
    let phone = (1, [9, 9, 9, 9, 9, 9]);
    // The remote's advertisement says nothing; its scan response lists HID.
    seen.report(remote, &ad(&[FLAGS]), -70);
    seen.report(phone, &ad(&[(0x09, b"Phone")]), -40);
    seen.report(remote, &ad(&[(0x03, &[0x12, 0x18])]), -62);
    seen.report(remote, &ad(&[FLAGS]), -75);
    assert_eq!(seen.devices(), 2);
    assert_eq!(seen.remotes(), 1);
    assert_eq!(seen.best_remote(), Some(-62));
    assert_eq!(seen.to_string(), "ble: 2 devices, 1 hid, best hid -62 dBm");
}

#[test]
fn a_crowded_scan_says_so_and_without_a_remote_says_that() {
    let mut seen = Seen::new();
    for i in 0..=SEEN_MOST {
        seen.report((0, [u8::try_from(i).unwrap(), 0, 0, 0, 0, 0]), &[], -80);
    }
    assert_eq!(
        seen.to_string(),
        std::format!("ble: {SEEN_MOST}+ devices, no hid")
    );
    assert_eq!(Seen::new().to_string(), "ble: 0 devices, no hid");
}

#[test]
fn the_appearance_is_the_first_field_of_two_bytes_as_nimble_reads_it() {
    let long = [4, 0x19, 0xC1, 0x03, 0x00];
    assert!(!Advert::read(&long).is_hid());
    let mut twice = ad(&[(0x19, &0x03C2u16.to_le_bytes())]);
    twice.extend(ad(&[(0x19, &KEYBOARD_APPEARANCE.to_le_bytes())]));
    assert_eq!(Advert::read(&twice).appearance, Some(0x03C2));
}

#[test]
fn a_remote_in_a_crowded_scan_takes_a_place_and_a_missing_signal_does_not_count() {
    let mut seen = Seen::new();
    for i in 0..SEEN_MOST {
        seen.report((0, [u8::try_from(i).unwrap(), 0, 0, 0, 0, 0]), &[], -40);
    }
    let remote = ad(&[(0x03, &HID_SERVICE.to_le_bytes())]);
    seen.report((1, [7; 6]), &remote, 127);
    assert_eq!(seen.remotes(), 1);
    assert_eq!(seen.best_remote(), None, "127 is HCI's no RSSI");
    seen.report((1, [7; 6]), &remote, -66);
    assert_eq!(
        seen.to_string(),
        std::format!("ble: {SEEN_MOST}+ devices, 1 hid, best hid -66 dBm")
    );
}
