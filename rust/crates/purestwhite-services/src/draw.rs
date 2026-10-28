//! The drawing loop: the latest view, drawn into a free frame.

use crate::{Drawn, Frame, Views};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{DynamicReceiver, DynamicSender};
use purestwhite_render::Text;

/// The ends of the channels the drawing loop uses.
pub struct DrawLinks<'a, M: RawMutex> {
    pub views: &'a Views<M>,
    /// Frames handed back by the panel.
    pub empty: DynamicReceiver<'a, Frame>,
    /// Where drawn frames go to the panel.
    pub drawn: DynamicSender<'a, Drawn>,
    /// The same channel's other end, to take back a frame the panel has not taken yet.
    pub unshown: DynamicReceiver<'a, Drawn>,
}

/// Draws each view from `links.views`, with its text in `text` when given, and sends it to
/// `links.drawn`. The frame is one the panel has not taken yet (its view is out of date now, and
/// the new view keeps the stronger refresh), or else the next one from `links.empty`.
pub async fn run_draw<M: RawMutex>(links: DrawLinks<'_, M>, mut text: Option<&mut Text<'_>>) -> ! {
    loop {
        let mut show = links.views.take().await;
        let frame = match links.unshown.try_receive() {
            Ok(stale) => {
                show.refresh = stale.refresh.stronger(show.refresh);
                stale.frame
            }
            Err(_) => links.empty.receive().await,
        };
        purestwhite_render::draw(&show.view, frame, text.as_deref_mut());
        links
            .drawn
            .send(Drawn {
                frame,
                seq: show.seq,
                refresh: show.refresh,
            })
            .await;
    }
}
