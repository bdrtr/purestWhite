//! Black-and-white refreshes: the built-in full, half and fast ones, and after a gray page the
//! stock firmware's non-flashing transition to the next base.

use super::Uc8179;
use super::command::{
    CCSET, CDI, CDI_INTERVAL, DISPLAY_REFRESH, DTM1, DTM2, GATE_SCAN, PANEL_SETTING, PARTIAL_IN,
    PARTIAL_OUT, PARTIAL_WINDOW, PFS, TSSET,
};
use super::plane::Rows;
use super::waveform::{LUT_VCOM, PRE_BW_MID};
use crate::uc8279x4::Refresh;
use crate::{BusError, Interface};

impl<B: AsRef<[u8]> + AsMut<[u8]>> Uc8179<B> {
    /// Shows `frame` (a bit per pixel, 1 white, `row_bytes() * height` bytes), and powers the
    /// panel off afterwards when asked. The SDK's blocking `display`: the first `Fast` refresh
    /// after an Overlay or Absolute gray refresh is the non-flashing transition from the previous
    /// base instead (see the module's doc).
    ///
    /// # Errors
    /// When a transfer or a pin fails, `frame` is too short, or the config's rows are wider than
    /// 128 bytes or do not fit the addressed gates.
    pub async fn display(
        &mut self,
        bus: &mut impl Interface,
        frame: &[u8],
        refresh: Refresh,
        power_off: bool,
    ) -> Result<(), BusError> {
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
    pub async fn display_plain(
        &mut self,
        bus: &mut impl Interface,
        frame: &[u8],
        refresh: Refresh,
        power_off: bool,
    ) -> Result<(), BusError> {
        if !self.fits(frame) {
            return Err(BusError);
        }
        let from_direct = self.direct_on_panel;
        self.direct_on_panel = false;
        self.restore_bw(bus).await?;
        self.end_pass();
        self.planes_synced = false;
        self.keep_base(frame);
        let fast = refresh == Refresh::Fast && !self.need_full_clear && self.old_plane_valid;
        // DTM1 is rewritten below: until it again holds what the panel shows, a failure must not
        // leave the next partial refresh diffing against it.
        self.old_plane_valid = false;
        if from_direct {
            self.leave_direct_page(bus, frame).await?;
        }
        self.load_planes(bus, frame, refresh, fast).await?;
        self.redrive_after_gray = false;
        self.start_refresh(bus, fast).await?;
        bus.wait_idle().await?;
        if fast {
            bus.cmd(PARTIAL_OUT).await?;
        }
        self.finish_refresh(bus, frame, power_off).await
    }

    /// From the Direct page the panel shows to `frame`, as a partial refresh.
    async fn leave_direct_page(
        &mut self,
        bus: &mut impl Interface,
        frame: &[u8],
    ) -> Result<(), BusError> {
        self.plane(bus, DTM1, Rows::Inverted(frame)).await?;
        self.plane(bus, DTM2, Rows::Frame(frame)).await?;
        self.start_refresh(bus, true).await?;
        bus.wait_idle().await?;
        bus.cmd(PARTIAL_OUT).await
    }

    /// The planes a refresh to `frame` starts from: `frame` in DTM2 and, unless `fast` (DTM1
    /// holds what the panel shows), white in DTM1, or for `Half` the frame's complement.
    async fn load_planes(
        &self,
        bus: &mut impl Interface,
        frame: &[u8],
        refresh: Refresh,
        fast: bool,
    ) -> Result<(), BusError> {
        self.plane(bus, DTM2, Rows::Frame(frame)).await?;
        if fast {
            return Ok(());
        }
        let old = if refresh == Refresh::Half {
            Rows::Inverted(frame)
        } else {
            Rows::White
        };
        self.plane(bus, DTM1, old).await
    }

    /// After a refresh to `frame`: the idle CDI, and `frame` in DTM1, which the next partial
    /// refresh starts from; the panel powered off when asked.
    async fn finish_refresh(
        &mut self,
        bus: &mut impl Interface,
        frame: &[u8],
        power_off: bool,
    ) -> Result<(), BusError> {
        bus.cmd_data(CDI, &[self.config.cdi_idle, CDI_INTERVAL])
            .await?;
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
    pub(super) async fn transition_base(
        &mut self,
        bus: &mut impl Interface,
        frame: &[u8],
        power_off: bool,
    ) -> Result<(), BusError> {
        self.keep_base(frame);
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
    pub(super) async fn precondition(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
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
}
