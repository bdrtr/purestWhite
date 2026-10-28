//! The panel, while it answers: nothing more goes to it after a call that failed.

use crate::counts::Counts;
use esp_hal::time::Instant;
use purestwhite_firmware::board::PanelParts;
use purestwhite_firmware::panel::{self, Lines, OpenError, Panel};

pub struct Screen {
    /// The controller's name.
    pub name: &'static str,
    pub panel: Panel,
    pub lines: Lines<'static>,
    pub working: bool,
}

impl Screen {
    /// Shows `frame` with the refresh `counts` picks, and notes it there.
    pub fn show(&mut self, frame: &[u8], counts: &mut Counts, full: bool) {
        if !self.working {
            return;
        }
        let refresh = counts.next_refresh(full);
        let start = Instant::now();
        self.working = self.panel.display(&mut self.lines, frame, refresh, false);
        counts.note_refresh(refresh, start.elapsed().as_millis(), self.working);
    }

    /// The panel asleep, as the C++ firmware leaves it; whether it is.
    pub fn sleep(&mut self) -> bool {
        self.working && self.panel.deep_sleep(&mut self.lines)
    }
}

/// The panel (`panel::open`), begun; or why it cannot be used.
pub fn open_screen(parts: PanelParts) -> Result<Screen, OpenError> {
    let mut opened = panel::open(parts, |_| None)?;
    let working = opened.panel.begin(&mut opened.lines);
    Ok(Screen {
        name: opened.probe.controller().name(),
        panel: opened.panel,
        lines: opened.lines,
        working,
    })
}
