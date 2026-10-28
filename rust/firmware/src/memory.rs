//! The PSRAM as a heap of its own, for the Embassy main (DESIGN.md, Memory): never the global
//! allocator, so that a full internal heap fails instead of spilling into PSRAM, and what lives
//! in PSRAM is chosen. It holds the panel's frames now; fonts, pages and images later.

use allocator_api2::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use esp_alloc::{EspHeap, HeapRegion, MemoryCapability};
use esp_hal::psram::Psram;
use purestwhite_render::FRAME_BYTES;
use purestwhite_services::Frame;

/// The PSRAM's heap, empty until `add_psram`.
pub static PSRAM: EspHeap = EspHeap::empty();

/// Whether `add_psram` has run.
static ADDED: AtomicBool = AtomicBool::new(false);

/// Gives `psram` (set up with `crate::board::psram_config`) to `PSRAM`, once: how many bytes it
/// has, 0 without PSRAM or on a second call. Keep `psram` for the run, and give it to no other
/// heap: the global one must not have it (no `esp_alloc::psram_allocator!` in the same image).
pub fn add_psram(psram: &Psram) -> usize {
    let (start, size) = psram.raw_parts();
    if size == 0 || ADDED.swap(true, Ordering::AcqRel) {
        return 0;
    }
    // SAFETY: `raw_parts` is the PSRAM mapped into the address space for the run (`psram` is
    // kept), and nothing else allocates from it: `ADDED` gives it to this heap once, and the
    // caller gives it to no other.
    unsafe {
        PSRAM.add_region(HeapRegion::new(
            start,
            size,
            MemoryCapability::External.into(),
        ));
    }
    size
}

/// A white frame of the panel in `PSRAM`, kept for the run; `None` without room. It is
/// allocated as a slice and then taken as an array: a boxed array would be built on the stack
/// first (48 KB).
#[must_use]
pub fn psram_frame() -> Option<Frame> {
    let mut bytes = Vec::new_in(&PSRAM);
    bytes.try_reserve_exact(FRAME_BYTES).ok()?;
    bytes.resize(FRAME_BYTES, 0xFF);
    bytes.leak().try_into().ok()
}
