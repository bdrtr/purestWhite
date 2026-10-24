//! Exports for lib/Epub's ImageDimsProbe (see `rust/include/bembeyaz/image.h`).

use bembeyaz_image::Probe;
use core::mem::MaybeUninit;

// image.h's `bb_image_probe`, opaque storage for a `Probe`; ImageDimsProbe.h checks its size too.
const _: () = assert!(
    size_of::<Probe>() <= 7 * 4 && align_of::<Probe>() <= 4 && !core::mem::needs_drop::<Probe>()
);

/// # Safety
/// `probe` is NULL or a `bb_image_probe`, of any contents.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_image_probe_init(probe: *mut MaybeUninit<Probe>) {
    // SAFETY: as documented.
    if let Some(slot) = unsafe { probe.as_mut() } {
        slot.write(Probe::default());
    }
}

/// # Safety
/// `probe` is NULL or a `bb_image_probe` that `bb_image_probe_init` set up, used by nothing else
/// during the call; `data` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_image_probe_feed(
    probe: *mut Probe,
    data: *const u8,
    len: usize,
) -> usize {
    // SAFETY: as documented.
    let Some(p) = (unsafe { probe.as_mut() }) else {
        return 0;
    };
    // SAFETY: as documented.
    p.feed(unsafe { crate::view(data, len) })
}

/// # Safety
/// `probe` is NULL or a `bb_image_probe` that `bb_image_probe_init` set up; `width` and `height`
/// are writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_image_probe_size(
    probe: *const Probe,
    width: *mut u32,
    height: *mut u32,
) -> bool {
    // SAFETY: as documented.
    let Some((w, h)) = (unsafe { probe.as_ref() }).and_then(Probe::size) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe {
        width.write(w);
        height.write(h);
    }
    true
}
