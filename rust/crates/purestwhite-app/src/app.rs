//! The application's state and how each event changes it.

pub(crate) mod pages;

use crate::armings::Armings;
use crate::cadence::Cadence;
use crate::view::{Home, View};
use crate::{Arm, Command, Commands, Event, Job, Key, Refresh, Show, Timer};
use pages::PageLoop;

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
    /// The latest `seq` the panel answered for, shown or failed.
    answered: Option<u32>,
    armings: Armings,
    /// The pages on the card, once counted.
    pages: u16,
    /// The page loop, while it runs (`pages`).
    page_loop: Option<PageLoop>,
    /// A page is being read: its answer comes even after the loop ended.
    reading: bool,
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        App::default()
    }

    /// The first commands: the home screen, with a full refresh, the idle timer, and the card's
    /// pages counted.
    #[must_use]
    pub fn start(&mut self) -> Commands {
        let home = self.show(View::Home(self.home), Refresh::Full);
        Commands::three(
            Command::Show(home),
            Command::Arm(self.arm_idle()),
            Command::Run(Job::CountPages),
        )
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
            Event::Shown(seq) => {
                self.shown(seq);
                self.page_after_answer()
            }
            Event::ScreenFailed(seq) => self.failed(seq),
            Event::Pressed(key) => self.pressed(key),
            Event::Fired(timer, id) if self.armings.is_latest(timer, id) => self.fired(timer),
            Event::Fired(..) => Commands::none(),
            Event::Pages(count) => {
                self.pages = count;
                Commands::none()
            }
            Event::PageLoaded(n) => self.page_loaded(n),
            Event::PageFailed(n) => self.page_failed(n),
        }
    }

    /// A key: the page loop's, or else the idle timer armed again.
    fn pressed(&mut self, key: Key) -> Commands {
        self.page_key(key)
            .unwrap_or_else(|| Commands::one(Command::Arm(self.arm_idle())))
    }

    /// The latest arming of `timer` fired.
    fn fired(&mut self, timer: Timer) -> Commands {
        match timer {
            Timer::Idle => self.sleep(),
            Timer::Page => self.page_due(),
        }
    }

    /// The panel showed `seq`: its failures in a row are over, and a full refresh owed for an
    /// earlier failure is no longer.
    fn shown(&mut self, seq: u32) {
        self.answer(seq);
        self.failures = 0;
        if self.full_after.is_some_and(|failed| seq > failed) {
            self.full_after = None;
        }
    }

    /// Notes that the panel answered for `seq`.
    fn answer(&mut self, seq: u32) {
        if self.answered.is_none_or(|answered| seq > answered) {
            self.answered = Some(seq);
        }
    }

    /// Whether the panel has answered for every view shown.
    fn is_quiet(&self) -> bool {
        self.answered
            .is_some_and(|answered| answered.wrapping_add(1) == self.next_seq)
    }

    /// The sleep screen, on its way to the panel; the sleep follows the panel's answer.
    fn sleep(&mut self) -> Commands {
        let show = self.show(View::Sleep, Refresh::Full);
        self.phase = Phase::Sleeping(show.seq);
        Commands::one(Command::Show(show))
    }

    /// The idle timer, armed from now: at the start and at each key press.
    fn arm_idle(&mut self) -> Arm {
        self.armings.arm(Timer::Idle, IDLE_MS)
    }

    /// The home screen redrawn with a fast refresh when `home` changed it, unless the page loop
    /// shows its pages.
    fn update(&mut self, home: Home) -> Commands {
        if home == self.home {
            return Commands::none();
        }
        self.home = home;
        if self.in_page_loop() {
            return Commands::none();
        }
        Commands::one(Command::Show(self.show(View::Home(home), Refresh::Fast)))
    }

    /// After the panel failed to show `seq`, the next show is a full refresh. When nothing newer
    /// is on its way, the current screen is shown again, up to `RETRIES` times in a row; after
    /// that, a page waiting for the panel's answer is read.
    fn failed(&mut self, seq: u32) -> Commands {
        self.answer(seq);
        self.full_after = Some(seq);
        self.failures = self.failures.saturating_add(1);
        let latest = seq.wrapping_add(1) == self.next_seq;
        if latest && self.failures <= RETRIES {
            Commands::one(Command::Show(self.show(self.current, Refresh::Full)))
        } else {
            self.page_after_answer()
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
