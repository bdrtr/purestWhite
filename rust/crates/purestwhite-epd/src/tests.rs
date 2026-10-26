// The fakes index their small arrays directly, and a wrong trace fails the test.
#![allow(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::struct_excessive_bools,
    clippy::collapsible_if
)]

extern crate std;

use super::uc8179::{self, Uc8179};
use super::uc8279x4::{Refresh, Uc8279X4, X4_PRO};
use super::*;
use core::cell::RefCell;
use core::convert::Infallible;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use embedded_hal::digital::ErrorType as PinErrorType;
use embedded_hal_async::spi::{ErrorKind, ErrorType};
use std::vec;
use std::vec::Vec;

/// Runs a future of the fakes below, which are always ready.
fn block_on<F: Future>(future: F) -> F::Output {
    match pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => unreachable!("the fakes are always ready"),
    }
}

/// A UC8279 as the bus sees it: chip-select frames of (dc, byte), the reset line, and a BUSY
/// that stays low for `busy_ms` after power-on, a refresh and power-off.
#[derive(Default)]
struct Panel {
    frames: Vec<Vec<(bool, u8)>>,
    resets: Vec<bool>,
    cs_low: bool,
    dc: bool,
    clock_ms: u32,
    busy_until: u32,
    busy_ms: u32,
    /// A write while BUSY was low, which the controller may drop.
    wrote_while_busy: bool,
    fail_spi: bool,
    /// Fails only writes of more than a byte: the plane bursts.
    fail_bursts: bool,
}

impl Panel {
    fn busy(&self) -> bool {
        self.clock_ms < self.busy_until
    }
    /// The commands, in order.
    fn commands(&self) -> Vec<u8> {
        self.frames
            .iter()
            .filter_map(|f| match f.as_slice() {
                [(false, c), ..] => Some(*c),
                _ => None,
            })
            .collect()
    }
}

type Shared<'a> = &'a RefCell<Panel>;

struct Spi<'a>(Shared<'a>);
struct Cs<'a>(Shared<'a>);
struct Dc<'a>(Shared<'a>);
struct Rst<'a>(Shared<'a>);
struct Busy<'a>(Shared<'a>);
struct Clock<'a>(Shared<'a>);

impl ErrorType for Spi<'_> {
    type Error = ErrorKind;
}

#[allow(clippy::unused_async_trait_impl)]
impl SpiBus for Spi<'_> {
    async fn read(&mut self, _: &mut [u8]) -> Result<(), ErrorKind> {
        Err(ErrorKind::Other)
    }
    async fn write(&mut self, bytes: &[u8]) -> Result<(), ErrorKind> {
        let mut p = self.0.borrow_mut();
        if p.fail_spi || (p.fail_bursts && bytes.len() > 1) {
            return Err(ErrorKind::Other);
        }
        assert!(p.cs_low, "a write outside a frame");
        if p.busy() {
            p.wrote_while_busy = true;
        }
        let dc = p.dc;
        let frame = p.frames.last_mut().expect("a frame");
        frame.extend(bytes.iter().map(|&b| (dc, b)));
        Ok(())
    }
    async fn transfer(&mut self, _: &mut [u8], _: &[u8]) -> Result<(), ErrorKind> {
        Err(ErrorKind::Other)
    }
    async fn transfer_in_place(&mut self, _: &mut [u8]) -> Result<(), ErrorKind> {
        Err(ErrorKind::Other)
    }
    async fn flush(&mut self) -> Result<(), ErrorKind> {
        Ok(())
    }
}

macro_rules! output {
    ($t:ident, |$p:ident, $high:ident| $body:expr) => {
        impl PinErrorType for $t<'_> {
            type Error = Infallible;
        }
        impl OutputPin for $t<'_> {
            fn set_low(&mut self) -> Result<(), Infallible> {
                let $p = &mut *self.0.borrow_mut();
                let $high = false;
                $body;
                Ok(())
            }
            fn set_high(&mut self) -> Result<(), Infallible> {
                let $p = &mut *self.0.borrow_mut();
                let $high = true;
                $body;
                Ok(())
            }
        }
    };
}

output!(Cs, |p, high| {
    if !high && !p.cs_low {
        p.frames.push(Vec::new());
    }
    // A frame ending: the controller acts on the command it carried.
    if high && p.cs_low {
        if let Some([(false, c)]) = p.frames.last().map(Vec::as_slice) {
            if matches!(*c, 0x02 | 0x04 | 0x12) {
                p.busy_until = p.clock_ms + p.busy_ms;
            }
        }
    }
    p.cs_low = !high;
});
output!(Dc, |p, high| p.dc = high);
output!(Rst, |p, high| p.resets.push(high));

impl PinErrorType for Busy<'_> {
    type Error = Infallible;
}

impl InputPin for Busy<'_> {
    fn is_high(&mut self) -> Result<bool, Infallible> {
        Ok(!self.0.borrow().busy())
    }
    fn is_low(&mut self) -> Result<bool, Infallible> {
        Ok(self.0.borrow().busy())
    }
}

impl DelayNs for Clock<'_> {
    #[allow(clippy::unused_async_trait_impl)]
    async fn delay_ns(&mut self, ns: u32) {
        self.0.borrow_mut().clock_ms += ns.div_ceil(1_000_000);
    }
    #[allow(clippy::unused_async_trait_impl)]
    async fn delay_ms(&mut self, ms: u32) {
        self.0.borrow_mut().clock_ms += ms;
    }
}

fn bus(panel: Shared<'_>) -> Bus<Spi<'_>, Cs<'_>, Dc<'_>, Rst<'_>, Busy<'_>, Clock<'_>> {
    Bus {
        spi: Spi(panel),
        cs: Cs(panel),
        dc: Dc(panel),
        rst: Rst(panel),
        busy: Busy(panel),
        delay: Clock(panel),
    }
}

/// A frame of all `byte`.
fn frame(byte: u8) -> Vec<u8> {
    vec![byte; 100 * 480]
}

/// The bytes of the data frame after the frame of `command` at `index`.
fn data_after(panel: &Panel, index: usize) -> Vec<u8> {
    panel.frames[index + 1].iter().map(|&(_, b)| b).collect()
}

#[test]
fn begin_resets_and_programs_the_x4_pro_panel() {
    let panel = RefCell::new(Panel::default());
    let mut uc = Uc8279X4::new(X4_PRO);
    block_on(uc.begin(&mut bus(&panel))).expect("begin");
    let p = panel.borrow();
    assert_eq!(p.resets, vec![true, false, true]);
    assert_eq!(p.clock_ms, 80);
    assert_eq!(p.commands(), vec![0x00, 0x61, 0x65, 0x03, 0x30, 0xE1]);
    // Each data byte in its own frame, with DC high.
    let bytes: Vec<(bool, u8)> = p.frames[1..].iter().flatten().copied().collect();
    assert_eq!(&bytes[..2], &[(true, 0x37), (true, 0x4D)]);
    assert!(p.frames.iter().all(|f| f.len() == 1));
}

#[test]
fn a_first_fast_refresh_is_full_and_planes_pad_the_hidden_gates() {
    let panel = RefCell::new(Panel {
        busy_ms: 7,
        ..Panel::default()
    });
    let mut uc = Uc8279X4::new(X4_PRO);
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    panel.borrow_mut().frames.clear();
    let mut image = frame(0xFF);
    image[0] = 0x0F; // the first visible row's first byte
    image[100 * 479 + 99] = 0xF0; // the last visible row's last byte
    block_on(uc.display(&mut b, &image, Refresh::Fast, true)).expect("display");
    let p = panel.borrow();
    assert!(!p.wrote_while_busy);
    // DTM2, DTM1 (white), CDI, CCSET, TSSET, PON, PSR, DRF, DTM1 (sync), POF: no partial window.
    let commands = p.commands();
    assert_eq!(
        commands,
        vec![0x13, 0x10, 0x50, 0xE0, 0xE5, 0x04, 0x00, 0x12, 0x10, 0x02]
    );
    let dtm2 = data_after(&p, 0);
    assert_eq!(dtm2.len(), 100 * 600);
    assert!(dtm2[..100 * 120].iter().all(|&b| b == 0xFF));
    assert_eq!(dtm2[100 * 120], 0x0F);
    assert_eq!(dtm2[100 * (120 + 479) + 99], 0xF0);
    // 120 + 480 = 600: the visible rows end at the last addressed gate, with no padding after.
    let dtm1 = data_after(&p, 2);
    assert!(dtm1.iter().all(|&b| b == 0xFF));
    // The full refresh's CDI and temperature, and PSR with REG cleared.
    assert_eq!(data_after(&p, 4), vec![0x97]);
    assert_eq!(data_after(&p, 8), vec![0x1E]);
    assert_eq!(p.frames[11], vec![(false, 0x00)]);
    assert_eq!(data_after(&p, 11), vec![0x17]);
}

#[test]
fn a_later_fast_refresh_is_partial_in_the_visible_window() {
    let panel = RefCell::new(Panel {
        busy_ms: 3,
        ..Panel::default()
    });
    let mut uc = Uc8279X4::new(X4_PRO);
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    block_on(uc.display(&mut b, &frame(0xFF), Refresh::Full, false)).expect("full");
    panel.borrow_mut().frames.clear();
    block_on(uc.display(&mut b, &frame(0x00), Refresh::Fast, false)).expect("fast");
    let p = panel.borrow();
    assert!(!p.wrote_while_busy);
    // Still powered: no PON; the window, then PTOUT after the refresh, no POF.
    assert_eq!(
        p.commands(),
        vec![
            0x13, 0x50, 0xE0, 0xE5, 0x03, 0xE1, 0x91, 0x90, 0x00, 0x12, 0x92, 0x10
        ]
    );
    let window_at = p
        .frames
        .iter()
        .position(|f| f == &vec![(false, 0x90)])
        .expect("PTL");
    let window: Vec<u8> = p.frames[window_at + 1..window_at + 10]
        .iter()
        .map(|f| f[0].1)
        .collect();
    assert_eq!(window, vec![0, 0, 0x03, 0x1F, 0, 120, 0x02, 0x57, 0x01]);
}

#[test]
fn half_seeds_the_old_plane_with_the_complement() {
    let panel = RefCell::new(Panel::default());
    let mut uc = Uc8279X4::new(X4_PRO);
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    panel.borrow_mut().frames.clear();
    block_on(uc.display(&mut b, &frame(0x3C), Refresh::Half, false)).expect("half");
    let p = panel.borrow();
    let dtm1 = data_after(&p, 2);
    assert!(dtm1[..100 * 120].iter().all(|&b| b == 0xFF));
    assert!(dtm1[100 * 120..100 * 600].iter().all(|&b| b == 0xC3));
}

#[test]
fn deep_sleep_powers_off_first_when_on() {
    let panel = RefCell::new(Panel::default());
    let mut uc = Uc8279X4::new(X4_PRO);
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    block_on(uc.display(&mut b, &frame(0xFF), Refresh::Full, false)).expect("full");
    panel.borrow_mut().frames.clear();
    block_on(uc.deep_sleep(&mut b)).expect("sleep");
    let p = panel.borrow();
    assert_eq!(p.commands(), vec![0x02, 0x07]);
    assert_eq!(data_after(&p, 1), vec![0xA5]);
}

#[test]
fn a_short_frame_and_a_failed_transfer_are_errors() {
    let panel = RefCell::new(Panel::default());
    let mut uc = Uc8279X4::new(X4_PRO);
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    assert_eq!(
        block_on(uc.display(&mut b, &[0xFF; 10], Refresh::Full, false)),
        Err(BusError)
    );
    panel.borrow_mut().fail_spi = true;
    assert_eq!(
        block_on(uc.display(&mut b, &frame(0xFF), Refresh::Full, false)),
        Err(BusError)
    );
    assert!(!panel.borrow().cs_low);
}

#[test]
fn a_failed_burst_ends_its_frame_and_the_next_fast_refresh_flashes() {
    let panel = RefCell::new(Panel::default());
    let mut uc = Uc8279X4::new(X4_PRO);
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    block_on(uc.display(&mut b, &frame(0xFF), Refresh::Full, false)).expect("full");
    panel.borrow_mut().fail_bursts = true;
    assert_eq!(
        block_on(uc.display(&mut b, &frame(0x00), Refresh::Full, false)),
        Err(BusError)
    );
    assert!(!panel.borrow().cs_low);
    // DTM1 may hold anything now: the next Fast flashes from white instead of diffing.
    let mut p = panel.borrow_mut();
    p.fail_bursts = false;
    p.frames.clear();
    drop(p);
    block_on(uc.display(&mut b, &frame(0x00), Refresh::Fast, false)).expect("fast");
    assert!(!panel.borrow().commands().contains(&0x91));
}

#[test]
fn begin_while_powered_powers_on_again_at_the_next_refresh() {
    let panel = RefCell::new(Panel::default());
    let mut uc = Uc8279X4::new(X4_PRO);
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    block_on(uc.display(&mut b, &frame(0xFF), Refresh::Full, false)).expect("full");
    block_on(uc.begin(&mut b)).expect("begin again");
    panel.borrow_mut().frames.clear();
    block_on(uc.display(&mut b, &frame(0xFF), Refresh::Full, false)).expect("full again");
    assert!(panel.borrow().commands().contains(&0x04));
}

#[test]
fn skip_cancels_a_resync_but_not_the_first_flash() {
    let panel = RefCell::new(Panel::default());
    let mut uc = Uc8279X4::new(X4_PRO);
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    uc.skip_initial_resync();
    block_on(uc.display(&mut b, &frame(0xFF), Refresh::Fast, false)).expect("first");
    assert!(!panel.borrow().commands().contains(&0x91));
    uc.request_resync();
    uc.skip_initial_resync();
    panel.borrow_mut().frames.clear();
    block_on(uc.display(&mut b, &frame(0x00), Refresh::Fast, false)).expect("second");
    assert!(panel.borrow().commands().contains(&0x91));
    uc.request_resync();
    panel.borrow_mut().frames.clear();
    block_on(uc.display(&mut b, &frame(0xFF), Refresh::Fast, false)).expect("third");
    assert!(!panel.borrow().commands().contains(&0x91));
}

#[test]
fn uc8179_begin_ends_with_power_save_only_when_set() {
    for (power_save, last) in [(0x22, 0xE3), (0, 0xE1)] {
        let panel = RefCell::new(Panel::default());
        let mut uc = Uc8179::new(uc8179::Config {
            power_save,
            ..uc8179::X4_PRO
        });
        block_on(uc.begin(&mut bus(&panel))).expect("begin");
        let p = panel.borrow();
        assert_eq!(
            p.commands(),
            [0x00, 0x61, 0x65, 0x03, 0x06]
                .into_iter()
                .chain([0xE1])
                .chain((power_save != 0).then_some(0xE3))
                .collect::<Vec<u8>>()
        );
        assert_eq!(p.commands().last(), Some(&last));
    }
}

#[test]
fn uc8179_fast_refresh_has_no_window_and_restores_the_idle_cdi() {
    let panel = RefCell::new(Panel {
        busy_ms: 3,
        ..Panel::default()
    });
    let mut uc = Uc8179::new(uc8179::X4_PRO);
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    block_on(uc.display(&mut b, &frame(0xFF), Refresh::Full, false)).expect("full");
    panel.borrow_mut().frames.clear();
    let mut image = frame(0x00);
    image[0] = 0x0F; // the first row's first byte: sent last of the visible rows
    block_on(uc.display(&mut b, &image, Refresh::Fast, false)).expect("fast");
    let p = panel.borrow();
    assert!(!p.wrote_while_busy);
    // DTM2, CDI, CCSET, TSSET, PSR, PFS, E1, PTIN, DRF, PTOUT, CDI (idle), DTM1.
    assert_eq!(
        p.commands(),
        vec![
            0x13, 0x50, 0xE0, 0xE5, 0x00, 0x03, 0xE1, 0x91, 0x12, 0x92, 0x50, 0x10
        ]
    );
    let dtm2 = data_after(&p, 0);
    assert_eq!(dtm2.len(), 100 * 600);
    assert_eq!(dtm2[100 * 479], 0x0F);
    assert!(dtm2[100 * 480..].iter().all(|&b| b == 0xFF));
    let cdi: Vec<Vec<u8>> = p
        .frames
        .iter()
        .enumerate()
        .filter(|(_, f)| f.as_slice() == [(false, 0x50)])
        .map(|(i, _)| p.frames[i + 1..i + 3].iter().map(|f| f[0].1).collect())
        .collect();
    assert_eq!(cdi, vec![vec![0x29, 0x07], vec![0xA9, 0x07]]);
}

#[test]
fn geometry_outside_the_plane_is_refused() {
    let panel = RefCell::new(Panel::default());
    let mut wide = Uc8179::new(uc8179::Config {
        width: 1100,
        ..uc8179::X4_PRO
    });
    let mut b = bus(&panel);
    assert_eq!(
        block_on(wide.display(&mut b, &vec![0xFF; 1100 / 8 * 480], Refresh::Full, false)),
        Err(BusError)
    );
    let mut short = Uc8279X4::new(super::uc8279x4::Config {
        tres_height: 500,
        ..X4_PRO
    });
    assert_eq!(
        block_on(short.display(&mut b, &frame(0xFF), Refresh::Full, false)),
        Err(BusError)
    );
    // And the other check of each: the UC8179's visible rows past its gates, the UC8279's rows
    // wider than the plane buffer.
    let mut tall = Uc8179::new(uc8179::Config {
        tres_height: 400,
        ..uc8179::X4_PRO
    });
    assert_eq!(
        block_on(tall.display(&mut b, &frame(0xFF), Refresh::Full, false)),
        Err(BusError)
    );
    let mut wide = Uc8279X4::new(super::uc8279x4::Config {
        width: 1100,
        ..X4_PRO
    });
    assert_eq!(
        block_on(wide.display(&mut b, &vec![0xFF; 1100 / 8 * 480], Refresh::Full, false)),
        Err(BusError)
    );
    assert!(panel.borrow().frames.is_empty());
}

#[test]
fn a_frame_carries_its_command_and_data_and_always_ends() {
    let panel = RefCell::new(Panel::default());
    let mut b = bus(&panel);
    block_on(b.frame(0x90, &[1, 2])).expect("frame");
    block_on(b.frame(0x50, &[])).expect("no data");
    panel.borrow_mut().fail_bursts = true;
    assert_eq!(block_on(b.frame(0x50, &[1, 2])), Err(BusError));
    let p = panel.borrow();
    assert_eq!(p.frames[0], vec![(false, 0x90), (true, 1), (true, 2)]);
    assert_eq!(p.frames[1], vec![(false, 0x50)]);
    assert_eq!(p.frames[2], vec![(false, 0x50)]);
    assert!(!p.cs_low);
}

/// Runs an Overlay page on a UC8179 with `base` and returns the DTM1 and DTM2 planes the gray
/// refresh got.
fn overlay_planes(
    base: Option<&mut [u8]>,
    bw: &[u8],
    lsb: &[u8],
    msb: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    let panel = RefCell::new(Panel::default());
    let mut uc = match base {
        Some(base) => Uc8179::with_base(uc8179::X4_PRO, base).expect("a frame long"),
        None => Uc8179::new(uc8179::X4_PRO),
    };
    let mut b = bus(&panel);
    block_on(uc.begin(&mut b)).expect("begin");
    block_on(uc.begin_grayscale(&mut b, bw, uc8179::GrayMode::Overlay, Refresh::Full, false))
        .expect("base");
    panel.borrow_mut().frames.clear();
    block_on(uc.copy_grayscale_lsb(&mut b, lsb)).expect("lsb");
    block_on(uc.copy_grayscale_msb(&mut b, msb)).expect("msb");
    let p = panel.borrow();
    (data_after(&p, 0), data_after(&p, 2))
}

#[test]
fn overlay_masks_become_absolute_planes_with_a_base() {
    // One byte of each: white, light, dark, black pixels in pairs.
    let bw = vec![0b1100_0000; 100 * 480];
    let lsb = vec![0b0000_1100; 100 * 480];
    let msb = vec![0b0011_1100; 100 * 480];
    let mut base = vec![0xA5; 100 * 480];
    let (dtm1, dtm2) = overlay_planes(Some(&mut base), &bw, &lsb, &msb);
    // (DTM1, DTM2): white (1,1), light (0,1), dark (1,0), black (0,0).
    assert_eq!(dtm1[0], 0b1100_1100);
    assert_eq!(dtm2[0], 0b1111_0000);
    // The base is recovered for the planes after the refresh.
    assert_eq!(base[0], 0b1100_0000);
    // Without a base the masks go as they are; a short one is refused.
    let (dtm1, dtm2) = overlay_planes(None, &bw, &lsb, &msb);
    assert_eq!((dtm1[0], dtm2[0]), (lsb[0], msb[0]));
    let mut short = vec![0; 100 * 480 - 1];
    assert!(Uc8179::with_base(uc8179::X4_PRO, short.as_mut_slice()).is_none());
}
