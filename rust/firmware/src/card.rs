//! Files read off the SD card in one session over the blocking bus, before the scheduler starts:
//! the card powered, its volume opened, each file loaded whole (`storage::load_at_most`) onto the
//! global heap, and the card powered off again. For the test images that read the C++ firmware's
//! saved settings.

use crate::board::CardParts;
use crate::ready;
use crate::sd;
use crate::storage::{LoadError, load_at_most};
use allocator_api2::alloc::Global;
use allocator_api2::vec::Vec;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{GPIO40, GPIO41, GPIO42, SDHOST};
use esp_hal::sdmmc::{Config as SdConfig, SdHostController};

/// Why a file was not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileError {
    /// It is not on the card.
    Missing,
    /// It could not be looked up or read whole (the card failed, or its volume contradicts itself).
    Unreadable,
    /// It is larger than asked for.
    TooLarge,
    /// The heap has no room for it.
    Memory,
}

impl FileError {
    /// Its line in a report: `no file`, `unreadable`, `file too large` or `no memory`.
    #[must_use]
    pub fn note(self) -> &'static str {
        match self {
            FileError::Missing => "no file",
            FileError::Unreadable => "unreadable",
            FileError::TooLarge => "file too large",
            FileError::Memory => "no memory",
        }
    }
}

/// A file's bytes, or why there are none.
pub type File = Result<Vec<u8, Global>, FileError>;

/// The card could not be read at all: no slot, no card or no volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoCard;

/// Each of `files` (a path, and the most bytes it may take) read in one session. `feed` runs
/// before each try at the card (a watchdog).
///
/// # Errors
/// [`NoCard`]: the slot, the card or its volume could not be had.
pub fn read_files<const N: usize>(
    card: CardParts,
    files: [(&str, usize); N],
    feed: impl FnMut(),
) -> Result<[File; N], NoCard> {
    let mut supply = Output::new(card.supply, Level::High, OutputConfig::default());
    let pins = (card.clk, card.cmd, card.dat0);
    let read = load(&mut supply, card.sdhost, pins, files, feed);
    supply.set_high();
    read
}

fn load<const N: usize>(
    supply: &mut Output<'_>,
    sdhost: SDHOST<'static>,
    pins: (GPIO41<'static>, GPIO42<'static>, GPIO40<'static>),
    files: [(&str, usize); N],
    mut feed: impl FnMut(),
) -> Result<[File; N], NoCard> {
    let controller = SdHostController::new(sdhost, SdConfig::default()).map_err(|_| NoCard)?;
    let mut volume = sd::open_volume(&controller, pins, supply, |_| feed()).map_err(|_| NoCard)?;
    Ok(files.map(
        |(path, most)| match ready(load_at_most(&mut volume, path, most, Global)) {
            Some(Ok(bytes)) => Ok(bytes),
            Some(Err(LoadError::File(purestwhite_fat::Error::NotFound))) => Err(FileError::Missing),
            Some(Err(LoadError::TooLarge)) => Err(FileError::TooLarge),
            Some(Err(LoadError::Memory)) => Err(FileError::Memory),
            Some(Err(LoadError::File(_) | LoadError::Read)) | None => Err(FileError::Unreadable),
        },
    ))
}
