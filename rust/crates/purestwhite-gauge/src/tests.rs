// The fake chip indexes its 256 registers by a u8.
#![allow(clippy::indexing_slicing)]

use super::*;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use embedded_hal_async::i2c::{ErrorKind, ErrorType, Operation};

/// Runs a future of the fakes below, which are always ready.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

/// The time paused.
#[derive(Default)]
struct Clock {
    paused_ms: u32,
}

// The fakes are ready at once.
impl DelayNs for Clock {
    #[allow(clippy::unused_async_trait_impl)]
    async fn delay_ns(&mut self, ns: u32) {
        self.paused_ms += ns / 1_000_000;
    }
    #[allow(clippy::unused_async_trait_impl)]
    async fn delay_ms(&mut self, ms: u32) {
        self.paused_ms += ms;
    }
}

/// A CW2017's registers. A reset leaves VERSION at 0xA0 for `starting` reads.
struct Chip {
    regs: [u8; 256],
    starting: u32,
    fail_all: bool,
    writes: u32,
}

impl Chip {
    /// Running, with our profile and `soc` percent.
    fn ready(soc: u8) -> Self {
        let mut regs = [0; 256];
        regs[usize::from(VERSION)] = 0x0D;
        regs[usize::from(SOC)] = soc;
        regs[usize::from(SOC_ALERT)] = UPDATE_FLAG;
        regs[0x10..0x60].copy_from_slice(&PROFILE);
        Chip {
            regs,
            starting: 0,
            fail_all: false,
            writes: 0,
        }
    }
}

impl ErrorType for Chip {
    type Error = ErrorKind;
}

impl I2c for Chip {
    #[allow(clippy::unused_async_trait_impl)]
    async fn transaction(&mut self, _addr: u8, ops: &mut [Operation<'_>]) -> Result<(), ErrorKind> {
        let ok = match ops {
            [Operation::Write(bytes)] => self.write_bytes(bytes),
            [Operation::Write(out), Operation::Read(buf)] => self.write_read_bytes(out, buf),
            _ => false,
        };
        if ok { Ok(()) } else { Err(ErrorKind::Other) }
    }
}

impl Chip {
    fn write_bytes(&mut self, bytes: &[u8]) -> bool {
        let &[reg, value] = bytes else {
            return false;
        };
        if self.fail_all {
            return false;
        }
        self.writes += 1;
        self.regs[usize::from(reg)] = value;
        if reg == MODE && value == MODE_NORMAL {
            self.starting = 3;
        }
        true
    }

    fn write_read_bytes(&mut self, out: &[u8], buf: &mut [u8]) -> bool {
        let (&[reg], [value]) = (out, buf) else {
            return false;
        };
        if self.fail_all {
            return false;
        }
        *value = if reg == VERSION && self.starting > 0 {
            self.starting -= 1;
            0xA0
        } else {
            self.regs[usize::from(reg)]
        };
        true
    }
}

#[test]
fn a_ready_gauge_is_read_without_writes() {
    let mut chip = Chip::ready(77);
    let mut gauge = Cw2017::new();
    let mut clock = Clock::default();
    assert_eq!(
        block_on(gauge.soc(&mut chip, &mut clock, 0x63, 5000)),
        Some(77)
    );
    assert_eq!(
        block_on(gauge.soc(&mut chip, &mut clock, 0x63, 6000)),
        Some(77)
    );
    assert_eq!(chip.writes, 0);
    assert_eq!(clock.paused_ms, 0);
}

#[test]
fn a_missing_profile_is_uploaded_and_the_gauge_restarted() {
    let mut chip = Chip::ready(50);
    chip.regs[usize::from(SOC_ALERT)] = 0;
    chip.regs[0x20] ^= 1;
    let mut gauge = Cw2017::new();
    let mut clock = Clock::default();
    assert_eq!(
        block_on(gauge.soc(&mut chip, &mut clock, 0x63, 5000)),
        Some(50)
    );
    // The profile, the flag and the three reset steps.
    assert_eq!(chip.writes, 80 + 1 + 3);
    assert_eq!(&chip.regs[0x10..0x60], &PROFILE[..]);
    // 20 ms after the flag, 3 × 20 for the reset, 3 × 20 while VERSION still says starting.
    assert_eq!(clock.paused_ms, 20 + 60 + 60);
}

#[test]
fn a_sleeping_gauge_is_restarted() {
    let mut chip = Chip::ready(40);
    chip.regs[usize::from(MODE)] = MODE_DEFAULT;
    let mut gauge = Cw2017::new();
    let mut clock = Clock::default();
    assert_eq!(
        block_on(gauge.soc(&mut chip, &mut clock, 0x63, 5000)),
        Some(40)
    );
    assert_eq!(chip.writes, 3);
}

#[test]
fn a_failed_first_reading_waits_a_second() {
    let mut chip = Chip::ready(30);
    chip.fail_all = true;
    let mut gauge = Cw2017::new();
    let mut clock = Clock::default();
    assert_eq!(block_on(gauge.soc(&mut chip, &mut clock, 0x63, 5000)), None);
    chip.fail_all = false;
    assert_eq!(block_on(gauge.soc(&mut chip, &mut clock, 0x63, 5999)), None);
    assert_eq!(
        block_on(gauge.soc(&mut chip, &mut clock, 0x63, 6000)),
        Some(30)
    );
}

#[test]
fn a_gauge_found_asleep_later_is_brought_back() {
    let mut chip = Chip::ready(60);
    let mut gauge = Cw2017::new();
    let mut clock = Clock::default();
    assert_eq!(
        block_on(gauge.soc(&mut chip, &mut clock, 0x63, 5000)),
        Some(60)
    );
    chip.regs[usize::from(MODE)] = MODE_DEFAULT;
    assert_eq!(block_on(gauge.soc(&mut chip, &mut clock, 0x63, 5100)), None);
    // The next reading, a second later, restarts it.
    assert_eq!(
        block_on(gauge.soc(&mut chip, &mut clock, 0x63, 7000)),
        Some(60)
    );
    assert_eq!(chip.regs[usize::from(MODE)], MODE_NORMAL);
}

#[test]
fn a_bad_state_of_charge_gets_one_restart() {
    let mut chip = Chip::ready(150);
    let mut gauge = Cw2017::new();
    let mut clock = Clock::default();
    // Restarted, but the state of charge stays bad: 30 tries 100 ms apart.
    assert_eq!(block_on(gauge.soc(&mut chip, &mut clock, 0x63, 5000)), None);
    assert_eq!(chip.writes, 3);
    assert_eq!(clock.paused_ms, 60 + 60 + 3000);
}

#[test]
fn the_retry_window_survives_a_clock_wrap() {
    let mut chip = Chip::ready(20);
    chip.fail_all = true;
    let mut gauge = Cw2017::new();
    let mut clock = Clock::default();
    assert_eq!(
        block_on(gauge.soc(&mut chip, &mut clock, 0x63, u32::MAX - 100)),
        None
    );
    chip.fail_all = false;
    assert_eq!(block_on(gauge.soc(&mut chip, &mut clock, 0x63, 500)), None);
    assert_eq!(
        block_on(gauge.soc(&mut chip, &mut clock, 0x63, 900)),
        Some(20)
    );
}
