//! The updater's parts on the host: a card in memory, the slot in memory, SHA-256 from `sha2`, the
//! lines kept, and one run of the updater over them.

use crate::releases::{RECOVERY_FILE, Release};
use crate::{Card, Done, Hashes, Notes, Updater};
use core::fmt::Arguments;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use purestwhite_fat::{BLOCK, Block, BlockDevice, BlockWrite, Volume};
use purestwhite_ota::image::DIGEST;
use purestwhite_ota::testing::{FakeError, FakeFlash, Sha};
use std::collections::BTreeSet;
use std::format;
use std::process::Command;
use std::string::{String, ToString};
use std::sync::atomic::{AtomicU32, Ordering};
use std::vec::Vec;

/// Runs a future of the fakes below, which are always ready.
pub fn block_on<F: Future>(future: F) -> F::Output {
    match pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => unreachable!("the fakes are always ready"),
    }
}

/// A card's blocks in memory. Once opened by the updater it counts its reads, can stop answering
/// (`stall_after`), and can change under the updater (`then`).
#[derive(Default)]
pub struct Disk {
    pub bytes: Vec<u8>,
    /// Served from the second read of a block where it differs on: the card changed between the
    /// passes.
    pub then: Option<Vec<u8>>,
    seen: BTreeSet<u32>,
    /// The reads since the card was opened.
    pub reads: usize,
    /// The reads after which a read never finishes, counted from the card's opening.
    pub stall_after: Option<usize>,
}

impl Disk {
    fn new(bytes: Vec<u8>) -> Self {
        Disk {
            bytes,
            ..Disk::default()
        }
    }

    /// What `start` and the blocks after it hold now: `then` once a block that differs there is
    /// read a second time (the release's, in the second pass).
    fn serve(&mut self, start: u32, count: u32) -> &[u8] {
        if let Some(then) = &self.then {
            let differs = |b: &u32| {
                let at = *b as usize * BLOCK;
                self.bytes[at..at + BLOCK] != then[at..at + BLOCK]
            };
            let blocks: Vec<u32> = (start..start + count).filter(differs).collect();
            if blocks.iter().any(|b| self.seen.contains(b)) {
                self.bytes = self.then.take().unwrap();
            }
            self.seen.extend(blocks);
        }
        let at = start as usize * BLOCK;
        &self.bytes[at..at + count as usize * BLOCK]
    }
}

impl BlockDevice for Disk {
    type Error = ();

    async fn read(&mut self, start: u32, blocks: &mut [Block]) -> Result<(), ()> {
        if self.stall_after.is_some_and(|n| self.reads >= n) {
            core::future::pending::<()>().await;
        }
        self.reads += 1;
        let from = self.serve(start, blocks.len() as u32);
        blocks.as_flattened_mut().copy_from_slice(from);
        Ok(())
    }
}

impl BlockWrite for Disk {
    #[allow(clippy::unused_async_trait_impl)]
    async fn write(&mut self, start: u32, blocks: &[Block]) -> Result<(), ()> {
        let at = start as usize * BLOCK;
        let to = self
            .bytes
            .get_mut(at..at + blocks.len() * BLOCK)
            .ok_or(())?;
        to.copy_from_slice(blocks.as_flattened());
        Ok(())
    }
}

/// Why the test card could not be opened.
#[derive(Debug)]
pub enum NoCard {
    /// No card in the slot.
    Missing,
    /// No FAT volume on it.
    Unformatted,
}

/// The card as the updater opens it: a FAT volume in memory, or none.
pub struct TestCard(pub Option<Disk>);

impl Card for TestCard {
    type Blocks<'c> = &'c mut Disk;
    type Error = NoCard;

    fn open(&mut self, feed: &mut dyn FnMut()) -> Result<Volume<&mut Disk>, NoCard> {
        feed();
        let disk = self.0.as_mut().ok_or(NoCard::Missing)?;
        let stall_after = disk.stall_after.take();
        let mut volume = block_on(Volume::mount(disk)).map_err(|_| NoCard::Unformatted)?;
        let disk = volume.device_mut();
        (disk.stall_after, disk.reads) = (stall_after, 0);
        disk.seen.clear();
        Ok(volume)
    }
}

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A FAT16 card of 4 MiB from mkfs.fat, holding `release` as the recovery file when given;
/// `None` when mkfs.fat is not there and `PW_FAT_SKIP_TOOLS` is set (the test then passes).
pub fn card_with(release: Option<&[u8]>) -> Option<TestCard> {
    let disk = formatted()?;
    let Some(bytes) = release else {
        return Some(TestCard(Some(disk)));
    };
    let mut volume = block_on(Volume::mount(disk)).unwrap();
    let mut file = block_on(volume.create(RECOVERY_FILE)).unwrap();
    assert_eq!(
        block_on(volume.write(&mut file, bytes)).unwrap(),
        bytes.len()
    );
    block_on(volume.close(file)).unwrap();
    Some(TestCard(Some(volume.into_inner())))
}

/// An empty FAT16 volume of 4 MiB, 512-byte clusters, made by mkfs.fat.
fn formatted() -> Option<Disk> {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("pw-updater-{}-{n}.img", std::process::id()));
    let made = Command::new("mkfs.fat")
        .args([
            "-C", "-F", "16", "-s", "1", "-n", "PWTEST", "-i", "1234ABCD",
        ])
        .arg(&path)
        .arg("4096")
        .output();
    let Ok(out) = made else {
        assert!(
            std::env::var_os("PW_FAT_SKIP_TOOLS").is_some(),
            "mkfs.fat is required (PW_FAT_SKIP_TOOLS=1 skips these tests)"
        );
        std::eprintln!("skipped: mkfs.fat is not installed");
        return None;
    };
    let bytes = std::fs::read(&path);
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "mkfs.fat: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(Disk::new(bytes.unwrap()))
}

/// The C++ firmware's slot, of `super::SLOT` bytes, holding `image` (and old bytes after it).
pub fn slot_with(image: &[u8]) -> FakeFlash {
    let mut flash = FakeFlash::new(super::SLOT);
    flash.mem[..image.len()].copy_from_slice(image);
    flash
}

/// The digest appended to `image`.
pub fn digest(image: &[u8]) -> [u8; DIGEST] {
    image[image.len() - DIGEST..].try_into().unwrap()
}

/// The line the updater notes for `image` found in `place` and named `name` (`slot` or
/// `card`), its time 0 ms (the tests' clock stands still).
pub fn line(place: &str, image: &[u8], name: &str) -> String {
    format!("{place} {} B 0 ms {}", image.len(), named(image, name))
}

/// `image` as the updater names it: its digest's first four bytes, then `name`.
pub fn named(image: &[u8], name: &str) -> String {
    let d = digest(image);
    format!("{:02x}{:02x}{:02x}{:02x} {name}", d[0], d[1], d[2], d[3])
}

/// SHA-256 from `sha2`, started anew for each digest.
struct Shas;

impl Hashes for Shas {
    type Sha<'s> = Sha;

    fn start(&mut self) -> Sha {
        Sha::default()
    }
}

/// The lines noted.
#[derive(Default)]
struct Lines(Vec<String>);

impl Notes for Lines {
    fn note(&mut self, line: Arguments<'_>) {
        self.0.push(line.to_string());
    }
}

/// A slot whose reads all fail.
pub struct Unreadable(pub FakeFlash);

impl ErrorType for Unreadable {
    type Error = FakeError;
}

impl ReadNorFlash for Unreadable {
    const READ_SIZE: usize = 1;

    fn read(&mut self, _: u32, _: &mut [u8]) -> Result<(), FakeError> {
        Err(FakeError)
    }

    fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

impl NorFlash for Unreadable {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = 4096;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), FakeError> {
        self.0.erase(from, to)
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), FakeError> {
        self.0.write(offset, bytes)
    }
}

/// What one run of the updater gave.
pub struct Run<F> {
    pub done: Done,
    pub lines: Vec<String>,
    pub flash: F,
    /// How many times the watchdog was fed.
    pub feeds: usize,
    pub card: TestCard,
}

/// The updater run once over `flash` and `card`, knowing the releases `known`.
pub fn run<F: NorFlash>(
    mut flash: F,
    mut card: TestCard,
    known: &[Release],
    fell_back: bool,
) -> Run<F> {
    let mut notes = Lines::default();
    let mut feeds = 0;
    let done = Updater {
        known,
        hashes: &mut Shas,
        notes: &mut notes,
        feed: || feeds += 1,
        now_ms: || 0,
    }
    .update(&mut flash, &mut card, fell_back);
    Run {
        done,
        lines: notes.0,
        flash,
        feeds,
        card,
    }
}
