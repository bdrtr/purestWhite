//! When a fast refresh becomes a half one: as the C++ reader's refresh cycle
//! (`ReaderUtils::displayWithRefreshCycle`, with its default "every 15 pages").

use crate::Refresh;

/// The 15th refresh after a full or half one is a half one.
const PERIOD: u8 = 15;

/// Fast refreshes leave traces behind; every `PERIOD`th refresh is a half one, which clears them.
#[derive(Debug)]
pub(crate) struct Cadence {
    /// Refreshes until the next half one, this one included.
    left: u8,
}

impl Default for Cadence {
    fn default() -> Self {
        Cadence { left: PERIOD }
    }
}

impl Cadence {
    /// The refresh to use when `wanted` is asked for.
    pub(crate) fn next(&mut self, wanted: Refresh) -> Refresh {
        if wanted == Refresh::Fast && self.left > 1 {
            self.left -= 1;
            return Refresh::Fast;
        }
        self.left = PERIOD;
        match wanted {
            Refresh::Full => Refresh::Full,
            Refresh::Half | Refresh::Fast => Refresh::Half,
        }
    }
}
