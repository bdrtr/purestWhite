//! The storage loop: the application's jobs on the card (`purestwhite_app::Job`), each answered
//! with an event, and the page buffer it reads pages into for the drawing loop to copy.

use crate::Frame;
use crate::pages::{count_pages, load_page};
use core::cell::Cell;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{Channel, DynamicSender};
use purestwhite_app::{Event, Job};
use purestwhite_fat::{BlockDevice, Volume};
use purestwhite_render::FRAME_BYTES;

/// Jobs waiting for the storage loop.
const JOBS: usize = 2;

/// The application's jobs on their way to the storage loop, and how many are not answered yet.
pub struct Jobs<M: RawMutex> {
    queue: Channel<M, Job, JOBS>,
    open: Mutex<M, Cell<u8>>,
}

impl<M: RawMutex> Jobs<M> {
    #[must_use]
    pub const fn new() -> Self {
        Jobs {
            queue: Channel::new(),
            open: Mutex::new(Cell::new(0)),
        }
    }

    /// Gives `job` to the storage loop; never waits. The application keeps one job open at most
    /// (it waits for each answer before the next), so there is room; a job that found none would
    /// be dropped, and the application would wait for its answer.
    pub fn give(&self, job: Job) {
        if self.queue.try_send(job).is_ok() {
            self.open
                .lock(|open| open.set(open.get().saturating_add(1)));
        }
    }

    /// Whether every job given is answered.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.open.lock(Cell::get) == 0
    }

    fn answered(&self) {
        self.open
            .lock(|open| open.set(open.get().saturating_sub(1)));
    }
}

impl<M: RawMutex> Default for Jobs<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// The page buffer: the storage loop reads a page into it, the drawing loop copies it into a
/// frame, each holding it only while it works. The application keeps them apart: it asks for the
/// next page only once the panel has answered for every view (`purestwhite_app::PAGE_MS`).
pub struct PageSlot<M: RawMutex> {
    page: Mutex<M, Cell<Option<Frame>>>,
}

impl<M: RawMutex> PageSlot<M> {
    #[must_use]
    pub const fn new() -> Self {
        PageSlot {
            page: Mutex::new(Cell::new(None)),
        }
    }

    /// Gives the page buffer, once, before the loops start; false if one was there.
    pub fn give(&self, page: Frame) -> bool {
        self.page.lock(|slot| {
            let there = slot.take();
            let given = there.is_none();
            slot.set(there.or(Some(page)));
            given
        })
    }

    /// Copies the page into `frame`; false, and nothing copied, when the storage loop holds it
    /// or none was given.
    pub fn copy_into(&self, frame: &mut [u8; FRAME_BYTES]) -> bool {
        let Some(page) = self.take() else {
            return false;
        };
        frame.copy_from_slice(&page[..]);
        self.put(page);
        true
    }

    fn take(&self) -> Option<Frame> {
        self.page.lock(Cell::take)
    }

    fn put(&self, page: Frame) {
        self.page.lock(|slot| slot.set(Some(page)));
    }
}

impl<M: RawMutex> Default for PageSlot<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// The ends of the channels the storage loop uses.
pub struct StorageLinks<'a, M: RawMutex> {
    pub jobs: &'a Jobs<M>,
    pub page: &'a PageSlot<M>,
    /// The answers, to the application.
    pub events: DynamicSender<'a, Event>,
}

/// Runs the application's jobs on `volume`, each answered with an event; without a card
/// (`None`), every job finds nothing.
pub async fn run_storage<M: RawMutex, D: BlockDevice>(
    mut volume: Option<Volume<D>>,
    links: StorageLinks<'_, M>,
) -> ! {
    loop {
        let job = links.jobs.queue.receive().await;
        let answer = match (volume.as_mut(), job) {
            (Some(volume), Job::CountPages) => Event::Pages(count_pages(volume).await),
            (Some(volume), Job::LoadPage(n)) => load(volume, n, links.page).await,
            (None, Job::CountPages) => Event::Pages(0),
            (None, Job::LoadPage(n)) => Event::PageFailed(n),
        };
        links.events.send(answer).await;
        links.jobs.answered();
    }
}

/// Page `n` read into the page buffer, as the event that answers for it.
async fn load<M: RawMutex, D: BlockDevice>(
    volume: &mut Volume<D>,
    n: u16,
    slot: &PageSlot<M>,
) -> Event {
    let Some(page) = slot.take() else {
        return Event::PageFailed(n);
    };
    let loaded = load_page(volume, n, page).await;
    slot.put(page);
    match loaded {
        Ok(()) => Event::PageLoaded(n),
        Err(_) => Event::PageFailed(n),
    }
}
