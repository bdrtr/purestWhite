//! Grayscale passes: the base, the two planes, the gray refresh and the cleanup after it (see
//! `uc8179`'s doc for the modes).

use super::command::{
    CDI, CDI_INTERVAL, DISPLAY_REFRESH, DTM1, DTM2, PANEL_SETTING, PLL, PLL_40_HZ, PLL_50_HZ,
};
use super::plane::Rows;
use super::waveform::{DIRECT_GRAY_LUTS, GRAY_LUTS, LUT_VCOM};
use super::{GrayMode, Uc8179};
use crate::uc8279x4::Refresh;
use crate::{BusError, Interface};

impl<B: AsRef<[u8]> + AsMut<[u8]>> Uc8179<B> {
    /// The base of a gray page: `fallback` (from `display`), or after a gray page a `Fast`
    /// becomes the non-flashing transition.
    ///
    /// # Errors
    /// As `display`'s.
    pub async fn display_grayscale_base(
        &mut self,
        bus: &mut impl Interface,
        frame: &[u8],
        fallback: Refresh,
        power_off: bool,
    ) -> Result<(), BusError> {
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
    pub async fn begin_grayscale(
        &mut self,
        bus: &mut impl Interface,
        frame: &[u8],
        mode: GrayMode,
        fallback: Refresh,
        power_off: bool,
    ) -> Result<(), BusError> {
        if mode == GrayMode::Direct {
            return self.begin_direct(bus).await;
        }
        self.end_pass();
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

    /// A Direct pass: the full-gray settings and waveform. The panel and the planes are left as
    /// they are; the next black-and-white refresh flashes.
    async fn begin_direct(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
        self.configure_direct(bus).await?;
        self.absolute_input = true;
        self.direct_pass = true;
        for (register, table) in (LUT_VCOM..).zip(&DIRECT_GRAY_LUTS) {
            bus.cmd(register).await?;
            bus.data(table).await?;
        }
        self.direct_planes = 0;
        self.base_valid = false;
        self.absolute_planes = false;
        self.old_plane_valid = false;
        self.planes_synced = false;
        self.need_full_clear = true;
        self.redrive_after_gray = false;
        Ok(())
    }

    /// The pass's LSB plane (see [`GrayMode`]), to DTM1; an Overlay mask is folded into the base
    /// first when there is one.
    ///
    /// # Errors
    /// When a transfer or a pin fails or `plane` is too short.
    pub async fn copy_grayscale_lsb(
        &mut self,
        bus: &mut impl Interface,
        plane: &[u8],
    ) -> Result<(), BusError> {
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
    pub async fn copy_grayscale_msb(
        &mut self,
        bus: &mut impl Interface,
        plane: &[u8],
    ) -> Result<(), BusError> {
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
    pub async fn display_gray(
        &mut self,
        bus: &mut impl Interface,
        power_off: bool,
        factory: bool,
    ) -> Result<(), BusError> {
        if self.direct_pass {
            return self.display_direct(bus, power_off).await;
        }
        bus.wait_idle().await?;
        self.planes_synced = false;
        self.load_gray_waveform(bus).await?;
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
            self.restore_base_planes(bus).await?;
        }
        self.base_valid = false;
        self.absolute_planes = false;
        self.redrive_after_gray = true;
        self.end_pass();
        if factory && power_off && self.powered {
            self.power_off(bus).await?;
        }
        Ok(())
    }

    /// A Direct pass's refresh, once both planes went; powers off when asked.
    async fn display_direct(
        &mut self,
        bus: &mut impl Interface,
        power_off: bool,
    ) -> Result<(), BusError> {
        if self.direct_planes != 3 {
            return Ok(());
        }
        bus.wait_idle().await?;
        self.power_on(bus).await?;
        bus.cmd(DISPLAY_REFRESH).await?;
        bus.wait_idle().await?;
        self.direct_on_panel = true;
        self.end_pass();
        self.need_full_clear = true;
        self.old_plane_valid = false;
        self.redrive_after_gray = false;
        if power_off {
            self.power_off(bus).await?;
        }
        Ok(())
    }

    /// The stock gray waveform, with the custom-waveform PSR and the refresh's CDI.
    async fn load_gray_waveform(&self, bus: &mut impl Interface) -> Result<(), BusError> {
        let c = self.config;
        bus.cmd_data(PANEL_SETTING, &[c.psr0, c.psr1]).await?;
        for (register, table) in (LUT_VCOM..).zip(&GRAY_LUTS) {
            bus.cmd(register).await?;
            bus.data(table).await?;
        }
        bus.cmd_data(CDI, &[c.cdi_active, CDI_INTERVAL]).await
    }

    /// The base back into both planes after a gray refresh, as a black-and-white refresh leaves
    /// them.
    async fn restore_base_planes(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
        if let Some(base) = self.base() {
            self.plane(bus, DTM1, Rows::Frame(base)).await?;
            self.plane(bus, DTM2, Rows::Frame(base)).await?;
        }
        self.old_plane_valid = true;
        self.planes_synced = true;
        self.need_full_clear = false;
        Ok(())
    }

    /// Ends a grayscale pass: both planes back to `bw` (the black-and-white frame shown) unless
    /// they already hold it. Without `bw`, or after a Direct pass, the next refresh flashes.
    ///
    /// # Errors
    /// When a transfer or a pin fails or `bw` is too short.
    pub async fn cleanup_grayscale(
        &mut self,
        bus: &mut impl Interface,
        bw: Option<&[u8]>,
    ) -> Result<(), BusError> {
        if bw.is_some_and(|f| !self.fits(f)) {
            return Err(BusError);
        }
        bus.wait_idle().await?;
        self.end_pass();
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
}
