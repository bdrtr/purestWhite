//! The card task: the SD card mounted over the interrupt-driven bus (`sd::AsyncBus`), as
//! tasks-check mounted it on the device, and the services' storage loop run on its volume (the
//! battery measurement's pages). Asked to rest, it hands the card's supply back to the power
//! loop, which turns the card off for the sleep. Without the card the loop still answers, with
//! nothing found.

use crate::tasks::{LINKS, Mutex};
use alloc::boxed::Box;
use core::sync::atomic::{AtomicU8, Ordering};
use embassy_executor::Spawner;
use embassy_sync::signal::Signal;
use embassy_time::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::sdmmc::{Config as SdConfig, SdHostController, SlotConfig};
use purestwhite_fat::Volume;
use purestwhite_firmware::board::CardParts;
use purestwhite_firmware::sd::{self, AsyncBus, AsyncCard, CardBlocks};
use purestwhite_services::run_storage;

/// The card's supply (active low), once the card task rests.
pub static CARD_RESTED: Signal<Mutex, Output<'static>> = Signal::new();

/// How the card came up: `PENDING` until the task knows, then the try that mounted it (1 to
/// `sd::TRIES`), `NO_SLOT`, `NOT_MOUNTED` or `NO_VOLUME`.
static CARD: AtomicU8 = AtomicU8::new(PENDING);
const PENDING: u8 = 0;
const NO_SLOT: u8 = 0xFD;
const NO_VOLUME: u8 = 0xFE;
const NOT_MOUNTED: u8 = 0xFF;

/// The card's report line once the task knows how the card came up: `card: try 1`,
/// `card: no slot` (the controller gave none), `card: not mounted` or `card: no volume`.
pub fn line(out: &mut impl core::fmt::Write) -> bool {
    let _ = match CARD.load(Ordering::Acquire) {
        PENDING => return false,
        NO_SLOT => writeln!(out, "card: no slot"),
        NOT_MOUNTED => writeln!(out, "card: not mounted"),
        NO_VOLUME => writeln!(out, "card: no volume"),
        attempt => writeln!(out, "card: try {attempt}"),
    };
    true
}

/// The card's volume over the interrupt-driven bus.
type CardVolume = Volume<CardBlocks<AsyncBus<'static>, Delay>>;

#[embassy_executor::task]
#[allow(
    clippy::large_futures,
    reason = "the task's future is its static storage, in internal RAM, which the card's DMA needs"
)]
async fn storage(card: Option<AsyncCard<'static>>, mut enable: Output<'static>) {
    let volume = if let Some(card) = card {
        mount(card, &mut enable).await
    } else {
        CARD.store(NO_SLOT, Ordering::Release);
        None
    };
    run_storage(volume, LINKS.storage_links()).await;
    CARD_RESTED.signal(enable);
}

/// The card mounted as the C++ firmware mounts it (`sd::mount_with`, `enable` its supply) and its
/// FAT volume, how it went noted in `CARD`; `None` when either fails.
#[allow(
    clippy::large_futures,
    reason = "as `storage`'s, whose future holds this one"
)]
async fn mount(mut card: AsyncCard<'static>, enable: &mut Output<'static>) -> Option<CardVolume> {
    let Some(attempt) = sd::mount_with(&mut card, enable, &mut Delay, |_| {}).await else {
        CARD.store(NOT_MOUNTED, Ordering::Release);
        return None;
    };
    let volume = Volume::mount(CardBlocks::new(card)).await.ok();
    let noted = if volume.is_some() { attempt } else { NO_VOLUME };
    CARD.store(noted, Ordering::Release);
    volume
}

/// Spawns the card task on SDMMC slot 1, the card off until its first try; without the slot, the
/// storage loop runs with no card, and answers every job with nothing found.
pub fn spawn(spawner: Spawner, card: CardParts) {
    let enable = Output::new(card.supply, Level::High, OutputConfig::default());
    // The slot borrows the controller, which lives as long as the image, on the heap.
    let slot = SdHostController::new(card.sdhost, SdConfig::default())
        .ok()
        .map(|controller| &*Box::leak(Box::new(controller)))
        .and_then(|controller| controller.slot::<1>(SlotConfig::default()).ok());
    let card = slot.map(|slot| {
        let slot = slot
            .with_clk(card.clk)
            .with_cmd(card.cmd)
            .with_data0(card.dat0)
            .into_async();
        AsyncCard::new_uninit(AsyncBus::new(slot), Delay)
    });
    spawner.spawn(storage(card, enable).unwrap());
}
