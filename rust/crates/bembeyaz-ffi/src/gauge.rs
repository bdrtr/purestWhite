//! Exports for lib/hal's HalPowerManager (see `rust/include/purestwhite/gauge.h`).

use crate::i2c::{Bus, CBus, Delay, block_on};
use purestwhite_gauge::Cw2017;

// gauge.h's opaque `pw_cw2017` holds a `Cw2017`; all zero bytes are `Cw2017::new()` (false and 0).
const _: () = assert!(size_of::<Cw2017>() <= 8 && align_of::<Cw2017>() <= 4);

/// # Safety
/// `gauge` is NULL or an aligned `pw_cw2017` that nothing else uses during the call, zeroed
/// before its first use and since then changed only by this call; `bus` is NULL or a `pw_i2c`
/// whose callbacks may be called during the call; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_cw2017_soc(
    gauge: *mut Cw2017,
    bus: *const CBus,
    addr: u8,
    now_ms: u32,
    out: *mut u8,
) -> bool {
    // SAFETY: as documented.
    let (Some(gauge), Some(bus), Some(out)) =
        (unsafe { gauge.as_mut() }, unsafe { bus.as_ref() }, unsafe {
            out.as_mut()
        })
    else {
        return false;
    };
    let Some(Some(soc)) = block_on(gauge.soc(&mut Bus(bus), &mut Delay(bus), addr, now_ms)) else {
        return false;
    };
    *out = soc;
    true
}
