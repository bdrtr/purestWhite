//! The UC8179 that X4 and X4 Pro batches ship in place of the SSD1677: freeink-sdk's
//! `Uc8179Driver`, black and white only. The 4-level grayscale paths (external LUTs, the direct
//! gray configuration) are not here yet.
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

/// How long a refresh may take to pull BUSY low before it is taken as started anyway.
const START_WAIT_MS: u32 = 50;
/// The longest row; the X4 Pro's is 100 bytes.
const MAX_ROW: usize = 128;

/// The controller's settings (freeink-sdk's `Uc8179Config`, black-and-white part).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// PSR byte 0 at init; the refreshes clear its REG bit.
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

/// The controller's state between calls.
pub struct Uc8179 {
    config: Config,
    powered: bool,
    /// DTM1 holds what the panel shows.
    old_plane_valid: bool,
    need_full_clear: bool,
}

impl Uc8179 {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Uc8179 {
            config,
            powered: false,
            old_plane_valid: false,
            need_full_clear: true,
        }
    }

    /// Bytes in a row of the framebuffer.
    #[must_use]
    pub fn row_bytes(&self) -> usize {
        usize::from(self.config.width / 8)
    }

    /// Rows no wider than the plane buffer, and every visible row inside the addressed gates.
    fn geometry_ok(&self) -> bool {
        let c = self.config;
        self.row_bytes() <= MAX_ROW && c.height <= c.tres_height
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
        self.old_plane_valid = false;
        self.need_full_clear = true;
        bus.reset(50).await?;
        // The reset powered the panel off, whatever follows.
        self.powered = false;
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

    /// The next `Fast` refresh flashes from white instead; a `Half` stays a `Half`.
    pub fn request_resync(&mut self) {
        self.need_full_clear = true;
    }

    /// Cancels a `request_resync` made after a refresh. The first refresh after `begin` flashes
    /// regardless, as the SDK's: DTM1 is unknown until a refresh writes it.
    pub fn skip_initial_resync(&mut self) {
        self.need_full_clear = false;
    }

    /// A plane: `frame`'s rows bottom first (inverted when asked), then white rows to the
    /// addressed height. Without `frame`, all white.
    async fn plane<S, Cs, Dc, Rst, Busy, D>(
        &self,
        bus: &mut Bus<S, Cs, Dc, Rst, Busy, D>,
        command: u8,
        frame: Option<&[u8]>,
        invert: bool,
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
            let source = frame.filter(|_| n < c.height).and_then(|f| {
                let start = usize::from(c.height - 1 - n) * wb;
                f.get(start..start + wb)
            });
            match source {
                Some(src) => {
                    for (to, &from) in row.iter_mut().zip(src) {
                        *to = if invert { !from } else { from };
                    }
                }
                None => row.fill(0xFF),
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
        if !self.powered {
            bus.cmd(POWER_ON).await?;
            bus.wait_idle().await?;
            self.powered = true;
        }
        // A whole-panel partial refresh: no window on this controller.
        if fast {
            bus.cmd(PARTIAL_IN).await?;
        }
        bus.cmd(DISPLAY_REFRESH).await?;
        bus.wait_busy_start(START_WAIT_MS).await
    }

    /// Shows `frame` (a bit per pixel, 1 white, `row_bytes() * height` bytes), and powers the
    /// panel off afterwards when asked.
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
        if frame.len() < self.row_bytes() * usize::from(self.config.height) || !self.geometry_ok() {
            return Err(BusError);
        }
        let fast = refresh == Refresh::Fast && !self.need_full_clear && self.old_plane_valid;
        // DTM1 is rewritten below: until it again holds what the panel shows, a failure must not
        // leave the next partial refresh diffing against it.
        self.old_plane_valid = false;
        self.plane(bus, DTM2, Some(frame), false).await?;
        if !fast {
            if refresh == Refresh::Half {
                self.plane(bus, DTM1, Some(frame), true).await?;
            } else {
                self.plane(bus, DTM1, None, false).await?;
            }
        }
        self.start_refresh(bus, fast).await?;
        bus.wait_idle().await?;
        if fast {
            bus.cmd(PARTIAL_OUT).await?;
        }
        bus.cmd_data(CDI, &[self.config.cdi_idle, CDI_INTERVAL])
            .await?;
        // The next partial refresh starts from what the panel now shows.
        self.plane(bus, DTM1, Some(frame), false).await?;
        self.old_plane_valid = true;
        self.need_full_clear = false;
        if power_off {
            bus.cmd(POWER_OFF).await?;
            bus.wait_idle().await?;
            self.powered = false;
        }
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
        if self.powered {
            bus.cmd(POWER_OFF).await?;
            bus.wait_idle().await?;
            self.powered = false;
        }
        bus.cmd_data(DEEP_SLEEP, &[DEEP_SLEEP_CHECK]).await
    }
}
