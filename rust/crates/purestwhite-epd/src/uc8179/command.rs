//! The controller's command bytes and the fixed values sent with them.

pub(super) const PANEL_SETTING: u8 = 0x00;
pub(super) const POWER_OFF: u8 = 0x02;
pub(super) const PFS: u8 = 0x03;
pub(super) const POWER_ON: u8 = 0x04;
pub(super) const BOOSTER_SOFT_START: u8 = 0x06;
pub(super) const DEEP_SLEEP: u8 = 0x07;
pub(super) const DTM1: u8 = 0x10;
pub(super) const DISPLAY_REFRESH: u8 = 0x12;
pub(super) const DTM2: u8 = 0x13;
pub(super) const PLL: u8 = 0x30;
pub(super) const PARTIAL_WINDOW: u8 = 0x90;
pub(super) const PARTIAL_IN: u8 = 0x91;
pub(super) const PARTIAL_OUT: u8 = 0x92;
pub(super) const CDI: u8 = 0x50;
pub(super) const RESOLUTION: u8 = 0x61;
pub(super) const GATE_SOURCE_START: u8 = 0x65;
pub(super) const CCSET: u8 = 0xE0;
pub(super) const GATE_SCAN: u8 = 0xE1;
pub(super) const POWER_SAVE: u8 = 0xE3;
pub(super) const TSSET: u8 = 0xE5;
pub(super) const DEEP_SLEEP_CHECK: u8 = 0xA5;
/// The VCOM and data interval's second byte, constant.
pub(super) const CDI_INTERVAL: u8 = 0x07;
/// PSR bit 5: external LUT. Cleared for the built-in (OTP) refreshes.
pub(super) const PSR_REG: u8 = 0x20;
/// The PLL during an absolute image's gray refresh, which runs 25% longer, and after it.
pub(super) const PLL_40_HZ: u8 = 0x05;
pub(super) const PLL_50_HZ: u8 = 0x06;
