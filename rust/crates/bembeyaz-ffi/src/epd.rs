//! Exports for test/epd_panels and test/epd_probe (see `rust/include/purestwhite/epd.h`).

use crate::i2c::block_on;
use core::convert::Infallible;
use core::ffi::c_void;
use core::mem::MaybeUninit;
use embedded_hal::digital::{ErrorType as PinErrorType, InputPin, OutputPin};
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::spi::{ErrorKind, ErrorType, SpiBus};
use purestwhite_epd::probe::{self, Controller, Line as ProbeLine, Mode, ProbeLines, Verdict};
use purestwhite_epd::uc8179::{Uc8179, X4_PRO as UC8179_X4_PRO};
use purestwhite_epd::uc8279x4::{Refresh, Uc8279X4, X4_PRO};
use purestwhite_epd::{Bus, BusError};

/// `pw_epd_bus`.
#[repr(C)]
pub struct CBus {
    context: *mut c_void,
    write: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> bool>,
    set_cs: Option<unsafe extern "C" fn(*mut c_void, bool)>,
    set_dc: Option<unsafe extern "C" fn(*mut c_void, bool)>,
    set_rst: Option<unsafe extern "C" fn(*mut c_void, bool)>,
    busy_high: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    pause_ms: Option<unsafe extern "C" fn(*mut c_void, u32)>,
}

/// `pw_uc8279x4` and `pw_uc8179`: room for a `Uc8279X4` or a `Uc8179`, each in its own struct type.
#[repr(C, align(4))]
pub struct CPanel([u32; 8]);

// As epd.h's.
const _: () = assert!(size_of::<CBus>() == 7 * size_of::<usize>());

/// A `pw_epd_bus` whose callbacks may be called with its context during the call.
#[derive(Clone, Copy)]
struct Lines<'a>(&'a CBus);

struct Spi<'a>(Lines<'a>);
struct Line<'a> {
    lines: Lines<'a>,
    set: Option<unsafe extern "C" fn(*mut c_void, bool)>,
}
struct BusyLine<'a>(Lines<'a>);
struct Pause<'a>(Lines<'a>);

impl ErrorType for Spi<'_> {
    type Error = ErrorKind;
}

#[allow(
    clippy::unused_async,
    reason = "the C++ calls block; the futures are ready at once"
)]
impl SpiBus for Spi<'_> {
    async fn read(&mut self, _: &mut [u8]) -> Result<(), ErrorKind> {
        Err(ErrorKind::Other)
    }
    async fn write(&mut self, bytes: &[u8]) -> Result<(), ErrorKind> {
        let c = self.0.0;
        // SAFETY: `bytes` is valid for the call, as the callback expects.
        if c.write
            .is_some_and(|f| unsafe { f(c.context, bytes.as_ptr(), bytes.len()) })
        {
            Ok(())
        } else {
            Err(ErrorKind::Other)
        }
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

impl PinErrorType for Line<'_> {
    type Error = Infallible;
}

impl Line<'_> {
    fn drive(&mut self, high: bool) {
        if let Some(f) = self.set {
            // SAFETY: the callback takes its context.
            unsafe { f(self.lines.0.context, high) }
        }
    }
}

impl OutputPin for Line<'_> {
    fn set_low(&mut self) -> Result<(), Infallible> {
        self.drive(false);
        Ok(())
    }
    fn set_high(&mut self) -> Result<(), Infallible> {
        self.drive(true);
        Ok(())
    }
}

impl PinErrorType for BusyLine<'_> {
    type Error = Infallible;
}

impl InputPin for BusyLine<'_> {
    fn is_high(&mut self) -> Result<bool, Infallible> {
        let c = self.0.0;
        // SAFETY: the callback takes its context. A bus without one reads idle.
        Ok(c.busy_high.is_none_or(|f| unsafe { f(c.context) }))
    }
    fn is_low(&mut self) -> Result<bool, Infallible> {
        self.is_high().map(|high| !high)
    }
}

#[allow(
    clippy::unused_async,
    reason = "the C++ calls block; the futures are ready at once"
)]
impl DelayNs for Pause<'_> {
    async fn delay_ns(&mut self, ns: u32) {
        self.delay_ms(ns.div_ceil(1_000_000)).await;
    }
    async fn delay_ms(&mut self, ms: u32) {
        let c = self.0.0;
        if let Some(f) = c.pause_ms {
            // SAFETY: the callback takes its context.
            unsafe { f(c.context, ms) }
        }
    }
}

type CBusLines<'a> = Bus<Spi<'a>, Line<'a>, Line<'a>, Line<'a>, BusyLine<'a>, Pause<'a>>;

/// Whether every callback is there; a bus missing one is refused before any traffic.
fn complete(bus: &CBus) -> bool {
    bus.write.is_some()
        && bus.set_cs.is_some()
        && bus.set_dc.is_some()
        && bus.set_rst.is_some()
        && bus.busy_high.is_some()
        && bus.pause_ms.is_some()
}

fn lines(bus: &CBus) -> CBusLines<'_> {
    let l = Lines(bus);
    Bus {
        spi: Spi(l),
        cs: Line {
            lines: l,
            set: bus.set_cs,
        },
        dc: Line {
            lines: l,
            set: bus.set_dc,
        },
        rst: Line {
            lines: l,
            set: bus.set_rst,
        },
        busy: BusyLine(l),
        delay: Pause(l),
    }
}

/// The panel drivers behind `pw_uc8279x4` and `pw_uc8179`.
trait Driver: Sized {
    fn x4_pro() -> Self;
    async fn begin(&mut self, bus: &mut CBusLines<'_>) -> Result<(), BusError>;
    async fn display(
        &mut self,
        bus: &mut CBusLines<'_>,
        frame: &[u8],
        refresh: Refresh,
        power_off: bool,
    ) -> Result<(), BusError>;
    async fn deep_sleep(&mut self, bus: &mut CBusLines<'_>) -> Result<(), BusError>;
}

macro_rules! driver {
    ($t:ty, $config:expr) => {
        impl Driver for $t {
            fn x4_pro() -> Self {
                <$t>::new($config)
            }
            async fn begin(&mut self, bus: &mut CBusLines<'_>) -> Result<(), BusError> {
                <$t>::begin(self, bus).await
            }
            async fn display(
                &mut self,
                bus: &mut CBusLines<'_>,
                frame: &[u8],
                refresh: Refresh,
                power_off: bool,
            ) -> Result<(), BusError> {
                <$t>::display(self, bus, frame, refresh, power_off).await
            }
            async fn deep_sleep(&mut self, bus: &mut CBusLines<'_>) -> Result<(), BusError> {
                <$t>::deep_sleep(self, bus).await
            }
        }
    };
}

driver!(Uc8279X4, X4_PRO);
driver!(Uc8179, UC8179_X4_PRO);

/// Whether a driver's state fits a panel struct, which the casts below rely on.
const fn fits<P>() -> bool {
    size_of::<P>() <= size_of::<CPanel>() && align_of::<P>() <= align_of::<CPanel>()
}

/// # Safety
/// `panel` is NULL or an aligned, writable panel struct that nothing else uses during the call.
unsafe fn init<P: Driver>(panel: *mut CPanel) {
    const { assert!(fits::<P>()) };
    if !panel.is_null() {
        // SAFETY: as documented; the assert above makes room and alignment. Zeroed first, so
        // the bytes past the state are not left indeterminate for a C copy.
        unsafe {
            panel.write(CPanel([0; 8]));
            panel
                .cast::<MaybeUninit<P>>()
                .write(MaybeUninit::new(P::x4_pro()));
        }
    }
}

/// The driver state in a panel struct that `init::<P>` set up.
///
/// # Safety
/// As the exports': `panel` is NULL or such a struct, used by nothing else during the call.
unsafe fn state<'a, P: Driver>(panel: *mut CPanel) -> Option<&'a mut P> {
    const { assert!(fits::<P>()) };
    // SAFETY: as documented.
    unsafe { panel.cast::<P>().as_mut() }
}

/// # Safety
/// As the `begin` exports.
unsafe fn begin<P: Driver>(panel: *mut CPanel, bus: *const CBus) -> bool {
    // SAFETY: as documented.
    let (Some(panel), Some(bus)) = (unsafe { state::<P>(panel) }, unsafe { bus.as_ref() }) else {
        return false;
    };
    if !complete(bus) {
        return false;
    }
    matches!(block_on(panel.begin(&mut lines(bus))), Some(Ok(())))
}

/// # Safety
/// As the `display` exports.
unsafe fn display<P: Driver>(
    panel: *mut CPanel,
    bus: *const CBus,
    frame: *const u8,
    len: usize,
    refresh: u8,
    power_off: bool,
) -> bool {
    // SAFETY: as documented.
    let (Some(panel), Some(bus)) = (unsafe { state::<P>(panel) }, unsafe { bus.as_ref() }) else {
        return false;
    };
    if !complete(bus) || frame.is_null() {
        return false;
    }
    // SAFETY: as documented.
    let frame = unsafe { core::slice::from_raw_parts(frame, len) };
    let refresh = match refresh {
        0 => Refresh::Full,
        1 => Refresh::Half,
        2 => Refresh::Fast,
        _ => return false,
    };
    matches!(
        block_on(panel.display(&mut lines(bus), frame, refresh, power_off)),
        Some(Ok(()))
    )
}

/// # Safety
/// As the `deep_sleep` exports.
unsafe fn deep_sleep<P: Driver>(panel: *mut CPanel, bus: *const CBus) -> bool {
    // SAFETY: as documented.
    let (Some(panel), Some(bus)) = (unsafe { state::<P>(panel) }, unsafe { bus.as_ref() }) else {
        return false;
    };
    if !complete(bus) {
        return false;
    }
    matches!(block_on(panel.deep_sleep(&mut lines(bus))), Some(Ok(())))
}

/// The C exports of one driver; the safety contract is each's, as epd.h states it. For `init`:
/// `panel` is NULL or an aligned, writable struct of the driver's own type, any contents. For the
/// others: `panel` is NULL or a struct of the driver's own type that its `init` set up (another
/// driver's is undefined behaviour) and nothing else uses during the call; `bus` is NULL or a
/// `pw_epd_bus` whose callbacks may be called during the call; `frame` is NULL or `len` readable
/// bytes.
macro_rules! exports {
    ($t:ty, $init:ident, $begin:ident, $display:ident, $sleep:ident, $resync:ident, $skip:ident) => {
        /// # Safety
        /// See `exports!`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $init(panel: *mut CPanel) {
            // SAFETY: as documented.
            unsafe { init::<$t>(panel) }
        }

        /// # Safety
        /// See `exports!`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $begin(panel: *mut CPanel, bus: *const CBus) -> bool {
            // SAFETY: as documented.
            unsafe { begin::<$t>(panel, bus) }
        }

        /// # Safety
        /// See `exports!`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $display(
            panel: *mut CPanel,
            bus: *const CBus,
            frame: *const u8,
            len: usize,
            refresh: u8,
            power_off: bool,
        ) -> bool {
            // SAFETY: as documented.
            unsafe { display::<$t>(panel, bus, frame, len, refresh, power_off) }
        }

        /// # Safety
        /// See `exports!`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $sleep(panel: *mut CPanel, bus: *const CBus) -> bool {
            // SAFETY: as documented.
            unsafe { deep_sleep::<$t>(panel, bus) }
        }

        /// # Safety
        /// See `exports!`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $resync(panel: *mut CPanel) {
            // SAFETY: as documented.
            if let Some(panel) = unsafe { state::<$t>(panel) } {
                panel.request_resync();
            }
        }

        /// # Safety
        /// See `exports!`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $skip(panel: *mut CPanel) {
            // SAFETY: as documented.
            if let Some(panel) = unsafe { state::<$t>(panel) } {
                panel.skip_initial_resync();
            }
        }
    };
}

exports!(
    Uc8279X4,
    pw_uc8279x4_init,
    pw_uc8279x4_begin,
    pw_uc8279x4_display,
    pw_uc8279x4_deep_sleep,
    pw_uc8279x4_request_resync,
    pw_uc8279x4_skip_initial_resync
);
exports!(
    Uc8179,
    pw_uc8179_init,
    pw_uc8179_begin,
    pw_uc8179_display,
    pw_uc8179_deep_sleep,
    pw_uc8179_request_resync,
    pw_uc8179_skip_initial_resync
);

/// `pw_epd_probe_lines`.
#[repr(C)]
pub struct CProbeLines {
    context: *mut c_void,
    mode: Option<unsafe extern "C" fn(*mut c_void, u8, u8)>,
    write: Option<unsafe extern "C" fn(*mut c_void, u8, bool)>,
    read: Option<unsafe extern "C" fn(*mut c_void, u8) -> bool>,
    release_reset_hold: Option<unsafe extern "C" fn(*mut c_void)>,
    pause_us: Option<unsafe extern "C" fn(*mut c_void, u32)>,
    pause_ms: Option<unsafe extern "C" fn(*mut c_void, u32)>,
}

/// `pw_epd_probe_result`.
#[repr(C)]
pub struct CProbeResult {
    verdict: u8,
    ver: [u8; 5],
    flg: u8,
    mtp_valid: bool,
    mtp: [u8; probe::MTP_LEN],
    controller: u8,
    lut_ver: u8,
}

const _: () = assert!(size_of::<CProbeLines>() == 7 * size_of::<usize>());
const _: () = assert!(size_of::<CProbeResult>() == 58 && align_of::<CProbeResult>() == 1);

/// A `pw_epd_probe_lines` with every callback, which may be called with its context.
struct GpioLines<'a> {
    c: &'a CProbeLines,
    mode: unsafe extern "C" fn(*mut c_void, u8, u8),
    write: unsafe extern "C" fn(*mut c_void, u8, bool),
    read: unsafe extern "C" fn(*mut c_void, u8) -> bool,
    release_reset_hold: unsafe extern "C" fn(*mut c_void),
}

fn line(l: ProbeLine) -> u8 {
    match l {
        ProbeLine::Sclk => 0,
        ProbeLine::Sda => 1,
        ProbeLine::Cs => 2,
        ProbeLine::Dc => 3,
        ProbeLine::Rst => 4,
        ProbeLine::Busy => 5,
    }
}

impl ProbeLines for GpioLines<'_> {
    fn mode(&mut self, l: ProbeLine, mode: Mode) {
        let m = match mode {
            Mode::Output => 0,
            Mode::Input => 1,
            Mode::InputPullUp => 2,
        };
        // SAFETY: the callback takes its context.
        unsafe { (self.mode)(self.c.context, line(l), m) }
    }
    fn write(&mut self, l: ProbeLine, high: bool) {
        // SAFETY: as `mode`.
        unsafe { (self.write)(self.c.context, line(l), high) }
    }
    fn read(&mut self, l: ProbeLine) -> bool {
        // SAFETY: as `mode`.
        unsafe { (self.read)(self.c.context, line(l)) }
    }
    fn release_reset_hold(&mut self) {
        // SAFETY: as `mode`.
        unsafe { (self.release_reset_hold)(self.c.context) }
    }
}

struct ProbePause<'a> {
    c: &'a CProbeLines,
    us: unsafe extern "C" fn(*mut c_void, u32),
    ms: unsafe extern "C" fn(*mut c_void, u32),
}

#[allow(
    clippy::unused_async,
    reason = "the C++ calls block; the futures are ready at once"
)]
impl DelayNs for ProbePause<'_> {
    async fn delay_ns(&mut self, ns: u32) {
        // SAFETY: the callback takes its context.
        unsafe { (self.us)(self.c.context, ns.div_ceil(1000)) }
    }
    async fn delay_us(&mut self, us: u32) {
        // SAFETY: as `delay_ns`.
        unsafe { (self.us)(self.c.context, us) }
    }
    async fn delay_ms(&mut self, ms: u32) {
        // SAFETY: as `delay_ns`.
        unsafe { (self.ms)(self.c.context, ms) }
    }
}

/// # Safety
/// `lines` is NULL or a `pw_epd_probe_lines` whose callbacks may be called during the call;
/// `out` is NULL or a writable `pw_epd_probe_result`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_epd_probe(lines: *const CProbeLines, out: *mut CProbeResult) -> bool {
    // SAFETY: as documented.
    let Some(c) = (unsafe { lines.as_ref() }) else {
        return false;
    };
    let (Some(mode), Some(write), Some(read), Some(release_reset_hold), Some(us), Some(ms)) = (
        c.mode,
        c.write,
        c.read,
        c.release_reset_hold,
        c.pause_us,
        c.pause_ms,
    ) else {
        return false;
    };
    if out.is_null() {
        return false;
    }
    let mut gpio = GpioLines {
        c,
        mode,
        write,
        read,
        release_reset_hold,
    };
    let mut pause = ProbePause { c, us, ms };
    let Some(found) = block_on(probe::probe(&mut gpio, &mut pause)) else {
        return false;
    };
    let (controller, lut_ver) = match found.controller() {
        Controller::Ssd1677 => (0, 0),
        Controller::Uc8179(id) => (1, id),
        Controller::Uc8279(id) => (2, id),
    };
    let result = CProbeResult {
        verdict: match found.verdict {
            Verdict::PrimaryAssumed => 0,
            Verdict::Uc81xxConfirmed => 1,
            Verdict::Inconclusive => 2,
        },
        ver: found.ver,
        flg: found.flg,
        mtp_valid: found.mtp.is_some(),
        mtp: found.mtp.unwrap_or([0; probe::MTP_LEN]),
        controller,
        lut_ver,
    };
    // SAFETY: as documented; written whole, after the callbacks ran, so `out` need not hold a
    // valid result before.
    unsafe { out.write(result) };
    true
}
