//! The page loop of the battery measurement (rust/firmware/DESIGN.md, gate 1): Right on the home
//! screen starts it when the card holds pages; it shows them in turn, one every `PAGE_MS`, and
//! after `LOOP_PAGES` the device sleeps. Left goes back home. The idle sleep waits while it runs.
//! The measurement spans the loop: `Measure::Begin` as it starts, `Measure::End` however it ends.
//! The next page is read only once the panel has answered for every view shown, so the storage
//! loop never reads into the page while drawing copies it.

use super::App;
use crate::view::View;
use crate::{Command, Commands, Job, Key, Measure, Refresh, SleepReason, Timer};

/// How long each page is shown.
pub const PAGE_MS: u32 = 30_000;
/// How many pages the loop shows before the device sleeps: an hour's worth.
pub const LOOP_PAGES: u16 = 120;

/// Where the loop is.
#[derive(Clone, Copy, Debug)]
pub(super) struct PageLoop {
    /// The pages on the card.
    count: u16,
    /// The page read last, or being read.
    page: u16,
    /// How many pages were shown.
    shown: u16,
    /// The next page is due, but the panel has not answered for every view yet.
    waiting: bool,
}

impl App {
    /// What a key does while the page loop runs, or starts it; `None` when the key is for the rest
    /// of the application.
    pub(super) fn page_key(&mut self, key: Key) -> Option<Commands> {
        match (self.page_loop, key) {
            (Some(_), Key::Left) => Some(self.stop_pages()),
            (Some(_), _) => Some(Commands::none()),
            // A page read for the loop that ended is answered first: one job at a time.
            (None, Key::Right) if self.pages > 0 && !self.reading => {
                self.armings.disarm(Timer::Idle);
                self.page_loop = Some(PageLoop {
                    count: self.pages,
                    page: 1,
                    shown: 0,
                    waiting: false,
                });
                Some(Commands::two(
                    Command::Measure(Measure::Begin),
                    self.read(1),
                ))
            }
            (None, _) => None,
        }
    }

    /// Page `n` read: shown (the first with a full refresh, from the home screen), and the next
    /// one's timer armed.
    pub(super) fn page_loaded(&mut self, n: u16) -> Commands {
        self.reading = false;
        let Some(page_loop) = self.page_loop.as_mut().filter(|l| l.page == n) else {
            return Commands::none();
        };
        let first = page_loop.shown == 0;
        page_loop.shown = page_loop.shown.saturating_add(1);
        let refresh = if first { Refresh::Full } else { Refresh::Fast };
        let show = self.show(View::Page(n), refresh);
        let arm = self.armings.arm(Timer::Page, PAGE_MS);
        Commands::two(Command::Show(show), Command::Arm(arm))
    }

    /// Page `n` could not be read: the loop ends, home.
    pub(super) fn page_failed(&mut self, n: u16) -> Commands {
        self.reading = false;
        match self.page_loop {
            Some(page_loop) if page_loop.page == n => self.stop_pages(),
            _ => Commands::none(),
        }
    }

    /// The next page is due: read once the panel has answered; after `LOOP_PAGES`, the sleep.
    pub(super) fn page_due(&mut self) -> Commands {
        let quiet = self.is_quiet();
        let Some(page_loop) = self.page_loop.as_mut() else {
            return Commands::none();
        };
        if page_loop.shown >= LOOP_PAGES {
            return self.sleep(SleepReason::LoopDone);
        }
        page_loop.page = page_loop.page % page_loop.count + 1;
        page_loop.waiting = !quiet;
        let page = page_loop.page;
        if quiet {
            Commands::one(self.read(page))
        } else {
            Commands::none()
        }
    }

    /// After the panel answered: the page that was waiting for it, if any.
    pub(super) fn page_after_answer(&mut self) -> Commands {
        let quiet = self.is_quiet();
        match self.page_loop.as_mut() {
            Some(page_loop) if page_loop.waiting && quiet => {
                page_loop.waiting = false;
                let page = page_loop.page;
                Commands::one(self.read(page))
            }
            _ => Commands::none(),
        }
    }

    /// Page `n` to be read.
    fn read(&mut self, n: u16) -> Command {
        self.reading = true;
        Command::Run(Job::LoadPage(n))
    }

    /// Whether the page loop runs: the home screen's updates are kept, not shown.
    pub(super) fn in_page_loop(&self) -> bool {
        self.page_loop.is_some()
    }

    /// The loop ended (Left, or a page that cannot be read): the measurement's end, home with a
    /// full refresh, and the idle timer again.
    fn stop_pages(&mut self) -> Commands {
        let Some(end) = self.end_pages() else {
            return Commands::none();
        };
        let home = self.show(View::Home(self.home), Refresh::Full);
        let idle = self.arm_idle();
        Commands::three(
            Command::Measure(end),
            Command::Show(home),
            Command::Arm(idle),
        )
    }

    /// The loop, if one runs, ended: its page timer out of date; the measurement's end.
    pub(super) fn end_pages(&mut self) -> Option<Measure> {
        let page_loop = self.page_loop.take()?;
        self.armings.disarm(Timer::Page);
        Some(Measure::End {
            shown: page_loop.shown,
        })
    }
}
