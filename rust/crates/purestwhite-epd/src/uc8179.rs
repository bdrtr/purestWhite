//! The UC8179 that X4 and X4 Pro batches ship in place of the SSD1677: freeink-sdk's
//! `Uc8179Driver`, black and white and its three 4-level grayscale passes.
//!
//! The controller addresses 800x600: the 480 visible rows go first, bottom row first, then the
//! rows past them are padded white. It refreshes differentially like the UC8279 (OLD plane in
//! DTM1, NEW in DTM2, OTP waveforms), with a two-byte VCOM and data interval that is set for the
//! refresh and restored after. A framebuffer has a bit per pixel, 1 for white, rows of
//! `width / 8` bytes.
//!
//! The first refresh after `begin` always flashes; the panel stays powered between refreshes
//! unless a refresh is asked to power it off; after `deep_sleep` only `begin` (a hardware reset)
//! wakes the controller. A failed call leaves the controller's state unknown: `begin` again
//! before the next refresh.
//!
//! Grayscale (the SDK's anti-aliasing): `begin_grayscale`, then `copy_grayscale_lsb` and
//! `copy_grayscale_msb`, then `display_gray`, then `cleanup_grayscale`. In [`GrayMode::Overlay`]
//! and [`GrayMode::Absolute`] the black-and-white base is shown first and the two planes select a
//! short custom waveform per pixel; [`GrayMode::Direct`] programs the controller's full-gray
//! waveform instead and shows the planes alone. The overlay masks are turned into the absolute
//! planes the waveform takes with the base kept in a frame-long buffer (the SDK's PSRAM copy, see
//! [`Uc8179::with_base`]); without one the masks go to the controller as they are. After an
//! Overlay or Absolute gray refresh, the next display, if it is a `Fast` `display` (with no
//! resync or base-less cleanup between), is the stock firmware's non-flashing transition from the
//! previous base. The SDK's `preconditionGrayscale` does nothing on this controller and has no
//! counterpart here.

mod bw;
mod command;
mod controller;
mod gray;
mod plane;
mod waveform;

/// The longest row; the X4 Pro's is 100 bytes.
const MAX_ROW: usize = 128;

/// The controller's settings (freeink-sdk's `Uc8179Config`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// PSR byte 0 at init and for the custom-waveform refreshes; the built-in ones clear its REG
    /// bit.
    pub psr0: u8,
    pub psr1: u8,
    /// Power-off sequence.
    pub pfs: u8,
    /// Booster soft start.
    pub btst: [u8; 4],
    pub gate_scan: u8,
    pub ccset: u8,
    /// Forced temperature for a full and a fast refresh.
    pub tsset: u8,
    pub tsset_fast: u8,
    /// The VCOM and data interval's first byte during a refresh and after it.
    pub cdi_active: u8,
    pub cdi_idle: u8,
    /// Power saving; 0 leaves the controller's default.
    pub power_save: u8,
    pub width: u16,
    pub height: u16,
    /// The gates addressed.
    pub tres_height: u16,
}

/// The X4 Pro's panel, as freeink-sdk's defaults.
pub const X4_PRO: Config = Config {
    psr0: 0x3F,
    psr1: 0x0A,
    pfs: 0x20,
    btst: [0x25, 0x25, 0x3C, 0x25],
    gate_scan: 0x02,
    ccset: 0x02,
    tsset: 0x1E,
    tsset_fast: 0x5A,
    cdi_active: 0x29,
    cdi_idle: 0xA9,
    power_save: 0x22,
    width: 800,
    height: 480,
    tres_height: 600,
};

/// What a grayscale pass's two planes hold (freeink-sdk's `GrayscaleMode`), as (LSB, MSB) bits
/// per pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrayMode {
    /// Masks over the black-and-white base shown first, whose bit is 1 only for white: white and
    /// black (0,0), light (0,1), dark (1,1). The EPUB reader's.
    Overlay,
    /// Every pixel, after the base: black (0,0), dark (1,0), light (0,1), white (1,1). The image
    /// viewer's.
    Absolute,
    /// As `Absolute`, with the full-gray waveform and no base. The sleep screen's.
    Direct,
}

/// The controller's state between calls. `B` holds the grayscale base: a frame-long buffer
/// (`AsRef`/`AsMut<[u8]>`), absent with [`Uc8179::new`].
#[allow(
    clippy::struct_excessive_bools,
    reason = "the SDK's flags, one for one"
)]
pub struct Uc8179<B = &'static mut [u8]> {
    config: Config,
    powered: bool,
    /// DTM1 holds what the panel shows.
    old_plane_valid: bool,
    need_full_clear: bool,
    /// Both planes hold the black-and-white base shown.
    planes_synced: bool,
    /// The SDK's `_grayBase`: the base shown, or the absolute plane0 between the two plane
    /// copies.
    base: Option<B>,
    /// `base` holds the base shown.
    base_valid: bool,
    /// `base` holds plane0, made from it and the LSB mask.
    absolute_planes: bool,
    /// The planes of this pass are absolute.
    absolute_input: bool,
    /// A Direct pass is under way; `direct_planes` has bit 0 once its LSB plane went, bit 1 its
    /// MSB.
    direct_pass: bool,
    direct_planes: u8,
    /// The controller runs the full-gray settings.
    direct_configured: bool,
    /// The panel shows a Direct page.
    direct_on_panel: bool,
    /// The next `Fast` refresh takes the non-flashing transition.
    redrive_after_gray: bool,
    gray_refreshed_once: bool,
}

impl Uc8179 {
    /// Without a grayscale base: Overlay masks go to the controller as they are.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self::build(config, None)
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> Uc8179<B> {
    /// With `base` for the grayscale base; `None` when it is shorter than a frame
    /// (`row_bytes() * height` bytes). Its contents need not be set.
    #[must_use]
    pub fn with_base(config: Config, base: B) -> Option<Self> {
        let state = Self::build(config, Some(base));
        state.base().is_some().then_some(state)
    }

    fn build(config: Config, base: Option<B>) -> Self {
        Uc8179 {
            config,
            powered: false,
            old_plane_valid: false,
            need_full_clear: true,
            planes_synced: false,
            base,
            base_valid: false,
            absolute_planes: false,
            absolute_input: false,
            direct_pass: false,
            direct_planes: 0,
            direct_configured: false,
            direct_on_panel: false,
            redrive_after_gray: false,
            gray_refreshed_once: false,
        }
    }

    /// Whether the grayscale base is kept (`with_base`).
    #[must_use]
    pub fn has_base(&self) -> bool {
        self.base().is_some()
    }

    /// Bytes in a row of the framebuffer.
    #[must_use]
    pub fn row_bytes(&self) -> usize {
        usize::from(self.config.width / 8)
    }

    fn frame_len(&self) -> usize {
        self.row_bytes() * usize::from(self.config.height)
    }

    /// Rows no wider than the plane buffer, and every visible row inside the addressed gates.
    fn geometry_ok(&self) -> bool {
        let c = self.config;
        self.row_bytes() <= MAX_ROW && c.height <= c.tres_height
    }

    /// A frame (or plane) long enough, with a geometry the planes can carry.
    fn fits(&self, frame: &[u8]) -> bool {
        frame.len() >= self.frame_len() && self.geometry_ok()
    }

    fn base(&self) -> Option<&[u8]> {
        let n = self.frame_len();
        self.base.as_ref().and_then(|b| b.as_ref().get(..n))
    }

    fn base_mut(&mut self) -> Option<&mut [u8]> {
        let n = self.frame_len();
        self.base.as_mut().and_then(|b| b.as_mut().get_mut(..n))
    }

    /// Ends the grayscale pass under way: its planes are no longer taken as absolute or Direct.
    fn end_pass(&mut self) {
        self.absolute_input = false;
        self.direct_pass = false;
        self.direct_planes = 0;
    }

    /// Keeps `frame` as the grayscale base, when there is one; it no longer holds plane0.
    fn keep_base(&mut self, frame: &[u8]) {
        self.base_valid = false;
        self.absolute_planes = false;
        if let Some(base) = self.base_mut()
            && let Some(shown) = frame.get(..base.len())
        {
            base.copy_from_slice(shown);
            self.base_valid = true;
        }
    }

    /// The next `Fast` refresh flashes from white instead; a `Half` stays a `Half`. Also ends a
    /// grayscale pass under way.
    pub fn request_resync(&mut self) {
        self.end_pass();
        self.need_full_clear = true;
    }

    /// Cancels a `request_resync` made after a refresh. The first refresh after `begin` flashes
    /// regardless, as the SDK's: DTM1 is unknown until a refresh writes it.
    pub fn skip_initial_resync(&mut self) {
        self.need_full_clear = false;
    }
}
