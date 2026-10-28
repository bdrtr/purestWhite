//! The card's font, loaded whole into PSRAM at the start so that drawing never reads the card.

use allocator_api2::vec::Vec;
use bembeyaz_font::Memory;
use bembeyaz_gfx::Canvas;
use esp_alloc::ExternalMemory;
use esp_hal::delay::Delay;
use esp_hal::gpio::Output;
use esp_hal::peripherals::{GPIO40, GPIO41, GPIO42, PSRAM};
use esp_hal::psram::Psram;
use esp_hal::sdmmc::{SdHostController, SlotConfig};
use esp_hal::time::Instant;
use purestwhite_fat::Volume;
use purestwhite_firmware::board::psram_config;
use purestwhite_firmware::ready;
use purestwhite_firmware::sd::{self, BlockingBus, Card, CardBlocks, SpinDelay};
use purestwhite_firmware::storage::load_whole;
use purestwhite_render::Writer;

/// The font, at the same place on the owner's card and the emulator's.
const FONT: &str = ".fonts/LiberationSerif/LiberationSerif_14.cpfont";

/// The card's font, in PSRAM.
pub struct Font<'t> {
    pub bytes: Vec<u8, ExternalMemory>,
    writer: Writer<'t>,
    /// How long the load took, in milliseconds, for the report.
    pub load_ms: u64,
}

impl Font<'_> {
    pub fn width(&mut self, text: &str) -> i32 {
        self.writer.width(&mut Memory::new(&self.bytes), text)
    }

    pub fn draw(&mut self, c: &mut Canvas<'_>, x: i32, y: i32, text: &str) -> i32 {
        self.writer
            .draw(&mut Memory::new(&self.bytes), c, x, y, text)
    }
}

/// The PSRAM, added to the heap for the font: after the internal heap, which ordinary allocations
/// take first; those the internal heap has no room for go to the PSRAM too. Keep it for the run.
pub fn add_psram(psram: PSRAM<'static>) -> Psram {
    let psram = Psram::new(psram, psram_config());
    if psram.raw_parts().1 > 0 {
        esp_alloc::psram_allocator!(&psram);
    }
    psram
}

/// The card on SDMMC slot 1 (CLK 41, CMD 42, DAT0 40, `enable` its supply) and the font on it
/// (`FONT`, loaded whole into PSRAM, its interval table in `table`); `None` without a card, the
/// font or the PSRAM's room. `feed` runs before each try at the card, which a card that stalls can
/// make long. The card is not read again after.
pub fn open_font<'t, 'd>(
    controller: &'d SdHostController<'_>,
    pins: (GPIO41<'d>, GPIO42<'d>, GPIO40<'d>),
    enable: &mut Output<'_>,
    table: &'t mut [u8],
    mut feed: impl FnMut(),
) -> Option<Font<'t>> {
    let (clk, cmd, dat0) = pins;
    let slot = controller
        .slot::<1>(SlotConfig::default())
        .ok()?
        .with_clk(clk)
        .with_cmd(cmd)
        .with_data0(dat0);
    let mut card: Card<'d> = Card::new_uninit(BlockingBus::new(slot), SpinDelay(Delay::new()));
    sd::mount(&mut card, enable, |_| feed())?;
    let mut volume = ready(Volume::mount(CardBlocks::new(card)))?.ok()?;
    let start = Instant::now();
    let bytes = ready(load_whole(&mut volume, FONT, ExternalMemory))?.ok()?;
    let load_ms = start.elapsed().as_millis();
    let writer = Writer::open(&mut Memory::new(&bytes), 0, table).ok()?;
    Some(Font {
        bytes,
        writer,
        load_ms,
    })
}
