//! The drawing loop: the latest view, drawn into a free frame.

use crate::{Drawn, Frame, Views};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{DynamicReceiver, DynamicSender};
use purestwhite_render::Text;

/// Draws each view from `views`, with its text in `text` when given, and sends it to `drawn`.
/// The frame is one the panel has not taken yet from `unshown` (the same channel as `drawn`: its
/// view is out of date now, and the new view keeps the stronger refresh), or else the next one
/// from `empty`.
pub async fn run_draw<M: RawMutex>(
    views: &Views<M>,
    empty: DynamicReceiver<'_, Frame>,
    drawn: DynamicSender<'_, Drawn>,
    unshown: DynamicReceiver<'_, Drawn>,
    mut text: Option<&mut Text<'_>>,
) -> ! {
    loop {
        let mut show = views.take().await;
        let frame = match unshown.try_receive() {
            Ok(stale) => {
                show.refresh = stale.refresh.stronger(show.refresh);
                stale.frame
            }
            Err(_) => empty.receive().await,
        };
        purestwhite_render::draw(&show.view, frame, text.as_deref_mut());
        drawn
            .send(Drawn {
                frame,
                seq: show.seq,
                refresh: show.refresh,
            })
            .await;
    }
}
