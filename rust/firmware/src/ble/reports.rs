//! The remote's reports read for a while: each notification handed to the SDK's decoder
//! (`purestwhite-ble`'s `Decoder`) with its report id and the time it came, the release timeouts
//! polled, and every event routed as the C++ runtime routes it (`Router`), the page turns counted.

use super::hid::Hid;
use core::fmt::{self, Display, Formatter};
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use purestwhite_ble::binding::{Action, Router};
use purestwhite_ble::decoder::Decoder;
use trouble_host::prelude::NotificationListener;

/// How often the timeouts are polled with no report: well inside the 150 ms release timeout.
const TICK: Duration = Duration::from_millis(20);

/// What the reports did.
#[derive(Clone, Copy, Debug, Default)]
pub struct Turns {
    /// Notifications.
    pub frames: u32,
    /// Key presses the decoder read.
    pub keys: u32,
    /// Raw button edges (presses and releases).
    pub raws: u32,
    pub next: u32,
    pub prev: u32,
    /// Any other action (a chapter, the menu and the like).
    pub other: u32,
}

impl Turns {
    fn act(&mut self, action: Action) {
        let count = match action {
            Action::None => return,
            Action::NextPage => &mut self.next,
            Action::PrevPage => &mut self.prev,
            _ => &mut self.other,
        };
        *count = count.saturating_add(1);
    }
}

/// `turns 40 fr, 12 k, 24 r, next 6 prev 6 other 0`.
impl Display for Turns {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        let Turns {
            frames,
            keys,
            raws,
            next,
            prev,
            other,
        } = self;
        write!(
            out,
            "turns {frames} fr, {keys} k, {raws} r, next {next} prev {prev} other {other}"
        )
    }
}

/// The clock as the decoder reads it: ms since boot on 32 bits, wrapping as the C++ `millis()`.
fn now_ms() -> u32 {
    let ms = Instant::now().as_millis();
    u32::try_from(ms & u64::from(u32::MAX)).unwrap_or_default()
}

/// Reads the reports of `hid` from `listener` for `how_long`, counted into `turns`; `feed` runs
/// every pass. An indication was confirmed by the host already.
pub(super) async fn read(
    mut listener: NotificationListener<'_, 512>,
    hid: &Hid,
    mut router: Router,
    how_long: Duration,
    turns: &mut Turns,
    feed: &mut impl FnMut(),
) {
    let mut decoder = Decoder::new(hid.hints);
    let end = Instant::now() + how_long;
    while Instant::now() < end {
        if let Either::First(n) = select(listener.next(), Timer::after(TICK)).await {
            turns.frames = turns.frames.saturating_add(1);
            decoder.ingest(n.as_ref(), hid.inputs.report_id(n.handle()), now_ms());
        }
        // The clock read after the reports taken, never before one of them.
        let now = now_ms();
        decoder.poll(now);
        route(&mut decoder, &mut router, now, turns);
        feed();
    }
}

/// The decoder's events routed: the raw edges, the hold wait, then the keys (as the C++
/// runtime's `serveReader`).
fn route(decoder: &mut Decoder, router: &mut Router, now: u32, turns: &mut Turns) {
    while let Some(raw) = decoder.pop_raw() {
        turns.raws = turns.raws.saturating_add(1);
        turns.act(router.raw(&raw));
    }
    turns.act(router.hold(now));
    while let Some(key) = decoder.pop_key() {
        turns.keys = turns.keys.saturating_add(1);
        turns.act(router.key(&key));
    }
}
