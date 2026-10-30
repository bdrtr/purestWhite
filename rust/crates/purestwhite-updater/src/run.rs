//! The updater's steps, each noted: the slot identified, the card opened and its release
//! identified, the plan, the write, and the slot identified once more.

use crate::images::{SlotError, identify_file, identify_slot, install_file};
use crate::releases::{Named, RECOVERY_FILE, Release, release};
use crate::{Card, Hashes, Notes, ready};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use purestwhite_fat::{BlockDevice, Volume};
use purestwhite_ota::digest::Identified;
use purestwhite_ota::plan::{InSlot, Plan, plan, resets};
use purestwhite_ota::slot::SECTOR;

/// What the updater borrows from the run.
pub struct Updater<'a, H, N, W> {
    /// The releases it may install (phase 2's limit), which also name the images it notes.
    pub known: &'a [Release],
    pub hashes: &'a mut H,
    pub notes: &'a mut N,
    /// Feeds the watchdog; run between pieces of work.
    pub feed: W,
    /// Milliseconds from any start, for the times noted.
    pub now_ms: fn() -> u64,
}

/// What the C++ firmware's slot holds once the updater is done, and whether the updater wrote
/// the card's release there (it reads back as that release).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Done {
    pub after: InSlot,
    pub wrote: bool,
}

impl Done {
    /// Whether the firmware resets into the slot, or waits (`purestwhite_ota::plan::resets`).
    #[must_use]
    pub fn resets(&self, fell_back: bool) -> bool {
        resets(self.after, self.wrote, fell_back)
    }
}

/// A slot's image, identified where it lies.
type Found<E> = Result<Identified, SlotError<E>>;

impl<H: Hashes, N: Notes, W: FnMut()> Updater<'_, H, N, W> {
    /// The updater's steps, noted: the card's release written into `slot` (the C++ firmware's)
    /// when the plan allows it. `fell_back`: the bootloader refused that slot and started the
    /// image that runs the updater. Nothing is written before both are identified.
    pub fn update<F: NorFlash, C: Card>(
        &mut self,
        slot: &mut F,
        card: &mut C,
        fell_back: bool,
    ) -> Done {
        let in_slot = self.slot_state(slot);
        let kept = Done {
            after: in_slot,
            wrote: false,
        };
        let mut volume = match card.open(&mut self.feed) {
            Ok(volume) => volume,
            Err(e) => {
                self.notes.note(format_args!("card: {e:?}"));
                return kept;
            }
        };
        let Some(image) = self.card_release(&mut volume, slot.capacity()) else {
            return kept;
        };
        let known = release(self.known, &image.digest).is_some();
        match plan(&image.digest, known, in_slot, fell_back) {
            Plan::Refuse(why) => {
                self.notes.note(format_args!("refused: {why:?}"));
                kept
            }
            plan => {
                self.write(&mut volume, slot, &image, plan);
                let after = self.installed(slot, &image);
                Done {
                    after,
                    wrote: after == InSlot::Image(image.digest),
                }
            }
        }
    }

    /// The milliseconds since `start` (`now_ms`).
    fn since(&self, start: u64) -> u64 {
        (self.now_ms)().saturating_sub(start)
    }

    /// The image in `slot`, measured and hashed where it lies, and how long that took in ms.
    fn identify<F: ReadNorFlash>(&mut self, slot: &mut F) -> (Found<F::Error>, u64) {
        let start = (self.now_ms)();
        let found = identify_slot(slot, self.hashes.start(), &mut self.feed);
        (found, self.since(start))
    }

    /// What `slot` (the C++ firmware's) holds, noted.
    fn slot_state<F: ReadNorFlash>(&mut self, slot: &mut F) -> InSlot {
        let (found, ms) = self.identify(slot);
        match &found {
            Ok(image) => {
                let (len, named) = (image.checked.len, Named(&image.digest, self.known));
                self.notes
                    .note(format_args!("slot {len} B {ms} ms {named}"));
            }
            Err(SlotError::Flash(at, e)) => {
                self.notes.note(format_args!("slot: read at {at:x}: {e:?}"));
            }
            Err(e) => self.notes.note(format_args!("slot: {e:?}")),
        }
        in_slot(&found)
    }

    /// The release on the card, at most `limit` bytes (the slot it would go to), checked and
    /// hashed as it streams in (the first pass), noted.
    fn card_release<D: BlockDevice>(
        &mut self,
        volume: &mut Volume<D>,
        limit: usize,
    ) -> Option<Identified> {
        let start = (self.now_ms)();
        let sha = self.hashes.start();
        let found = ready(identify_file(
            volume,
            RECOVERY_FILE,
            limit,
            sha,
            &mut self.feed,
        ));
        let ms = self.since(start);
        match found {
            Some(Ok(image)) => {
                let (len, named) = (image.checked.len, Named(&image.digest, self.known));
                self.notes
                    .note(format_args!("card {len} B {ms} ms {named}"));
                Some(image)
            }
            Some(Err(e)) => {
                self.notes.note(format_args!("card: {e:?}"));
                None
            }
            None => {
                self.notes.note(format_args!("card: not ready"));
                None
            }
        }
    }

    /// Writes the card's release, as the first pass found it (`image`), into `slot` (the second
    /// pass), noting how it went after `plan`'s name.
    fn write<D: BlockDevice, F: NorFlash>(
        &mut self,
        volume: &mut Volume<D>,
        slot: &mut F,
        image: &Identified,
        plan: Plan,
    ) {
        let start = (self.now_ms)();
        let sha = self.hashes.start();
        let written = ready(install_file(
            volume,
            RECOVERY_FILE,
            slot,
            &image.digest,
            sha,
            &mut self.feed,
        ));
        let ms = self.since(start);
        let sectors = image.checked.len.div_ceil(SECTOR);
        match written {
            Some(Ok(_)) => self
                .notes
                .note(format_args!("{plan:?}: {sectors} sectors {ms} ms")),
            Some(Err(e)) => self.notes.note(format_args!("{plan:?}: {e:?}")),
            None => self.notes.note(format_args!("{plan:?}: not ready")),
        }
    }

    /// What `slot` holds after the write: noted when it is an image that passes (`installed: same
    /// digest` for the card's release); otherwise the write's own line says why.
    fn installed<F: ReadNorFlash>(&mut self, slot: &mut F, image: &Identified) -> InSlot {
        let (found, _) = self.identify(slot);
        match &found {
            Ok(now) if now.digest == image.digest => {
                self.notes.note(format_args!("installed: same digest"));
            }
            Ok(now) => self.notes.note(format_args!(
                "installed: {}",
                Named(&now.digest, self.known)
            )),
            Err(_) => {}
        }
        in_slot(&found)
    }
}

/// What a slot holds, as `found` says.
fn in_slot<E>(found: &Found<E>) -> InSlot {
    match found {
        Ok(image) => InSlot::Image(image.digest),
        Err(SlotError::Flash(..)) => InSlot::Unreadable,
        Err(_) => InSlot::Broken,
    }
}
