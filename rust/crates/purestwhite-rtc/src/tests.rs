use super::*;

/// A clock's registers, and what was done on the bus.
#[derive(Default)]
struct FakeBus {
    regs: [u8; 16],
    /// Transfers that fail, by their number from 1.
    fail: &'static [usize],
    transfers: usize,
    written: [Option<(u8, [u8; 8], usize)>; 4],
    writes: usize,
    paused_ms: u32,
}

impl FakeBus {
    fn failing(&mut self) -> bool {
        self.transfers += 1;
        self.fail.contains(&self.transfers)
    }
}

impl Bus for FakeBus {
    fn write(&mut self, addr: u8, bytes: &[u8]) -> bool {
        if self.failing() {
            return false;
        }
        let mut copy = [0; 8];
        copy.iter_mut().zip(bytes).for_each(|(d, s)| *d = *s);
        if let Some(slot) = self.written.get_mut(self.writes) {
            *slot = Some((addr, copy, bytes.len()));
        }
        self.writes += 1;
        true
    }

    fn write_read(&mut self, _addr: u8, out: &[u8], buf: &mut [u8]) -> bool {
        if self.failing() {
            return false;
        }
        let [reg] = out else {
            return false;
        };
        let reg = *reg;
        for (i, b) in buf.iter_mut().enumerate() {
            *b = self.regs.get(usize::from(reg) + i).copied().unwrap_or(0);
        }
        true
    }

    fn pause_ms(&mut self, ms: u32) {
        self.paused_ms += ms;
    }
}

#[test]
fn bcd_round_trips_two_digits() {
    for v in 0..100 {
        assert_eq!(from_bcd(to_bcd(v)), v);
    }
    assert_eq!(to_bcd(59), 0x59);
    // As the C++ driver stored them: the tens digit spills over.
    assert_eq!(to_bcd(255), 0x95);
}

#[test]
fn begin_turns_clkout_off() {
    let mut bus = FakeBus::default();
    assert!(begin(&mut bus, 0x51));
    assert_eq!(
        bus.written[0],
        Some((0x51, [CLKOUT, 0, 0, 0, 0, 0, 0, 0], 2))
    );
    assert_eq!(bus.paused_ms, 0);
}

#[test]
fn begin_retries_a_failed_probe() {
    let mut bus = FakeBus {
        fail: &[1, 2],
        ..FakeBus::default()
    };
    assert!(begin(&mut bus, 0x51));
    assert_eq!(bus.paused_ms, 20);
    let mut absent = FakeBus {
        fail: &[1, 2, 3],
        ..FakeBus::default()
    };
    assert!(!begin(&mut absent, 0x51));
    assert_eq!(absent.paused_ms, 30);
    assert_eq!(absent.writes, 0);
}

#[test]
fn now_reads_bcd_and_the_century() {
    let mut bus = FakeBus::default();
    bus.regs[2..9].copy_from_slice(&[0x56, 0x34, 0x12, 0x25, 0x00, 0x10, 0x26]);
    let time = now(&mut bus, 0x51);
    let expected = DateTime {
        year: 2026,
        month: 10,
        day: 25,
        hour: 12,
        minute: 34,
        second: 56,
        weekday: 0,
    };
    assert_eq!(time, Some(expected));
    bus.regs[7] |= CENTURY;
    assert_eq!(now(&mut bus, 0x51).map(|t| t.year), Some(1926));
}

#[test]
fn a_stopped_oscillator_gives_no_time() {
    let mut bus = FakeBus::default();
    bus.regs[2] = VOLTAGE_LOW | 0x10;
    assert_eq!(now(&mut bus, 0x51), None);
    let mut failing = FakeBus {
        fail: &[1],
        ..FakeBus::default()
    };
    assert_eq!(now(&mut failing, 0x51), None);
}

#[test]
fn set_writes_the_time_registers() {
    let mut bus = FakeBus::default();
    let time = DateTime {
        year: 1999,
        month: 12,
        day: 31,
        hour: 23,
        minute: 59,
        second: 58,
        weekday: 5,
    };
    assert!(set(&mut bus, 0x51, &time));
    assert_eq!(
        bus.written[0],
        Some((0x51, [TIME, 0x58, 0x59, 0x23, 0x31, 0x05, 0x92, 0x99], 8))
    );
    let mut failing = FakeBus {
        fail: &[1],
        ..FakeBus::default()
    };
    assert!(!set(&mut failing, 0x51, &time));
}

#[test]
fn epoch_matches_known_dates() {
    let at = |year, month, day, hour, minute, second| DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        weekday: 0,
    };
    assert_eq!(epoch_from_utc(&at(1970, 1, 1, 0, 0, 0)), 0);
    assert_eq!(epoch_from_utc(&at(2000, 2, 29, 23, 59, 59)), 951_868_799);
    assert_eq!(epoch_from_utc(&at(1999, 12, 31, 0, 0, 0)), 946_598_400);
    assert_eq!(epoch_from_utc(&at(2026, 10, 25, 12, 34, 56)), 1_792_931_696);
    assert_eq!(epoch_from_utc(&at(2099, 1, 1, 0, 0, 1)), 4_070_908_801);
}

#[test]
fn epoch_takes_garbage_fields() {
    // A day of 0 (an unset clock's registers) does not panic.
    let time = DateTime {
        year: 2000,
        month: 3,
        day: 0,
        ..DateTime::default()
    };
    let _ = epoch_from_utc(&time);
}
