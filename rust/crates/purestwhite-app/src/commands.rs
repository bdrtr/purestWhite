//! The commands one event calls for: a few, in order, without allocating.

use crate::Command;

/// Room for three commands (the start calls for three, an event for two at most so far); raised
/// when a handler needs more.
const MAX: usize = 3;

/// Commands in the order they are to be carried out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Commands {
    items: [Option<Command>; MAX],
}

impl Commands {
    #[must_use]
    pub fn none() -> Self {
        Commands { items: [None; MAX] }
    }

    #[must_use]
    pub fn one(command: Command) -> Self {
        Commands {
            items: [Some(command), None, None],
        }
    }

    #[must_use]
    pub fn two(first: Command, second: Command) -> Self {
        Commands {
            items: [Some(first), Some(second), None],
        }
    }

    #[must_use]
    pub fn three(first: Command, second: Command, third: Command) -> Self {
        Commands {
            items: [Some(first), Some(second), Some(third)],
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.iter().all(Option::is_none)
    }
}

impl IntoIterator for Commands {
    type Item = Command;
    type IntoIter = core::iter::Flatten<core::array::IntoIter<Option<Command>, MAX>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter().flatten()
    }
}
