//! The application's state and how each event changes it.

use crate::cadence::Cadence;
use crate::view::{Home, View};
use crate::{Arm, Command, Commands, Event, Refresh, Show, Timer};

/// How many times in a row a screen the panel failed to show is shown again, with a full refresh.
const RETRIES: u8 = 2;
/// How long without a key press before the device sleeps: the C++ firmware's default auto-sleep
/// (`CrossPointSettings::sleepTimeoutMinutes`, 10 minutes).
pub const IDLE_MS: u32 = 10 * 60 * 1000;

/// Where the device is on its way to sleep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Awake,
    /// The sleep screen, with this `seq`, is on its way to the panel.
    Sleeping(u32),
    Asleep,
}

/// The application's state.
#[derive(Debug, Default)]
pub struct App {
    home: Home,
    /// The screen last asked for, shown again after a failure.
    current: View,
    phase: Phase,
    cadence: Cadence,
    /// The next show's `seq`.
    next_seq: u32,
    /// The panel failed to show this `seq`: the next show is a full refresh, unless a later view
    /// is shown first (the drivers flash the first refresh after bringing the panel back).
    full_after: Option<u32>,
    /// The panel's failures in a row.
    failures: u8,
    /// The idle timer's latest `Arm::id`; a firing of an earlier one is out of date.
    idle: Option<u32>,
    /// The next `Arm::id`.
    next_arm: u32,
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        App::default()
    }

    /// The first commands: the home screen, with a full refresh, and the idle timer.
    #[must_use]
    pub fn start(&mut self) -> Commands {
        let home = self.show(View::Home(self.home), Refresh::Full);
        Commands::two(Command::Show(home), Command::Arm(self.arm_idle()))
    }

    /// The commands `event` calls for; once the sleep screen is asked for, only the panel's
    /// answer to it, and nothing once asleep.
    #[must_use]
    pub fn handle(&mut self, event: Event) -> Commands {
        match (self.phase, event) {
            (Phase::Awake, _) => self.awake(event),
            (Phase::Sleeping(screen), Event::Shown(seq) | Event::ScreenFailed(seq))
                if seq >= screen =>
            {
                self.phase = Phase::Asleep;
                Commands::one(Command::Sleep)
            }
            (Phase::Sleeping(_) | Phase::Asleep, _) => Commands::none(),
        }
    }

    /// Whether the device was asked to sleep.
    #[must_use]
    pub fn is_asleep(&self) -> bool {
        self.phase == Phase::Asleep
    }

    fn awake(&mut self, event: Event) -> Commands {
        match event {
            Event::Battery(percent) => self.update(Home {
                battery: Some(percent.min(100)),
                ..self.home
            }),
            Event::Time(time) => self.update(Home {
                time: Some(time),
                ..self.home
            }),
            Event::SleepRequested => self.sleep(),
            Event::Fired(Timer::Idle, id) if self.idle == Some(id) => self.sleep(),
            Event::Shown(seq) => {
                self.failures = 0;
                if self.full_after.is_some_and(|failed| seq > failed) {
                    self.full_after = None;
                }
                Commands::none()
            }
            Event::ScreenFailed(seq) => self.failed(seq),
            Event::Pressed(_) => Commands::one(Command::Arm(self.arm_idle())),
            Event::Fired(..) => Commands::none(),
        }
    }

    /// The sleep screen, on its way to the panel; the sleep follows the panel's answer.
    fn sleep(&mut self) -> Commands {
        let show = self.show(View::Sleep, Refresh::Full);
        self.phase = Phase::Sleeping(show.seq);
        Commands::one(Command::Show(show))
    }

    /// The idle timer, armed from now: at the start and at each key press.
    fn arm_idle(&mut self) -> Arm {
        let id = self.next_arm;
        self.next_arm = id.wrapping_add(1);
        self.idle = Some(id);
        Arm {
            timer: Timer::Idle,
            after_ms: IDLE_MS,
            id,
        }
    }

    /// The home screen redrawn with a fast refresh when `home` changed it.
    fn update(&mut self, home: Home) -> Commands {
        if home == self.home {
            return Commands::none();
        }
        self.home = home;
        Commands::one(Command::Show(self.show(View::Home(home), Refresh::Fast)))
    }

    /// After the panel failed to show `seq`, the next show is a full refresh. When nothing newer
    /// is on its way, the current screen is shown again, up to `RETRIES` times in a row.
    fn failed(&mut self, seq: u32) -> Commands {
        self.full_after = Some(seq);
        self.failures = self.failures.saturating_add(1);
        let latest = seq.wrapping_add(1) == self.next_seq;
        if latest && self.failures <= RETRIES {
            Commands::one(Command::Show(self.show(self.current, Refresh::Full)))
        } else {
            Commands::none()
        }
    }

    /// `view` to show with `wanted`, or the refresh the cadence or a failure needs.
    fn show(&mut self, view: View, wanted: Refresh) -> Show {
        let wanted = if self.full_after.take().is_some() {
            Refresh::Full
        } else {
            wanted
        };
        self.current = view;
        let seq = self.next_seq;
        self.next_seq = seq.wrapping_add(1);
        Show {
            seq,
            view,
            refresh: self.cadence.next(wanted),
        }
    }
}
