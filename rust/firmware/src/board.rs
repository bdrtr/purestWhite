//! The X4 Pro's wiring, in one place: [`Board::take`] splits the chip's peripherals into the
//! device's parts (freeink-sdk's board profile), each handed over as it is, not yet set up. The
//! layers that drive a part take its pins from here. Also what the board fixes beyond the pins:
//! the power latch ([`hold_power_latch`]) and the PSRAM's mode ([`psram_config`]).

use esp_bootloader_esp_idf::partitions::FlashStorage;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{
    CPU_CTRL, FLASH, FROM_CPU_INTR0, GPIO0, GPIO1, GPIO2, GPIO3, GPIO4, GPIO5, GPIO6, GPIO7,
    GPIO10, GPIO11, GPIO12, GPIO13, GPIO14, GPIO18, GPIO38, GPIO39, GPIO40, GPIO41, GPIO42, I2C0,
    LPWR, PSRAM, Peripherals, RTC_TIMER, SDHOST, SHA, SPI1, SPI2, SYSTIMER,
};
use esp_hal::psram::{
    FlashFreq, PsramConfig, PsramMode, PsramSize, SpiRamFreq, SpiTimingConfigCoreClock,
};

/// What every image takes first: the RTC watchdog's timer, the power latch (GPIO1, high keeps
/// the device on), and the flash and the second core, for handing the next boot back.
pub struct Boot {
    pub rtc_timer: RTC_TIMER<'static>,
    pub latch: GPIO1<'static>,
    pub flash: FLASH<'static>,
    pub cpu: CPU_CTRL<'static>,
}

/// The panel: SCLK 12, MOSI 11, CS 13, DC 18, RST 14, BUSY 6, on SPI2.
pub struct PanelParts {
    pub spi: SPI2<'static>,
    pub sclk: GPIO12<'static>,
    pub mosi: GPIO11<'static>,
    pub cs: GPIO13<'static>,
    pub dc: GPIO18<'static>,
    pub rst: GPIO14<'static>,
    pub busy: GPIO6<'static>,
}

/// The SD card: SDMMC slot 1 (CLK 41, CMD 42, DAT0 40) and its supply (GPIO5, active low).
pub struct CardParts {
    pub sdhost: SDHOST<'static>,
    pub supply: GPIO5<'static>,
    pub clk: GPIO41<'static>,
    pub cmd: GPIO42<'static>,
    pub dat0: GPIO40<'static>,
}

/// The I2C bus the touch panel, the clock and the fuel gauge share: I2C0, SDA 39, SCL 38; and the
/// touch panel's supply (GPIO2, active low), which the C++ firmware turns on before it drives the
/// bus at all.
pub struct I2cParts {
    pub i2c: I2C0<'static>,
    pub sda: GPIO39<'static>,
    pub scl: GPIO38<'static>,
    pub supply: GPIO2<'static>,
}

/// The GT911 touch panel, on the shared I2C bus: RST 4, INT 10.
pub struct TouchParts {
    pub reset: GPIO4<'static>,
    pub int: GPIO10<'static>,
}

/// The three buttons, active low: Left (GPIO0, a boot strap), Right (GPIO7) and Power (GPIO3);
/// the SDK's Up, Down and Power.
pub struct ButtonParts {
    pub left: GPIO0<'static>,
    pub right: GPIO7<'static>,
    pub power: GPIO3<'static>,
}

/// What esp-rtos runs on: the system timer and a software interrupt.
pub struct RtosParts {
    pub systimer: SYSTIMER<'static>,
    pub interrupt: FROM_CPU_INTR0<'static>,
}

/// The device's parts.
pub struct Board {
    pub boot: Boot,
    pub panel: PanelParts,
    pub card: CardParts,
    pub i2c: I2cParts,
    pub touch: TouchParts,
    pub buttons: ButtonParts,
    pub psram: PSRAM<'static>,
    /// The RTC's low-power control: deep sleep and its wake sources.
    pub low_power: LPWR<'static>,
    pub rtos: RtosParts,
    /// The SHA accelerator, for checking firmware images.
    pub sha: SHA<'static>,
}

impl Board {
    /// The parts, from the peripherals `esp_hal::init` gave.
    #[must_use]
    pub fn take(p: Peripherals) -> Self {
        Board {
            boot: Boot {
                rtc_timer: p.RTC_TIMER,
                latch: p.GPIO1,
                flash: p.FLASH,
                cpu: p.CPU_CTRL,
            },
            panel: PanelParts {
                spi: p.SPI2,
                sclk: p.GPIO12,
                mosi: p.GPIO11,
                cs: p.GPIO13,
                dc: p.GPIO18,
                rst: p.GPIO14,
                busy: p.GPIO6,
            },
            card: CardParts {
                sdhost: p.SDHOST,
                supply: p.GPIO5,
                clk: p.GPIO41,
                cmd: p.GPIO42,
                dat0: p.GPIO40,
            },
            i2c: I2cParts {
                i2c: p.I2C0,
                sda: p.GPIO39,
                scl: p.GPIO38,
                supply: p.GPIO2,
            },
            touch: TouchParts {
                reset: p.GPIO4,
                int: p.GPIO10,
            },
            buttons: ButtonParts {
                left: p.GPIO0,
                right: p.GPIO7,
                power: p.GPIO3,
            },
            psram: p.PSRAM,
            low_power: p.LPWR,
            rtos: RtosParts {
                systimer: p.SYSTIMER,
                interrupt: p.FROM_CPU_INTR0,
            },
            sha: p.SHA,
        }
    }
}

/// The PSRAM's clock, in MHz: 40, not the C++ firmware's 80. ESP-IDF tunes the sampling timing
/// at every boot for octal PSRAM above 40 MHz, and esp-hal 1.2.2 does not yet (esp-hal#6618). The
/// `psram-80mhz` feature runs it at 80 without that tuning, for a test image only: it may read
/// wrong data on another chip or when warm.
#[cfg(not(feature = "psram-80mhz"))]
pub const PSRAM_MHZ: u32 = 40;
#[cfg(feature = "psram-80mhz")]
pub const PSRAM_MHZ: u32 = 80;

/// The PSRAM's clock as esp-hal takes it, and the clock of the memory interface it is divided
/// from: at 80 MHz, 160 MHz as ESP-IDF has it (and esp-hal#3956 found it needs).
#[cfg(not(feature = "psram-80mhz"))]
const PSRAM_CLOCKS: (SpiRamFreq, Option<SpiTimingConfigCoreClock>) = (SpiRamFreq::Freq40m, None);
#[cfg(feature = "psram-80mhz")]
const PSRAM_CLOCKS: (SpiRamFreq, Option<SpiTimingConfigCoreClock>) = (
    SpiRamFreq::Freq80m,
    Some(SpiTimingConfigCoreClock::SpiTimingConfigCoreClock160m),
);

/// The X4 Pro's PSRAM: 8 MB octal, beside flash at 80 MHz, as the C++ firmware has it (the Arduino
/// core's `dio_opi` variant), at `PSRAM_MHZ`; its size is read from the chip.
#[must_use]
pub fn psram_config() -> PsramConfig {
    let (ram_frequency, core_clock) = PSRAM_CLOCKS;
    PsramConfig {
        mode: PsramMode::OctalSpi,
        size: PsramSize::AutoDetect,
        core_clock,
        flash_frequency: FlashFreq::FlashFreq80m,
        ram_frequency,
    }
}

/// Raises the X4 Pro's power latch (GPIO1) and holds its pad: on battery the device turns off as
/// soon as the latch floats, and a system reset releases every pin it does not hold. The C++
/// firmware releases the hold at boot (`BoardConfig::holdPowerRails`).
#[must_use = "keep the pin for the rest of the run"]
pub fn hold_power_latch(pin: GPIO1<'_>) -> Output<'_> {
    let mut latch = Output::new(pin, Level::High, OutputConfig::default());
    latch.set_pad_hold(true);
    latch
}

/// The flash as esp-storage's storage, SPI1 made to select the flash alone first
/// (`select_flash_alone`). esp-storage 0.10 reads the flash's size with a hardware command when a
/// storage is made: make one per run, before anything writes the flash (see `crate::boot::start`).
#[must_use]
pub fn flash_storage(flash: FLASH<'_>) -> FlashStorage<'_> {
    select_flash_alone();
    FlashStorage::new(flash)
}

/// Makes SPI1, which esp-storage drives through the ROM, select the flash alone, as ESP-IDF's
/// flash driver does before each operation: its second chip select is the PSRAM's, which would
/// otherwise answer every flash command too. It is the chip's reset value, which eink-qemu 0.9.2's
/// cold boot does not give. Starting the PSRAM sends commands through SPI1 and may leave it
/// otherwise: call this again before using the flash after that.
pub fn select_flash_alone() {
    SPI1::regs()
        .misc()
        .modify(|_, w| w.cs0_dis().clear_bit().cs1_dis().set_bit());
}
