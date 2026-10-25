// The fakes index their small arrays directly, and a wrong reading fails the test.
#![allow(clippy::indexing_slicing, clippy::panic)]

extern crate std;

use super::*;
use core::cell::RefCell;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use embedded_hal_async::i2c::{ErrorKind, ErrorType, Operation};
use std::vec;
use std::vec::Vec;

/// One list for the bus, the pins and the clock, so their order shows.
type Log = RefCell<Vec<Event>>;

/// Runs a future of the fakes below, which are always ready.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => unreachable!("the fakes are always ready"),
    }
}

/// Everything the driver did, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Reset(bool),
    Int(bool),
    ReleaseInt,
    Pause(u32),
    Write(u8, Vec<u8>),
    WriteRead(u8, Vec<u8>, usize),
}

/// A GT911 answering at `addr`, with a status byte and contact records.
#[derive(Default)]
struct Chip<'a> {
    addr: u8,
    status: u8,
    points: Vec<u8>,
    fail_status: bool,
    fail_points: bool,
    log: Option<&'a Log>,
    events: Vec<Event>,
}

impl Chip<'_> {
    fn push(&mut self, event: Event) {
        match self.log {
            Some(log) => log.borrow_mut().push(event),
            None => self.events.push(event),
        }
    }
}

impl ErrorType for Chip<'_> {
    type Error = ErrorKind;
}

impl I2c for Chip<'_> {
    #[allow(clippy::unused_async_trait_impl)]
    async fn transaction(&mut self, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), ErrorKind> {
        let ok = match ops {
            [Operation::Write(bytes)] => {
                self.push(Event::Write(addr, bytes.to_vec()));
                addr == self.addr
            }
            [Operation::Write(out), Operation::Read(buf)] => {
                self.push(Event::WriteRead(addr, out.to_vec(), buf.len()));
                match *out {
                    [0x81, 0x4E] if !self.fail_status && addr == self.addr => {
                        buf.fill(self.status);
                        true
                    }
                    [0x81, 0x50] if !self.fail_points && addr == self.addr => {
                        for (i, b) in buf.iter_mut().enumerate() {
                            *b = self.points.get(i).copied().unwrap_or(0);
                        }
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if ok { Ok(()) } else { Err(ErrorKind::Other) }
    }
}

/// The pins and the clock, logging into a shared list.
struct Pins<'a>(&'a Log);

impl ResetPins for Pins<'_> {
    fn set_reset(&mut self, high: bool) {
        self.0.borrow_mut().push(Event::Reset(high));
    }
    fn drive_int(&mut self, high: bool) {
        self.0.borrow_mut().push(Event::Int(high));
    }
    fn release_int(&mut self) {
        self.0.borrow_mut().push(Event::ReleaseInt);
    }
}

struct Clock<'a>(&'a Log);

impl DelayNs for Clock<'_> {
    #[allow(clippy::unused_async_trait_impl)]
    async fn delay_ns(&mut self, ns: u32) {
        self.0
            .borrow_mut()
            .push(Event::Pause(ns.div_ceil(1_000_000)));
    }
    #[allow(clippy::unused_async_trait_impl)]
    async fn delay_ms(&mut self, ms: u32) {
        self.0.borrow_mut().push(Event::Pause(ms));
    }
}

/// The X4 Pro's wiring (freeink-sdk's `BoardConfig`).
fn x4pro() -> Config {
    Config {
        addr: 0x5D,
        alt_addr: 0x14,
        raw_min_x: 0,
        raw_max_x: 799,
        raw_min_y: 0,
        raw_max_y: 479,
        swap_xy: true,
        flip_x: false,
        flip_y: true,
        coords_at_byte0: true,
    }
}

fn reset_events(int_high: bool) -> Vec<Event> {
    vec![
        Event::Reset(false),
        Event::Int(int_high),
        Event::Pause(10),
        Event::Reset(true),
        Event::Pause(10),
        Event::Int(int_high),
        Event::Pause(50),
        Event::ReleaseInt,
        Event::Pause(50),
    ]
}

/// Runs `begin` with pins on a chip at `addr`; what it found, and everything in order.
fn begin_with_pins(addr: u8, config: &Config) -> (Option<u8>, Vec<Event>) {
    let log = Log::default();
    let mut chip = Chip {
        addr,
        log: Some(&log),
        ..Chip::default()
    };
    let found = block_on(begin(
        &mut chip,
        &mut Clock(&log),
        Some(&mut Pins(&log)),
        config,
    ));
    (found, log.take())
}

#[test]
fn begin_finds_the_primary_address_after_one_reset() {
    let (found, events) = begin_with_pins(0x5D, &x4pro());
    assert_eq!(found, Some(0x5D));
    let mut expected = reset_events(false);
    expected.push(Event::Write(0x5D, vec![]));
    assert_eq!(events, expected);
}

#[test]
fn begin_finds_the_alternate_address_after_one_reset() {
    let (found, events) = begin_with_pins(0x14, &x4pro());
    assert_eq!(found, Some(0x14));
    let mut expected = reset_events(false);
    expected.push(Event::Write(0x5D, vec![]));
    expected.push(Event::Write(0x14, vec![]));
    assert_eq!(events, expected);
}

#[test]
fn begin_resets_again_with_int_high_then_gives_up() {
    let (found, events) = begin_with_pins(0x33, &x4pro());
    assert_eq!(found, None);
    let mut expected = reset_events(false);
    expected.push(Event::Write(0x5D, vec![]));
    expected.push(Event::Write(0x14, vec![]));
    expected.extend(reset_events(true));
    expected.push(Event::Write(0x5D, vec![]));
    expected.push(Event::Write(0x14, vec![]));
    assert_eq!(events, expected);
}

#[test]
fn begin_without_pins_probes_twice_without_pausing() {
    let mut chip = Chip {
        addr: 0x33,
        ..Chip::default()
    };
    let log = Log::default();
    let config = Config {
        alt_addr: 0,
        ..x4pro()
    };
    assert_eq!(
        block_on(begin(
            &mut chip,
            &mut Clock(&log),
            None::<&mut NoPins>,
            &config
        )),
        None
    );
    assert!(log.borrow().is_empty());
    assert_eq!(
        chip.events,
        vec![Event::Write(0x5D, vec![]), Event::Write(0x5D, vec![])]
    );
}

const CLEAR: [u8; 3] = [0x81, 0x4E, 0];

fn status_read(addr: u8) -> Event {
    Event::WriteRead(addr, vec![0x81, 0x4E], 1)
}

#[test]
fn a_failed_status_read_keeps_the_frame() {
    let mut chip = Chip {
        addr: 0x5D,
        fail_status: true,
        ..Chip::default()
    };
    assert_eq!(block_on(read(&mut chip, 0x5D, &x4pro())), Reading::Failed);
    assert_eq!(chip.events, vec![status_read(0x5D)]);
}

#[test]
fn no_new_frame_is_not_cleared() {
    let mut chip = Chip {
        addr: 0x5D,
        status: 0x7F,
        ..Chip::default()
    };
    assert_eq!(block_on(read(&mut chip, 0x5D, &x4pro())), Reading::NotReady);
    assert_eq!(chip.events, vec![status_read(0x5D)]);
}

#[test]
fn a_frame_without_contacts_is_a_release() {
    let mut chip = Chip {
        addr: 0x5D,
        status: 0x90,
        ..Chip::default()
    };
    assert_eq!(
        block_on(read(&mut chip, 0x5D, &x4pro())),
        Reading::Frame(Frame {
            home_key: true,
            ..Frame::default()
        })
    );
    assert_eq!(
        chip.events,
        vec![status_read(0x5D), Event::Write(0x5D, CLEAR.to_vec())]
    );
}

/// A record with coordinates from byte `at`.
fn record(at: usize, id: u8, x: u16, y: u16) -> Vec<u8> {
    let mut r = vec![0u8; RECORD];
    if at == 1 {
        r[0] = id;
    }
    r[at..at + 2].copy_from_slice(&x.to_le_bytes());
    r[at + 2..at + 4].copy_from_slice(&y.to_le_bytes());
    r
}

#[test]
fn contacts_swap_and_map_as_on_the_x4_pro() {
    // Mounted portrait: the digitizer's x (0..480) is the panel's y, flipped.
    let mut points = record(0, 0, 100, 700);
    points.extend(record(0, 0, 479, 0));
    let mut chip = Chip {
        addr: 0x5D,
        status: 0x82,
        points,
        ..Chip::default()
    };
    let Reading::Frame(frame) = block_on(read(&mut chip, 0x5D, &x4pro())) else {
        panic!("a frame");
    };
    assert_eq!((frame.home_key, frame.reported, frame.count), (false, 2, 2));
    assert_eq!(
        frame.contacts[0],
        Contact {
            id: 0,
            x: 700,
            y: 379
        }
    );
    assert_eq!(frame.contacts[1], Contact { id: 1, x: 0, y: 0 });
    assert_eq!(
        chip.events,
        vec![
            status_read(0x5D),
            Event::WriteRead(0x5D, vec![0x81, 0x50], 16),
            Event::Write(0x5D, CLEAR.to_vec()),
        ]
    );
}

#[test]
fn ids_flips_and_ranges() {
    let config = Config {
        addr: 0x5D,
        alt_addr: 0,
        raw_min_x: 100,
        raw_max_x: 1100,
        raw_min_y: 50,
        raw_max_y: 2050,
        swap_xy: false,
        flip_x: true,
        flip_y: true,
        coords_at_byte0: false,
    };
    assert!(config.ids_stable());
    let mut points = record(1, 0x37, 600, 1050);
    points.extend(record(1, 0x02, 50, 3000));
    let mut chip = Chip {
        addr: 0x5D,
        status: 0x82,
        points,
        ..Chip::default()
    };
    let Reading::Frame(frame) = block_on(read(&mut chip, 0x5D, &config)) else {
        panic!("a frame");
    };
    // The id is the low nibble; below the range maps to 0 and above to the span, then flipped.
    assert_eq!(
        frame.contacts[..2],
        [
            Contact {
                id: 7,
                x: 500,
                y: 1000
            },
            Contact {
                id: 2,
                x: 1000,
                y: 0
            },
        ]
    );
}

#[test]
fn more_contacts_than_kept() {
    let mut points = Vec::new();
    for i in 0..8 {
        points.extend(record(0, 0, i * 10, i * 20));
    }
    let mut chip = Chip {
        addr: 0x5D,
        status: 0x8F,
        points,
        ..Chip::default()
    };
    let Reading::Frame(frame) = block_on(read(&mut chip, 0x5D, &x4pro())) else {
        panic!("a frame");
    };
    assert_eq!((frame.reported, frame.count), (15, 4));
    assert_eq!(
        chip.events[1],
        Event::WriteRead(0x5D, vec![0x81, 0x50], MAX_CONTACTS * RECORD)
    );
}

#[test]
fn failed_contacts_still_clear_and_report_the_home_key() {
    let mut chip = Chip {
        addr: 0x5D,
        status: 0x91,
        fail_points: true,
        ..Chip::default()
    };
    assert_eq!(
        block_on(read(&mut chip, 0x5D, &x4pro())),
        Reading::ContactsFailed { home_key: true }
    );
    assert_eq!(
        chip.events.last(),
        Some(&Event::Write(0x5D, CLEAR.to_vec()))
    );
}

#[test]
fn axis_mapping_matches_the_sdk_integer_math() {
    assert_eq!(map_axis(0, 0, 799, 799), 0);
    assert_eq!(map_axis(799, 0, 799, 799), 799);
    assert_eq!(map_axis(400, 0, 799, 799), 400);
    assert_eq!(map_axis(5, 10, 20, 10), 0);
    assert_eq!(map_axis(15, 10, 20, 10), 5);
    assert_eq!(map_axis(17, 10, 20, 3), 2); // 7 * 3 / 10, truncated
    assert_eq!(map_axis(u16::MAX - 1, 0, u16::MAX, u16::MAX), u16::MAX - 1);
}

#[test]
fn degenerate_ranges_map_as_the_sdk() {
    // raw_max below raw_min: the span wraps, and raw at raw_min still maps to 0.
    let span = 50u16.wrapping_sub(100);
    assert_eq!(map_axis(100, 100, 50, span), 0);
    assert_eq!(map_axis(101, 100, 50, span), span);
    assert_eq!(map_axis(7, 100, 50, span), 0);
    // An empty range: 0 below and at it, the span (0) above.
    assert_eq!(map_axis(30, 30, 30, 0), 0);
    assert_eq!(map_axis(31, 30, 30, 0), 0);
}

#[test]
fn no_controller_reads_nothing() {
    let mut chip = Chip {
        addr: 0x5D,
        status: 0x81,
        ..Chip::default()
    };
    assert_eq!(block_on(read(&mut chip, 0, &x4pro())), Reading::Failed);
    assert!(chip.events.is_empty());
}
