//! The X4 Pro's and X4 Classic's fuel gauge: a `CellWise` CW2017 on the caller's I2C bus, shared
//! with the touch controller and the real-time clock. It reports 0% until the 80-byte battery
//! profile of
//! the device's cell is resident, so the first reading checks it (uploading it when it differs)
//! and restarts the gauge when it is not running. Register map, profile and timings are the stock
//! firmware's, as freeink-sdk's `BatteryMonitor` recovered them.
//!
//! The bus and the delay are embedded-hal-async's: a first reading can wait about 4 s, which an
//! executor must be free to spend elsewhere.
//!
//! Nothing here panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::i2c::I2c;

#[cfg(test)]
mod tests;

/// 0xA0 while the gauge starts; 0x0D or 0x0F once its calculation engine runs.
const VERSION: u8 = 0x00;
/// The state of charge, whole percent.
const SOC: u8 = 0x04;
/// Soft reset and sleep.
const MODE: u8 = 0x08;
/// Bit 7: the profile is loaded.
const SOC_ALERT: u8 = 0x0B;
/// The battery profile, 0x10 to 0x5F.
const BATINFO: u8 = 0x10;

const MODE_NORMAL: u8 = 0x00;
const MODE_RESTART: u8 = 0x30;
const MODE_DEFAULT: u8 = 0xF0;
const UPDATE_FLAG: u8 = 0x80;

/// A failed first reading is tried again no sooner than this.
const INIT_RETRY_MS: u32 = 1000;

/// The profile the stock firmware uploads, for the X4 Pro's cell.
const PROFILE: [u8; 80] = [
    0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xaa, 0xbf, 0xb5, 0xb4, 0xa4, 0x9c, 0xeb, 0xe2,
    0xdf, 0xe5, 0xca, 0xa0, 0x8a, 0x62, 0x53, 0x48, 0x40, 0x3a, 0x32, 0xb1, 0xae, 0xda, 0xb5, 0xff,
    0xff, 0xff, 0xe8, 0xdb, 0xd9, 0xd6, 0xd4, 0xd2, 0xd0, 0xcb, 0xc3, 0xbc, 0x9e, 0x87, 0x7b, 0x71,
    0x72, 0x7c, 0x8c, 0xa3, 0xb7, 0xc8, 0xa5, 0x4f, 0x00, 0x00, 0xab, 0x02, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x23,
];

async fn read(bus: &mut impl I2c, addr: u8, reg: u8) -> Option<u8> {
    let mut value = [0];
    bus.write_read(addr, &[reg], &mut value).await.ok()?;
    Some(value[0])
}

async fn write(bus: &mut impl I2c, addr: u8, reg: u8, value: u8) -> bool {
    bus.write(addr, &[reg, value]).await.is_ok()
}

fn running(version: u8) -> bool {
    version & 0xFD == 0x0D
}

/// MODE 0xF0, 0x30, 0x00, 20 ms apart.
async fn reset(bus: &mut impl I2c, delay: &mut impl DelayNs, addr: u8) -> bool {
    for mode in [MODE_DEFAULT, MODE_RESTART, MODE_NORMAL] {
        if !write(bus, addr, MODE, mode).await {
            return false;
        }
        delay.delay_ms(20).await;
    }
    true
}

/// Up to about 1 s for the engine to run, then up to 3 s for a valid state of charge.
async fn wait_until_ready(bus: &mut impl I2c, delay: &mut impl DelayNs, addr: u8) -> bool {
    let mut started = false;
    for _ in 0..50 {
        if read(bus, addr, VERSION).await.is_some_and(running) {
            started = true;
            break;
        }
        delay.delay_ms(20).await;
    }
    if !started {
        return false;
    }
    for _ in 0..30 {
        if read(bus, addr, SOC).await.is_some_and(|soc| soc <= 100) {
            return true;
        }
        delay.delay_ms(100).await;
    }
    false
}

/// Whether the resident profile is ours; `None` on a bus error, which must not trigger a rewrite.
async fn profile_matches(bus: &mut impl I2c, addr: u8) -> Option<bool> {
    if read(bus, addr, SOC_ALERT).await? & UPDATE_FLAG == 0 {
        return Some(false);
    }
    for (reg, &expected) in (BATINFO..).zip(&PROFILE) {
        if read(bus, addr, reg).await? != expected {
            return Some(false);
        }
    }
    Some(true)
}

/// The gauge running with our profile; false on any failure, so the caller tries again.
async fn ensure_profile(bus: &mut impl I2c, delay: &mut impl DelayNs, addr: u8) -> bool {
    let Some(mode) = read(bus, addr, MODE).await else {
        return false;
    };
    let Some(version) = read(bus, addr, VERSION).await else {
        return false;
    };
    let Some(matches) = profile_matches(bus, addr).await else {
        return false;
    };
    let mut restart = mode != MODE_NORMAL || !running(version);
    if !matches {
        for (reg, &value) in (BATINFO..).zip(&PROFILE) {
            if !write(bus, addr, reg, value).await {
                return false;
            }
        }
        if !write(bus, addr, SOC_ALERT, UPDATE_FLAG).await {
            return false;
        }
        delay.delay_ms(20).await;
        restart = true;
    }
    if !restart {
        match read(bus, addr, SOC).await {
            None => return false,
            Some(soc) if soc <= 100 => return true,
            // A running gauge never shows a bad state of charge: one controlled restart.
            Some(_) => {}
        }
    }
    reset(bus, delay, addr).await && wait_until_ready(bus, delay, addr).await
}

/// A CW2017 and what is known about it between readings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cw2017 {
    initialized: bool,
    /// When the last first reading was tried, in ms; 0 for never.
    last_init_ms: u32,
}

impl Cw2017 {
    #[must_use]
    pub const fn new() -> Self {
        Cw2017 {
            initialized: false,
            last_init_ms: 0,
        }
    }

    /// The state of charge (0 to 100) at `now_ms` (a millisecond clock that may wrap); `None`
    /// when the gauge cannot give one now. A gauge found asleep or restarting is brought back on
    /// the next reading.
    pub async fn soc(
        &mut self,
        bus: &mut impl I2c,
        delay: &mut impl DelayNs,
        addr: u8,
        now_ms: u32,
    ) -> Option<u8> {
        if !self.initialized {
            if self.last_init_ms != 0 && now_ms.wrapping_sub(self.last_init_ms) < INIT_RETRY_MS {
                return None;
            }
            self.last_init_ms = now_ms;
            self.initialized = ensure_profile(bus, delay, addr).await;
            if !self.initialized {
                return None;
            }
        }
        // A gauge asleep or restarting gives no reading; the next one brings it back.
        let awake = read(bus, addr, MODE).await == Some(MODE_NORMAL)
            && read(bus, addr, VERSION).await.is_some_and(running);
        let soc = if awake {
            read(bus, addr, SOC).await.filter(|&soc| soc <= 100)
        } else {
            None
        };
        if soc.is_none() {
            self.initialized = false;
        }
        soc
    }
}
