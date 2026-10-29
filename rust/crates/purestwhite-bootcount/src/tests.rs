#![allow(clippy::unwrap_used)]

use super::*;

/// The record the next run sees after one that was `begun` ended without being healthy.
fn crash(begun: Runs) -> Runs {
    Runs::begin(Runs::decode(begun.encode()), false)
}

#[test]
fn a_record_reads_back_as_it_was_written() {
    for unfinished in [0, 1, 2, 3, 200, 255] {
        for running in [false, true] {
            let runs = Runs {
                unfinished,
                running,
            };
            assert_eq!(Runs::decode(runs.encode()), Some(runs));
        }
    }
}

#[test]
fn memory_that_holds_no_record_reads_as_none() {
    let good = Runs {
        unfinished: 2,
        running: true,
    }
    .encode();
    let mut no_magic = good;
    no_magic[0] ^= 1;
    let mut bad_copy = good;
    bad_copy[2] ^= 0x100;
    let mut bad_value = good;
    bad_value[1] |= 0x1000;
    bad_value[2] = !bad_value[1];
    for words in [no_magic, bad_copy, bad_value, [0; WORDS], [u32::MAX; WORDS]] {
        assert_eq!(Runs::decode(words), None, "{words:x?}");
    }
}

#[test]
fn three_runs_in_a_row_that_never_got_healthy_call_for_recovery() {
    let first = Runs::begin(None, true);
    assert_eq!(
        first,
        Runs {
            unfinished: 0,
            running: true
        }
    );
    let second = crash(first);
    let third = crash(second);
    let fourth = crash(third);
    assert_eq!(
        [second, third, fourth].map(|runs| runs.unfinished),
        [1, 2, 3]
    );
    assert!(!third.recovery_due());
    assert!(fourth.recovery_due());
}

#[test]
fn a_healthy_run_starts_the_count_again() {
    let crashed = crash(crash(Runs::begin(None, true)));
    assert_eq!(crashed.unfinished, 2);
    let next = Runs::begin(Runs::decode(Runs::healthy().encode()), false);
    assert_eq!(
        next,
        Runs {
            unfinished: 0,
            running: true
        }
    );
}

#[test]
fn a_power_on_or_no_record_starts_from_zero() {
    let due = Runs {
        unfinished: LIMIT,
        running: true,
    };
    assert_eq!(Runs::begin(Some(due), true).unfinished, 0);
    assert_eq!(Runs::begin(None, false).unfinished, 0);
}

#[test]
fn the_count_stops_at_its_largest() {
    let most = Runs {
        unfinished: u8::MAX,
        running: true,
    };
    assert_eq!(crash(most).unfinished, u8::MAX);
}

#[test]
fn a_report_line_says_how_many_and_whether_recovery_is_due() {
    extern crate std;
    use std::string::ToString;
    let runs = |unfinished| Runs {
        unfinished,
        running: true,
    };
    assert_eq!(runs(0).to_string(), "runs: 0 unfinished");
    assert_eq!(runs(3).to_string(), "runs: 3 unfinished, recovery due");
}
