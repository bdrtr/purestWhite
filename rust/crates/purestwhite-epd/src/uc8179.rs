//! The UC8179 that X4 and X4 Pro batches ship in place of the SSD1677: freeink-sdk's
//! `Uc8179Driver`, black and white and its three 4-level grayscale passes.
//!
//! The controller addresses 800x600: the 480 visible rows go first, bottom row first, then the
//! rows past them are padded white. It refreshes differentially like the UC8279 (OLD plane in
//! DTM1, NEW in DTM2, OTP waveforms), with a two-byte VCOM and data interval that is set for the
//! refresh and restored after. A framebuffer has a bit per pixel, 1 for white, rows of
//! `width / 8` bytes.
//!
//! The first refresh after `begin` always flashes; the panel stays powered between refreshes
//! unless a refresh is asked to power it off; after `deep_sleep` only `begin` (a hardware reset)
//! wakes the controller. A failed call leaves the controller's state unknown: `begin` again
//! before the next refresh.
//!
//! Grayscale (the SDK's anti-aliasing): `begin_grayscale`, then `copy_grayscale_lsb` and
//! `copy_grayscale_msb`, then `display_gray`, then `cleanup_grayscale`. In [`GrayMode::Overlay`]
//! and [`GrayMode::Absolute`] the black-and-white base is shown first and the two planes select a
//! short custom waveform per pixel; [`GrayMode::Direct`] programs the controller's full-gray
//! waveform instead and shows the planes alone. The overlay masks are turned into the absolute
//! planes the waveform takes with the base kept in a frame-long buffer (the SDK's PSRAM copy, see
//! [`Uc8179::with_base`]); without one the masks go to the controller as they are. After an
//! Overlay or Absolute gray refresh, the next display, if it is a `Fast` `display` (with no
//! resync or base-less cleanup between), is the stock firmware's non-flashing transition from the
//! previous base. The SDK's `preconditionGrayscale` does nothing on this controller and has no
//! counterpart here.

use crate::uc8279x4::Refresh;
use crate::{Bus, BusError};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::spi::SpiBus;

const PANEL_SETTING: u8 = 0x00;
const POWER_OFF: u8 = 0x02;
const PFS: u8 = 0x03;
const POWER_ON: u8 = 0x04;
const BOOSTER_SOFT_START: u8 = 0x06;
const DEEP_SLEEP: u8 = 0x07;
const DTM1: u8 = 0x10;
const DISPLAY_REFRESH: u8 = 0x12;
const DTM2: u8 = 0x13;
const PLL: u8 = 0x30;
const PARTIAL_WINDOW: u8 = 0x90;
const PARTIAL_IN: u8 = 0x91;
const PARTIAL_OUT: u8 = 0x92;
const CDI: u8 = 0x50;
const RESOLUTION: u8 = 0x61;
const GATE_SOURCE_START: u8 = 0x65;
const CCSET: u8 = 0xE0;
const GATE_SCAN: u8 = 0xE1;
const POWER_SAVE: u8 = 0xE3;
const TSSET: u8 = 0xE5;
const DEEP_SLEEP_CHECK: u8 = 0xA5;
/// The VCOM and data interval's second byte, constant.
const CDI_INTERVAL: u8 = 0x07;
/// PSR bit 5: external LUT. Cleared for the built-in (OTP) refreshes.
const PSR_REG: u8 = 0x20;
/// The PLL during an absolute image's gray refresh, which runs 25% longer, and after it.
const PLL_40_HZ: u8 = 0x05;
const PLL_50_HZ: u8 = 0x06;

/// How long a refresh may take to pull BUSY low before it is taken as started anyway.
const START_WAIT_MS: u32 = 50;
/// The longest row; the X4 Pro's is 100 bytes.
const MAX_ROW: usize = 128;

/// A waveform table's data bytes; the tables go to LUT registers 0x20 (VCOM) to 0x24.
const LUT_LEN: usize = 42;
const LUT_VCOM: u8 = 0x20;

/// A 42-byte table from its leading bytes, zero after.
#[allow(
    clippy::indexing_slicing,
    reason = "evaluated at build time, where an index out of range fails the build"
)]
const fn lut(head: [u8; 12]) -> [u8; LUT_LEN] {
    let mut t = [0; LUT_LEN];
    let mut i = 0;
    while i < head.len() {
        t[i] = head[i];
        i += 1;
    }
    t
}

/// The stock gray waveform, by register 0x20 to 0x24: VCOM, then the pixels whose (DTM1, DTM2)
/// selectors are (1,1) white, (0,1), (1,0) and (0,0) black.
const GRAY_LUTS: [[u8; LUT_LEN]; 5] = [
    lut([
        0x00, 0x02, 0x02, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    ]),
    lut([
        0x08, 0x02, 0x02, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    ]),
    lut([
        0x20, 0x02, 0x02, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    ]),
    // Dark gray (0x23): the SDK's, with the VDL phase a frame shorter than stock's.
    lut([
        0x20, 0x02, 0x01, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    ]),
    lut([
        0x00, 0x02, 0x02, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    ]),
];

/// The stock `XTF_PRE_BW_MID` waveform, by register 0x20 to 0x24: the transition from the
/// previous base to the new one after a gray page.
const PRE_BW_MID: [[u8; LUT_LEN]; 5] = [
    lut([
        0x00, 0x06, 0x01, 0x06, 0x06, 0x01, 0x00, 0x02, 0x04, 0x00, 0x00, 0x01,
    ]),
    lut([
        0x20, 0x06, 0x01, 0x06, 0x06, 0x01, 0x00, 0x02, 0x04, 0x00, 0x00, 0x01,
    ]),
    lut([
        0xAA, 0x06, 0x01, 0x06, 0x06, 0x01, 0xA0, 0x02, 0x04, 0x00, 0x00, 0x01,
    ]),
    lut([
        0x55, 0x06, 0x01, 0x06, 0x06, 0x01, 0x50, 0x02, 0x04, 0x00, 0x00, 0x01,
    ]),
    lut([
        0x00, 0x06, 0x01, 0x06, 0x06, 0x01, 0x10, 0x02, 0x04, 0x00, 0x00, 0x01,
    ]),
];

/// The full-gray waveform (freeink-sdk's `kUltraChipDirectGray`), rows VCOM, black, light, dark,
/// white; 18 leading bytes each.
const DIRECT_GRAY: [[u8; 18]; 5] = [
    [
        0x00, 0x08, 0x0B, 0x02, 0x03, 0x01, 0x00, 0x0C, 0x02, 0x07, 0x02, 0x01, 0x00, 0x01, 0x00,
        0x02, 0x00, 0x01,
    ],
    [
        0xA8, 0x08, 0x0B, 0x02, 0x03, 0x01, 0x44, 0x0C, 0x02, 0x07, 0x02, 0x01, 0x04, 0x01, 0x00,
        0x02, 0x00, 0x01,
    ],
    [
        0x80, 0x08, 0x0B, 0x02, 0x03, 0x01, 0x62, 0x0C, 0x02, 0x07, 0x02, 0x01, 0x00, 0x01, 0x00,
        0x02, 0x00, 0x01,
    ],
    [
        0x88, 0x08, 0x0B, 0x02, 0x03, 0x01, 0x60, 0x0C, 0x02, 0x07, 0x02, 0x01, 0x00, 0x01, 0x00,
        0x02, 0x00, 0x01,
    ],
    [
        0x00, 0x08, 0x0B, 0x02, 0x03, 0x01, 0x4A, 0x0C, 0x02, 0x07, 0x02, 0x01, 0x88, 0x01, 0x00,
        0x02, 0x00, 0x01,
    ],
];
/// Which `DIRECT_GRAY` row each LUT register 0x20 to 0x24 gets: VCOM, then the selectors as in
/// `GRAY_LUTS`, white, light, dark, black.
const DIRECT_GRAY_ROWS: [usize; 5] = [0, 4, 2, 3, 1];
/// The stock `gray_full` packet's header (freeink-sdk's `kUc8179DirectGrayConfig`): power, PLL and
/// VCOM settings, sent before the tables.
const DIRECT_GRAY_CONFIG: [u8; 6] = [0x67, 0xBF, 0x3F, 0x0D, 0x00, 0x26];

/// The settings the stock firmware's `gray_full` packet makes of `DIRECT_GRAY_CONFIG`, as
/// (command, data): 0x52, PLL, power setting (0x01), 0x2A and VCOM DC (0x82).
const fn direct_gray_setup() -> [(u8, [u8; 5], usize); 5] {
    let c = DIRECT_GRAY_CONFIG;
    [
        (0x52, [(c[0] & 8) | (c[1] >> 6), 0, 0, 0, 0], 1),
        (PLL, [c[0] >> 4, 0, 0, 0, 0], 1),
        (
            0x01,
            [0x17, c[0] & 7, c[1] & 0x3F, c[2] & 0x3F, c[3] & 0x3F],
            5,
        ),
        (0x2A, [c[2] & 0xC0, c[4], 0, 0, 0], 2),
        (0x82, [c[5], 0, 0, 0, 0], 1),
    ]
}

/// The controller's settings (freeink-sdk's `Uc8179Config`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// PSR byte 0 at init and for the custom-waveform refreshes; the built-in ones clear its REG
    /// bit.
    pub psr0: u8,
    pub psr1: u8,
    /// Power-off sequence.
    pub pfs: u8,
    /// Booster soft start.
    pub btst: [u8; 4],
    pub gate_scan: u8,
    pub ccset: u8,
    /// Forced temperature for a full and a fast refresh.
    pub tsset: u8,
    pub tsset_fast: u8,
    /// The VCOM and data interval's first byte during a refresh and after it.
    pub cdi_active: u8,
    pub cdi_idle: u8,
    /// Power saving; 0 leaves the controller's default.
    pub power_save: u8,
    pub width: u16,
    pub height: u16,
    /// The gates addressed.
    pub tres_height: u16,
}

/// The X4 Pro's panel, as freeink-sdk's defaults.
pub const X4_PRO: Config = Config {
    psr0: 0x3F,
    psr1: 0x0A,
    pfs: 0x20,
    btst: [0x25, 0x25, 0x3C, 0x25],
    gate_scan: 0x02,
    ccset: 0x02,
    tsset: 0x1E,
    tsset_fast: 0x5A,
    cdi_active: 0x29,
    cdi_idle: 0xA9,
    power_save: 0x22,
    width: 800,
    height: 480,
    tres_height: 600,
};

/// What a grayscale pass's two planes hold (freeink-sdk's `GrayscaleMode`), as (LSB, MSB) bits
/// per pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrayMode {
    /// Masks over the black-and-white base shown first, whose bit is 1 only for white: white and
    /// black (0,0), light (0,1), dark (1,1). The EPUB reader's.
    Overlay,
    /// Every pixel, after the base: black (0,0), dark (1,0), light (0,1), white (1,1). The image
    /// viewer's.
    Absolute,
    /// As `Absolute`, with the full-gray waveform and no base. The sleep screen's.
    Direct,
}

/// A plane's rows, before the white padding.
#[derive(Clone, Copy)]
enum Rows<'f> {
    White,
    Frame(&'f [u8]),
    Inverted(&'f [u8]),
    /// The two frames XOR-ed.
    Xor(&'f [u8], &'f [u8]),
}

/// The `wb` bytes from `start` of `f`, when the row is visible (`start` set).
fn row_of(f: &[u8], start: Option<usize>, wb: usize) -> Option<&[u8]> {
    start.and_then(|s| f.get(s..s + wb))
}

/// The controller's state between calls. `B` holds the grayscale base: a frame-long buffer
/// (`AsRef`/`AsMut<[u8]>`), absent with [`Uc8179::new`].
#[allow(
    clippy::struct_excessive_bools,
    reason = "the SDK's flags, one for one"
)]
pub struct Uc8179<B = &'static mut [u8]> {
    config: Config,
    powered: bool,
    /// DTM1 holds what the panel shows.
    old_plane_valid: bool,
    need_full_clear: bool,
    /// Both planes hold the black-and-white base shown.
    planes_synced: bool,
    /// The SDK's `_grayBase`: the base shown, or the absolute plane0 between the two plane
    /// copies.
    base: Option<B>,
    /// `base` holds the base shown.
    base_valid: bool,
    /// `base` holds plane0, made from it and the LSB mask.
    absolute_planes: bool,
    /// The planes of this pass are absolute.
    absolute_input: bool,
    /// A Direct pass is under way; `direct_planes` has bit 0 once its LSB plane went, bit 1 its
    /// MSB.
    direct_pass: bool,
    direct_planes: u8,
    /// The controller runs the full-gray settings.
    direct_configured: bool,
    /// The panel shows a Direct page.
    direct_on_panel: bool,
    /// The next `Fast` refresh takes the non-flashing transition.
    redrive_after_gray: bool,
    gray_refreshed_once: bool,
}

impl Uc8179 {
    /// Without a grayscale base: Overlay masks go to the controller as they are.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self::build(config, None)
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> Uc8179<B> {
    /// With `base` for the grayscale base; `None` when it is shorter than a frame
    /// (`row_bytes() * height` bytes). Its contents need not be set.
    #[must_use]
    pub fn with_base(config: Config, base: B) -> Option<Self> {
        let state = Self::build(config, Some(base));
        state.base().is_some().then_some(state)
    }

    fn build(config: Config, base: Option<B>) -> Self {
        Uc8179 {
            config,
            powered: false,
            old_plane_valid: false,
            need_full_clear: true,
            planes_synced: false,
            base,
            base_valid: false,
            absolute_planes: false,
            absolute_input: false,
            direct_pass: false,
            direct_planes: 0,
            direct_configured: false,
            direct_on_panel: false,
            redrive_after_gray: false,
            gray_refreshed_once: false,
        }
    }

    /// Whether the grayscale base is kept (`with_base`).
    #[must_use]
    pub fn has_base(&self) -> bool {
        self.base().is_some()
    }

    /// Bytes in a row of the framebuffer.
    #[must_use]
    pub fn row_bytes(&self) -> usize {
        usize::from(self.config.width / 8)
    }

    fn frame_len(&self) -> usize {
        self.row_bytes() * usize::from(self.config.height)
    }

    /// Rows no wider than the plane buffer, and every visible row inside the addressed gates.
    fn geometry_ok(&self) -> bool {
        let c = self.config;
        self.row_bytes() <= MAX_ROW && c.height <= c.tres_height
    }

    /// A frame (or plane) long enough, with a geometry the planes can carry.
    fn fits(&self, frame: &[u8]) -> bool {
        frame.len() >= self.frame_len() && self.geometry_ok()
    }

    fn base(&self) -> Option<&[u8]> {
        let n = self.frame_len();
        self.base.as_ref().and_then(|b| b.as_ref().get(..n))
    }

    fn base_mut(&mut self) -> Option<&mut [u8]> {
        let n = self.frame_len();
        self.base.as_mut().and_then(|b| b.as_mut().get_mut(..n))
    }

    /// Resets the controller and programs it.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    pub async fn begin<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        self.direct_on_panel = false;
        self.direct_configured = false;
        self.direct_pass = false;
        self.direct_planes = 0;
        self.need_full_clear = true;
        self.old_plane_valid = false;
        self.redrive_after_gray = false;
        bus.reset(50).await?;
        self.init_controller(bus).await
    }

    /// The init after a reset, which also forgets what the planes and the panel hold.
    async fn init_controller<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        // The reset powered the panel off, whatever follows.
        self.powered = false;
        self.gray_refreshed_once = false;
        self.planes_synced = false;
        self.base_valid = false;
        self.absolute_planes = false;
        let c = self.config;
        bus.cmd_data(PANEL_SETTING, &[c.psr0, c.psr1]).await?;
        let [w1, w0] = c.width.to_be_bytes();
        let [h1, h0] = c.tres_height.to_be_bytes();
        bus.cmd_data(RESOLUTION, &[w1, w0, h1, h0]).await?;
        bus.cmd_data(GATE_SOURCE_START, &[0; 4]).await?;
        bus.cmd_data(PFS, &[c.pfs]).await?;
        bus.cmd_data(BOOSTER_SOFT_START, &c.btst).await?;
        bus.cmd_data(GATE_SCAN, &[c.gate_scan]).await?;
        if c.power_save != 0 {
            bus.cmd_data(POWER_SAVE, &[c.power_save]).await?;
        }
        Ok(())
    }

    /// Powers off when on, without noting it (the reset that follows does).
    async fn power_off_before_reset<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if self.powered {
            bus.cmd(POWER_OFF).await?;
            bus.wait_idle().await?;
        }
        Ok(())
    }

    /// The full-gray settings for a Direct pass, once.
    async fn configure_direct<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if self.direct_configured {
            return Ok(());
        }
        self.power_off_before_reset(bus).await?;
        bus.reset(50).await?;
        self.init_controller(bus).await?;
        for (command, data, len) in direct_gray_setup() {
            bus.cmd_data(command, data.get(..len).unwrap_or_default())
                .await?;
        }
        bus.frame(CDI, &[self.config.cdi_active, CDI_INTERVAL])
            .await?;
        self.direct_configured = true;
        Ok(())
    }

    /// Back from the full-gray settings to the black-and-white ones: a reset and the init. The
    /// next refresh flashes.
    async fn restore_bw<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if !self.direct_configured {
            return Ok(());
        }
        self.power_off_before_reset(bus).await?;
        bus.reset(50).await?;
        self.init_controller(bus).await?;
        self.direct_configured = false;
        self.need_full_clear = true;
        self.old_plane_valid = false;
        self.redrive_after_gray = false;
        Ok(())
    }

    /// The next `Fast` refresh flashes from white instead; a `Half` stays a `Half`. Also ends a
    /// grayscale pass under way.
    pub fn request_resync(&mut self) {
        self.absolute_input = false;
        self.direct_pass = false;
        self.direct_planes = 0;
        self.need_full_clear = true;
    }

    /// Cancels a `request_resync` made after a refresh. The first refresh after `begin` flashes
    /// regardless, as the SDK's: DTM1 is unknown until a refresh writes it.
    pub fn skip_initial_resync(&mut self) {
        self.need_full_clear = false;
    }

    /// A plane: `rows` bottom first, then white rows to the addressed height.
    async fn plane<S, Cs, Dc, Rst, Busy, D>(
        &self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        command: u8,
        rows: Rows<'_>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        let c = self.config;
        let wb = self.row_bytes().min(MAX_ROW);
        let mut row = [0u8; MAX_ROW];
        let row = row.get_mut(..wb).unwrap_or_default();
        bus.burst(command, row, c.tres_height, |n, row| {
            let start = (n < c.height).then(|| usize::from(c.height - 1 - n) * wb);
            let at = |f| row_of(f, start, wb);
            let written = match rows {
                Rows::White => None,
                Rows::Frame(f) => at(f).map(|src| row.copy_from_slice(src)),
                Rows::Inverted(f) => at(f).map(|src| {
                    for (to, &from) in row.iter_mut().zip(src) {
                        *to = !from;
                    }
                }),
                Rows::Xor(a, b) => at(a).zip(at(b)).map(|(a, b)| {
                    for ((to, &x), &y) in row.iter_mut().zip(a).zip(b) {
                        *to = x ^ y;
                    }
                }),
            };
            if written.is_none() {
                row.fill(0xFF);
            }
        })
        .await
    }

    /// The built-in refresh, in the stock firmware's order; returns once the waveform started.
    async fn start_refresh<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        fast: bool,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        let c = self.config;
        bus.cmd_data(CDI, &[c.cdi_active, CDI_INTERVAL]).await?;
        bus.cmd_data(CCSET, &[c.ccset]).await?;
        bus.cmd_data(TSSET, &[if fast { c.tsset_fast } else { c.tsset }])
            .await?;
        bus.cmd_data(PANEL_SETTING, &[c.psr0 & !PSR_REG, c.psr1])
            .await?;
        if fast {
            bus.cmd_data(PFS, &[c.pfs]).await?;
            bus.cmd_data(GATE_SCAN, &[c.gate_scan]).await?;
        }
        self.power_on(bus).await?;
        // A whole-panel partial refresh: no window on this controller.
        if fast {
            bus.cmd(PARTIAL_IN).await?;
        }
        bus.cmd(DISPLAY_REFRESH).await?;
        bus.wait_busy_start(START_WAIT_MS).await
    }

    async fn power_on<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if !self.powered {
            bus.cmd(POWER_ON).await?;
            bus.wait_idle().await?;
            self.powered = true;
        }
        Ok(())
    }

    async fn power_off<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        bus.cmd(POWER_OFF).await?;
        bus.wait_idle().await?;
        self.powered = false;
        Ok(())
    }

    /// Shows `frame` (a bit per pixel, 1 white, `row_bytes() * height` bytes), and powers the
    /// panel off afterwards when asked. The SDK's blocking `display`: the first `Fast` refresh
    /// after an Overlay or Absolute gray refresh is the non-flashing transition from the previous
    /// base instead (see the module's doc).
    ///
    /// # Errors
    /// When a transfer or a pin fails, `frame` is too short, or the config's rows are wider than
    /// 128 bytes or do not fit the addressed gates.
    pub async fn display<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        frame: &[u8],
        refresh: Refresh,
        power_off: bool,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if !self.fits(frame) {
            return Err(BusError);
        }
        if refresh == Refresh::Fast
            && self.redrive_after_gray
            && self.gray_refreshed_once
            && self.old_plane_valid
            && !self.need_full_clear
        {
            return self.transition_base(bus, frame, power_off).await;
        }
        self.display_plain(bus, frame, refresh, power_off).await
    }

    /// As `display`, never the transition (it is no longer due after this): the SDK's
    /// `displayStart` and `displayFinish`, which its asynchronous refresh uses (the reader's
    /// overlays).
    ///
    /// # Errors
    /// As `display`'s.
    pub async fn display_plain<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        frame: &[u8],
        refresh: Refresh,
        power_off: bool,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if !self.fits(frame) {
            return Err(BusError);
        }
        let paint_destination = self.direct_on_panel;
        self.direct_on_panel = false;
        self.restore_bw(bus).await?;
        self.absolute_input = false;
        self.direct_pass = false;
        self.direct_planes = 0;
        self.planes_synced = false;
        self.absolute_planes = false;
        self.base_valid = false;
        if let Some(base) = self.base_mut()
            && let Some(shown) = frame.get(..base.len())
        {
            base.copy_from_slice(shown);
            self.base_valid = true;
        }
        let fast = refresh == Refresh::Fast && !self.need_full_clear && self.old_plane_valid;
        // DTM1 is rewritten below: until it again holds what the panel shows, a failure must not
        // leave the next partial refresh diffing against it.
        self.old_plane_valid = false;
        if paint_destination {
            // From the Direct page to the frame first, as a partial refresh.
            self.plane(bus, DTM1, Rows::Inverted(frame)).await?;
            self.plane(bus, DTM2, Rows::Frame(frame)).await?;
            self.start_refresh(bus, true).await?;
            bus.wait_idle().await?;
            bus.cmd(PARTIAL_OUT).await?;
        }
        self.plane(bus, DTM2, Rows::Frame(frame)).await?;
        if !fast {
            if refresh == Refresh::Half {
                self.plane(bus, DTM1, Rows::Inverted(frame)).await?;
            } else {
                self.plane(bus, DTM1, Rows::White).await?;
            }
        }
        self.redrive_after_gray = false;
        self.start_refresh(bus, fast).await?;
        bus.wait_idle().await?;
        if fast {
            bus.cmd(PARTIAL_OUT).await?;
        }
        bus.cmd_data(CDI, &[self.config.cdi_idle, CDI_INTERVAL])
            .await?;
        // The next partial refresh starts from what the panel now shows.
        self.plane(bus, DTM1, Rows::Frame(frame)).await?;
        self.old_plane_valid = true;
        self.planes_synced = true;
        self.need_full_clear = false;
        if power_off {
            self.power_off(bus).await?;
        }
        Ok(())
    }

    /// The non-flashing transition from the base DTM1 holds to `frame`, after a gray page.
    async fn transition_base<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        frame: &[u8],
        power_off: bool,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        self.base_valid = false;
        self.absolute_planes = false;
        if let Some(base) = self.base_mut()
            && let Some(shown) = frame.get(..base.len())
        {
            base.copy_from_slice(shown);
            self.base_valid = true;
        }
        bus.wait_idle().await?;
        self.plane(bus, DTM2, Rows::Frame(frame)).await?;
        self.planes_synced = false;
        self.old_plane_valid = false;
        self.precondition(bus).await?;
        // The planes as an ordinary refresh leaves them, in case no gray page follows.
        self.plane(bus, DTM1, Rows::Frame(frame)).await?;
        self.old_plane_valid = true;
        self.planes_synced = true;
        self.redrive_after_gray = false;
        self.need_full_clear = false;
        if power_off && self.powered {
            self.power_off(bus).await?;
        }
        Ok(())
    }

    /// The `XTF_PRE_BW_MID` refresh from DTM1 to DTM2, but not before the first gray page.
    async fn precondition<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        // DTM1 was valid on entry: the caller cleared the flag only for the plane writes.
        if !self.gray_refreshed_once {
            return Ok(());
        }
        let c = self.config;
        bus.wait_idle().await?;
        bus.cmd(PARTIAL_IN).await?;
        let [x1, x0] = c.width.saturating_sub(1).to_be_bytes();
        let [y1, y0] = c.height.saturating_sub(1).to_be_bytes();
        bus.frame(PARTIAL_WINDOW, &[0, 0, x1, x0 | 0x07, 0, 0, y1, y0, 0x01])
            .await?;
        bus.cmd_data(PANEL_SETTING, &[c.psr0, c.psr1]).await?;
        bus.cmd_data(PFS, &[c.pfs]).await?;
        bus.cmd_data(GATE_SCAN, &[c.gate_scan]).await?;
        bus.cmd_data(CDI, &[c.cdi_active, CDI_INTERVAL]).await?;
        bus.cmd_data(CCSET, &[c.ccset]).await?;
        bus.cmd_data(TSSET, &[c.tsset_fast]).await?;
        for (register, table) in (LUT_VCOM..).zip(&PRE_BW_MID) {
            bus.cmd(register).await?;
            bus.data(table).await?;
        }
        self.power_on(bus).await?;
        bus.cmd(DISPLAY_REFRESH).await?;
        bus.wait_idle().await?;
        bus.cmd(PARTIAL_OUT).await?;
        bus.cmd_data(CDI, &[c.cdi_idle, CDI_INTERVAL]).await
    }

    /// The base of a gray page: `fallback` (from `display`), or after a gray page a `Fast`
    /// becomes the non-flashing transition.
    ///
    /// # Errors
    /// As `display`'s.
    pub async fn display_grayscale_base<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        frame: &[u8],
        fallback: Refresh,
        power_off: bool,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if !self.fits(frame) {
            return Err(BusError);
        }
        if fallback != Refresh::Fast
            || !self.gray_refreshed_once
            || !self.old_plane_valid
            || self.need_full_clear
        {
            return self.display(bus, frame, fallback, power_off).await;
        }
        self.transition_base(bus, frame, power_off).await
    }

    /// Starts a grayscale pass: shows `frame`, the base (see `display_grayscale_base`), or for
    /// `Direct` programs the full-gray waveform and leaves the panel as it is (`frame` is not
    /// used).
    ///
    /// # Errors
    /// As `display`'s. A refused frame still ends the pass under way and starts this one, as the
    /// SDK's NULL frame does: after `Absolute` the planes are taken as absolute.
    pub async fn begin_grayscale<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        frame: &[u8],
        mode: GrayMode,
        fallback: Refresh,
        power_off: bool,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if mode == GrayMode::Direct {
            self.configure_direct(bus).await?;
            self.absolute_input = true;
            self.direct_pass = true;
            for (register, &row) in (LUT_VCOM..).zip(&DIRECT_GRAY_ROWS) {
                let mut table = [0; LUT_LEN];
                if let (Some(to), Some(from)) =
                    (table.get_mut(..DIRECT_GRAY[0].len()), DIRECT_GRAY.get(row))
                {
                    to.copy_from_slice(from);
                }
                bus.cmd(register).await?;
                bus.data(&table).await?;
            }
            self.direct_planes = 0;
            self.base_valid = false;
            self.absolute_planes = false;
            self.old_plane_valid = false;
            self.planes_synced = false;
            self.need_full_clear = true;
            self.redrive_after_gray = false;
            return Ok(());
        }
        self.absolute_input = false;
        self.direct_pass = false;
        self.direct_planes = 0;
        // A frame that does not fit ends the pass under way, as the SDK's NULL one.
        let shown = if self.fits(frame) {
            self.display_grayscale_base(bus, frame, fallback, power_off)
                .await
        } else {
            Err(BusError)
        };
        self.absolute_input = mode == GrayMode::Absolute;
        shown
    }

    /// The pass's LSB plane (see [`GrayMode`]), to DTM1; an Overlay mask is folded into the base
    /// first when there is one.
    ///
    /// # Errors
    /// When a transfer or a pin fails or `plane` is too short.
    pub async fn copy_grayscale_lsb<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        plane: &[u8],
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if !self.fits(plane) {
            return Err(BusError);
        }
        bus.wait_idle().await?;
        self.planes_synced = false;
        if self.absolute_input {
            self.plane(bus, DTM1, Rows::Frame(plane)).await?;
            if self.direct_pass {
                self.direct_planes |= 1;
            }
            return Ok(());
        }
        self.absolute_planes = false;
        let folded = self.base_valid;
        if folded {
            // plane0 = base | LSB mask: black (0,0), dark (1,0), light (0,1), white (1,1) with
            // the MSB plane below.
            if let Some(base) = self.base_mut() {
                for (b, &m) in base.iter_mut().zip(plane) {
                    *b |= m;
                }
            }
        }
        self.base_valid = false;
        match self.base().filter(|_| folded) {
            Some(plane0) => {
                self.plane(bus, DTM1, Rows::Frame(plane0)).await?;
                self.absolute_planes = true;
            }
            None => self.plane(bus, DTM1, Rows::Frame(plane)).await?,
        }
        Ok(())
    }

    /// The pass's MSB plane, to DTM2: from an Overlay mask and the folded base, plane1 = plane0
    /// XOR mask, and the base is recovered as plane0 AND plane1.
    ///
    /// # Errors
    /// As `copy_grayscale_lsb`'s.
    pub async fn copy_grayscale_msb<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        plane: &[u8],
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if !self.fits(plane) {
            return Err(BusError);
        }
        bus.wait_idle().await?;
        if self.absolute_input {
            self.planes_synced = false;
            self.plane(bus, DTM2, Rows::Frame(plane)).await?;
            if self.direct_pass {
                self.direct_planes |= 2;
            }
            return Ok(());
        }
        match self.base().filter(|_| self.absolute_planes) {
            Some(plane0) => {
                self.plane(bus, DTM2, Rows::Xor(plane0, plane)).await?;
                if let Some(base) = self.base_mut() {
                    for (b, &m) in base.iter_mut().zip(plane) {
                        *b &= *b ^ m;
                    }
                }
                self.base_valid = true;
            }
            None => self.plane(bus, DTM2, Rows::Frame(plane)).await?,
        }
        Ok(())
    }

    /// Runs the pass's gray refresh on the planes copied. A Direct pass needs both planes, or
    /// sends nothing; it powers off when asked. The others leave the panel powered unless
    /// `factory` (an image) and `power_off`, and put the base back into both planes when there is
    /// one; an Absolute pass with `factory` runs its waveform 25% longer.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    pub async fn display_gray<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        power_off: bool,
        factory: bool,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if self.direct_pass {
            if self.direct_planes != 3 {
                return Ok(());
            }
            bus.wait_idle().await?;
            self.power_on(bus).await?;
            bus.cmd(DISPLAY_REFRESH).await?;
            bus.wait_idle().await?;
            self.direct_on_panel = true;
            self.direct_pass = false;
            self.direct_planes = 0;
            self.absolute_input = false;
            self.need_full_clear = true;
            self.old_plane_valid = false;
            self.redrive_after_gray = false;
            if power_off {
                self.power_off(bus).await?;
            }
            return Ok(());
        }
        let c = self.config;
        bus.wait_idle().await?;
        self.planes_synced = false;
        bus.cmd_data(PANEL_SETTING, &[c.psr0, c.psr1]).await?;
        for (register, table) in (LUT_VCOM..).zip(&GRAY_LUTS) {
            bus.cmd(register).await?;
            bus.data(table).await?;
        }
        bus.cmd_data(CDI, &[c.cdi_active, CDI_INTERVAL]).await?;
        self.gray_refreshed_once = true;
        let slower = factory && self.absolute_input;
        if slower {
            bus.cmd_data(PLL, &[PLL_40_HZ]).await?;
        }
        self.power_on(bus).await?;
        bus.cmd(DISPLAY_REFRESH).await?;
        bus.wait_idle().await?;
        if slower {
            bus.cmd_data(PLL, &[PLL_50_HZ]).await?;
        }
        if self.base_valid {
            if let Some(base) = self.base() {
                self.plane(bus, DTM1, Rows::Frame(base)).await?;
                self.plane(bus, DTM2, Rows::Frame(base)).await?;
            }
            self.old_plane_valid = true;
            self.planes_synced = true;
            self.need_full_clear = false;
        }
        self.base_valid = false;
        self.absolute_planes = false;
        self.redrive_after_gray = true;
        self.absolute_input = false;
        self.direct_pass = false;
        self.direct_planes = 0;
        if factory && power_off && self.powered {
            self.power_off(bus).await?;
        }
        Ok(())
    }

    /// Ends a grayscale pass: both planes back to `bw` (the black-and-white frame shown) unless
    /// they already hold it. Without `bw`, or after a Direct pass, the next refresh flashes.
    ///
    /// # Errors
    /// When a transfer or a pin fails or `bw` is too short.
    pub async fn cleanup_grayscale<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        bw: Option<&[u8]>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        if bw.is_some_and(|f| !self.fits(f)) {
            return Err(BusError);
        }
        bus.wait_idle().await?;
        self.absolute_input = false;
        self.direct_pass = false;
        self.direct_planes = 0;
        self.base_valid = false;
        self.absolute_planes = false;
        if self.direct_configured {
            self.need_full_clear = true;
            self.old_plane_valid = false;
            self.planes_synced = false;
            return Ok(());
        }
        let Some(bw) = bw else {
            self.need_full_clear = true;
            self.old_plane_valid = false;
            return Ok(());
        };
        if self.planes_synced && self.old_plane_valid {
            return Ok(());
        }
        self.old_plane_valid = false;
        self.plane(bus, DTM1, Rows::Frame(bw)).await?;
        self.plane(bus, DTM2, Rows::Frame(bw)).await?;
        self.old_plane_valid = true;
        self.planes_synced = true;
        Ok(())
    }

    /// Powers the panel off when on, then puts the controller in deep sleep; `begin` wakes it.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    pub async fn deep_sleep<S, Cs, Dc, Rst, Busy, D>(
        &mut self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
    ) -> Result<(), BusError>
    where
        S: SpiBus,
        Cs: OutputPin,
        Dc: OutputPin,
        Rst: OutputPin,
        Busy: InputPin,
        D: DelayNs,
    {
        self.direct_on_panel = false;
        self.absolute_input = false;
        self.direct_pass = false;
        self.direct_planes = 0;
        self.base_valid = false;
        self.absolute_planes = false;
        if self.powered {
            self.power_off(bus).await?;
        }
        bus.cmd_data(DEEP_SLEEP, &[DEEP_SLEEP_CHECK]).await
    }
}
