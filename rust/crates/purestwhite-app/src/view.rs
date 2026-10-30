//! What a screen shows, as plain data: the renderer (`purestwhite-render`) draws it.

/// A time of day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
}

/// The home screen: the name, the battery and the time when known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Home {
    /// In percent.
    pub battery: Option<u8>,
    pub time: Option<Time>,
}

/// A screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Home(Home),
    /// Page n of the card's pages (from 1), as the storage loop read it last.
    Page(u16),
    /// What the panel keeps showing while the device sleeps.
    Sleep,
}

/// The first screen.
impl Default for View {
    fn default() -> Self {
        View::Home(Home::default())
    }
}
