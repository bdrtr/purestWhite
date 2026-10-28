//! The controller itself: reset and init, the black-and-white and full-gray settings, power,
//! the built-in refresh and deep sleep.

use super::Uc8179;
use super::command::{
    BOOSTER_SOFT_START, CCSET, CDI, CDI_INTERVAL, DEEP_SLEEP, DEEP_SLEEP_CHECK, DISPLAY_REFRESH,
    GATE_SCAN, GATE_SOURCE_START, PANEL_SETTING, PARTIAL_IN, PFS, POWER_OFF, POWER_ON, POWER_SAVE,
    PSR_REG, RESOLUTION, TSSET,
};
use super::waveform::direct_gray_setup;
use crate::{BusError, Interface};

/// How long a refresh may take to pull BUSY low before it is taken as started anyway.
const START_WAIT_MS: u32 = 50;

impl<B: AsRef<[u8]> + AsMut<[u8]>> Uc8179<B> {
    /// Resets the controller and programs it.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    pub async fn begin(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
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
    async fn init_controller(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
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
    async fn power_off_before_reset(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
        if self.powered {
            bus.cmd(POWER_OFF).await?;
            bus.wait_idle().await?;
        }
        Ok(())
    }

    /// The full-gray settings for a Direct pass, once.
    pub(super) async fn configure_direct(
        &mut self,
        bus: &mut impl Interface,
    ) -> Result<(), BusError> {
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
    pub(super) async fn restore_bw(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
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

    /// The built-in refresh, in the stock firmware's order; returns once the waveform started.
    pub(super) async fn start_refresh(
        &mut self,
        bus: &mut impl Interface,
        fast: bool,
    ) -> Result<(), BusError> {
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

    pub(super) async fn power_on(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
        if !self.powered {
            bus.cmd(POWER_ON).await?;
            bus.wait_idle().await?;
            self.powered = true;
        }
        Ok(())
    }

    pub(super) async fn power_off(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
        bus.cmd(POWER_OFF).await?;
        bus.wait_idle().await?;
        self.powered = false;
        Ok(())
    }

    /// Powers the panel off when on, then puts the controller in deep sleep; `begin` wakes it.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    pub async fn deep_sleep(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
        self.direct_on_panel = false;
        self.end_pass();
        self.base_valid = false;
        self.absolute_planes = false;
        if self.powered {
            self.power_off(bus).await?;
        }
        bus.cmd_data(DEEP_SLEEP, &[DEEP_SLEEP_CHECK]).await
    }
}
