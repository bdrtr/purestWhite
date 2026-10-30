//! The application's loop: its first commands, then the commands each event calls for.

use crate::{Jobs, Timers, Views};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::DynamicReceiver;
use embassy_sync::signal::Signal;
use purestwhite_app::{App, Command, Commands, Event};

/// The ends of the channels the application's loop uses.
pub struct AppLinks<'a, M: RawMutex> {
    pub events: DynamicReceiver<'a, Event>,
    pub views: &'a Views<M>,
    pub timers: &'a Timers<M>,
    pub jobs: &'a Jobs<M>,
    pub sleep: &'a Signal<M, ()>,
}

/// Runs `app` on the events from `links.events`: each view goes to `links.views`, each timer to
/// `links.timers`, each job to `links.jobs`, and `links.sleep` is set when the application asks to
/// sleep. Only the events are waited for.
pub async fn run_app<M: RawMutex>(app: &mut App, links: AppLinks<'_, M>) -> ! {
    carry_out(app.start(), &links);
    loop {
        let event = links.events.receive().await;
        carry_out(app.handle(event), &links);
    }
}

/// Hands `commands` to the services, in order.
fn carry_out<M: RawMutex>(commands: Commands, links: &AppLinks<'_, M>) {
    for command in commands {
        match command {
            Command::Show(show) => links.views.put(show),
            Command::Arm(arm) => links.timers.arm(arm),
            Command::Run(job) => links.jobs.give(job),
            Command::Sleep => links.sleep.signal(()),
        }
    }
}
