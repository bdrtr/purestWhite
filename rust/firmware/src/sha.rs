//! The chip's SHA accelerator as `purestwhite_ota::digest::Sha256`, for checking firmware images,
//! and as the SD updater's `purestwhite_updater::Hashes`. A digest dropped unfinished is finished
//! then: the unit could otherwise still be busy with it when the next starts.

use core::convert::Infallible;
use esp_hal::sha::{Sha, Sha256 as Algorithm, ShaDigest};
use purestwhite_ota::digest::Sha256;
use purestwhite_ota::image::DIGEST;
use purestwhite_updater::Hashes;

/// A SHA-256 on the accelerator, borrowed for as long as it is computed.
pub struct HwSha<'a, 'd>(Option<ShaDigest<'d, Algorithm, &'a mut Sha<'d>>>);

impl<'a, 'd> HwSha<'a, 'd> {
    #[must_use]
    pub fn new(sha: &'a mut Sha<'d>) -> Self {
        HwSha(Some(sha.start::<Algorithm>()))
    }
}

/// The accelerator lent to the SD updater, which starts one digest on it at a time.
pub struct ShaUnit<'a, 'd>(pub &'a mut Sha<'d>);

impl<'d> Hashes for ShaUnit<'_, 'd> {
    type Sha<'s>
        = HwSha<'s, 'd>
    where
        Self: 's;

    fn start(&mut self) -> HwSha<'_, 'd> {
        HwSha::new(self.0)
    }
}

/// `op`'s result once the accelerator is no longer busy.
fn when_ready<T>(mut op: impl FnMut() -> nb::Result<T, Infallible>) -> T {
    loop {
        match op() {
            Ok(value) => return value,
            Err(nb::Error::WouldBlock) => {}
            Err(nb::Error::Other(never)) => match never {},
        }
    }
}

/// The digest `digest` gives, waiting for the accelerator.
fn finish<'d>(digest: &mut ShaDigest<'d, Algorithm, &mut Sha<'d>>) -> [u8; DIGEST] {
    let mut out = [0u8; DIGEST];
    when_ready(|| digest.finish(&mut out));
    out
}

impl Sha256 for HwSha<'_, '_> {
    fn update(&mut self, mut data: &[u8]) {
        if let Some(digest) = self.0.as_mut() {
            while !data.is_empty() {
                data = when_ready(|| digest.update(data));
            }
        }
    }

    fn finish(mut self) -> [u8; DIGEST] {
        self.0.take().as_mut().map_or([0; DIGEST], finish)
    }
}

impl Drop for HwSha<'_, '_> {
    fn drop(&mut self) {
        if let Some(digest) = self.0.as_mut() {
            finish(digest);
        }
    }
}
