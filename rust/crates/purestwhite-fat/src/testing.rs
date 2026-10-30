//! A card in memory, for the tests and the host simulator of the crates built on this one (the
//! `testing` feature): its blocks in a `Vec`, an empty volume made by the host's mkfs.fat, and a
//! runner for futures that are always ready, as the card's here are.

extern crate std;

use crate::{BLOCK, Block, BlockDevice, BlockWrite};
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::format;
use std::process::Command;
use std::string::{String, ToString};
use std::sync::atomic::{AtomicU32, Ordering};
use std::vec::Vec;

/// A card's blocks in memory; a block past its end fails.
#[derive(Clone, Debug, Default)]
pub struct MemCard(pub Vec<u8>);

impl BlockDevice for MemCard {
    type Error = ();

    #[allow(clippy::unused_async_trait_impl, reason = "in memory: ready at once")]
    async fn read(&mut self, start: u32, blocks: &mut [Block]) -> Result<(), ()> {
        let at = start as usize * BLOCK;
        let from = self.0.get(at..at + blocks.len() * BLOCK).ok_or(())?;
        blocks.as_flattened_mut().copy_from_slice(from);
        Ok(())
    }
}

impl BlockWrite for MemCard {
    #[allow(clippy::unused_async_trait_impl, reason = "in memory: ready at once")]
    async fn write(&mut self, start: u32, blocks: &[Block]) -> Result<(), ()> {
        let at = start as usize * BLOCK;
        let to = self.0.get_mut(at..at + blocks.len() * BLOCK).ok_or(())?;
        to.copy_from_slice(blocks.as_flattened());
        Ok(())
    }
}

/// The output of a future that is ready when first polled (a `MemCard`'s); `None` if it is not.
pub fn ready<F: Future>(future: F) -> Option<F::Output> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

static NEXT: AtomicU32 = AtomicU32::new(0);

/// Why mkfs.fat gave no volume.
#[derive(Debug)]
pub enum FormatError {
    /// mkfs.fat is not installed.
    NoTool,
    /// It failed, saying this.
    Failed(String),
}

/// An empty FAT16 volume of `kib` KiB (at least 2100, for FAT16's clusters) with 512-byte
/// clusters, made by the host's mkfs.fat with a fixed serial, so the same every time.
///
/// # Errors
/// mkfs.fat is not there, or fails.
pub fn format(kib: u32) -> Result<MemCard, FormatError> {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("pw-fat-card-{}-{n}.img", std::process::id()));
    let made = Command::new("mkfs.fat")
        .args([
            "-C", "-F", "16", "-s", "1", "-n", "PWTEST", "-i", "1234ABCD",
        ])
        .arg(&path)
        .arg(kib.to_string())
        .output()
        .map_err(|_| FormatError::NoTool)?;
    let bytes = std::fs::read(&path);
    let _ = std::fs::remove_file(&path);
    if !made.status.success() {
        return Err(FormatError::Failed(
            String::from_utf8_lossy(&made.stderr).into_owned(),
        ));
    }
    bytes
        .map(MemCard)
        .map_err(|e| FormatError::Failed(e.to_string()))
}

/// `format(kib)` for a test: `None` when mkfs.fat is not there and `PW_FAT_SKIP_TOOLS` is set (the
/// test then says so and passes, as this crate's own tests do).
///
/// # Panics
/// mkfs.fat is not there and `PW_FAT_SKIP_TOOLS` is not set, or mkfs.fat fails.
#[allow(clippy::panic, reason = "for tests: a missing tool fails the test")]
#[must_use]
pub fn test_card(kib: u32) -> Option<MemCard> {
    match format(kib) {
        Ok(card) => Some(card),
        Err(FormatError::NoTool) if std::env::var_os("PW_FAT_SKIP_TOOLS").is_some() => {
            std::eprintln!("skipped: mkfs.fat is not installed");
            None
        }
        Err(FormatError::NoTool) => {
            panic!("mkfs.fat is required (PW_FAT_SKIP_TOOLS=1 skips these tests)")
        }
        Err(FormatError::Failed(why)) => panic!("mkfs.fat: {why}"),
    }
}
