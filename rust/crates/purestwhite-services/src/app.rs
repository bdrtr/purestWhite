//! The application's loop: its first commands, then the commands each event calls for.

use crate::Views;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::DynamicReceiver;
use embassy_sync::signal::Signal;
use purestwhite_app::{App, Command, Commands};

/// Runs `app` on the events from `events`: each view goes to `views`, and `sleep` is set when
/// the application asks to sleep. Only `events` is waited for.
pub async fn run_app<M: RawMutex>(
    app: &mut App,
    events: DynamicReceiver<'_, purestwhite_app::Event>,
    views: &Views<M>,
    sleep: &Signal<M, ()>,
) -> ! {
    carry_out(app.start(), views, sleep);
    loop {
        let event = events.receive().await;
        carry_out(app.handle(event), views, sleep);
    }
}

/// Hands `commands` to the services, in order.
fn carry_out<M: RawMutex>(commands: Commands, views: &Views<M>, sleep: &Signal<M, ()>) {
    for command in commands {
        match command {
            Command::Show(show) => views.put(show),
            Command::Sleep => sleep.signal(()),
        }
    }
}
