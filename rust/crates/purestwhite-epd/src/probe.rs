//! Which controller drives the X4 Pro's panel: freeink-sdk's `XteinkDetect` display-bus probe
//! (the X4-family path), which the C++ firmware runs at every boot. It bit-bangs the panel lines
//! as GPIOs and reads the `UC81xx` status (FLG, 0x71) and version (VER, 0x70) registers half
//! duplex on the data line; an SSD1677 answers neither, so the line floats. A `UC81xx` is
//! confirmed only when two passes agree on a driven answer, or, for modules that read VER as all
//! 0xFF, when the MTP (0xA2) read shows a programmed or repeatable block. VER byte 2 (`LUT_VER`)
//! then tells the UC8279 from the UC8179.
//!
//! The lines go through [`ProbeLines`], which mirrors Arduino's `pinMode`, `digitalWrite` and
//! `digitalRead`: the probe switches the data line between output and input, which no
//! embedded-hal trait covers. The caller takes the pins back (as SPI) afterwards.

use embedded_hal_async::delay::DelayNs;

/// The panel's lines during the probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Sclk,
    /// The controller's bidirectional data line (the SPI MOSI pin).
    Sda,
    Cs,
    Dc,
    Rst,
    Busy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Output,
    Input,
    InputPullUp,
}

/// The panel's lines as GPIOs.
pub trait ProbeLines {
    fn mode(&mut self, line: Line, mode: Mode);
    fn write(&mut self, line: Line, high: bool);
    fn read(&mut self, line: Line) -> bool;
    /// Releases a pad hold on reset, which a deep sleep may leave.
    fn release_reset_hold(&mut self);
}

const FLG: u8 = 0x71;
const VER: u8 = 0x70;
const RMTP: u8 = 0xA2;
/// The MTP bytes kept: the refresh-enable key, the factory settings, product id, LUT version.
pub const MTP_LEN: usize = 48;

/// The probe's verdict, as the SDK's `DisplayControllerVerdict`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// A `UC81xx` answered.
    Uc81xxConfirmed,
    /// Neither pass matched and no MTP confirmed: the board's default controller (the SSD1677
    /// on the X4 Pro).
    PrimaryAssumed,
    /// Exactly one pass matched, or both matched with different versions: the default too.
    Inconclusive,
}

/// What the probe read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    pub verdict: Verdict,
    /// VER from the authoritative pass; byte 2 is `LUT_VER`.
    pub ver: [u8; 5],
    /// FLG from the first pass.
    pub flg: u8,
    /// The MTP block, read whenever something drove the status line.
    pub mtp: Option<[u8; MTP_LEN]>,
}

/// The X4 Pro's panel controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Controller {
    Ssd1677,
    /// With its `LUT_VER`.
    Uc8179(u8),
    Uc8279(u8),
}

/// `LUT_VER` values of the UC8279 (the stock firmware's panel tables).
#[must_use]
pub fn lut_ver_is_uc8279(id: u8) -> bool {
    matches!(id, 0x02 | 0x03 | 0x41 | 0x42 | 0x67 | 0x68 | 0x69)
}

impl Probe {
    /// The controller the X4 Pro runs, as the SDK promotes its SSD1677 default: a confirmed
    /// `UC81xx` is a UC8279 when `LUT_VER` says so, otherwise a UC8179 (every unrecognized part
    /// benched so far was one).
    #[must_use]
    pub fn controller(&self) -> Controller {
        let [_, _, lut_ver, _, _] = self.ver;
        match self.verdict {
            Verdict::Uc81xxConfirmed if lut_ver_is_uc8279(lut_ver) => Controller::Uc8279(lut_ver),
            Verdict::Uc81xxConfirmed => Controller::Uc8179(lut_ver),
            Verdict::PrimaryAssumed | Verdict::Inconclusive => Controller::Ssd1677,
        }
    }
}

/// About 500 kHz.
async fn tick(delay: &mut impl DelayNs) {
    delay.delay_us(1).await;
}

async fn write_byte(lines: &mut impl ProbeLines, delay: &mut impl DelayNs, mut b: u8) {
    for _ in 0..8 {
        lines.write(Line::Sda, b & 0x80 != 0);
        tick(delay).await;
        lines.write(Line::Sclk, true);
        tick(delay).await;
        lines.write(Line::Sclk, false);
        b <<= 1;
    }
}

/// The controller shifts each bit out on the falling clock edge: sampled while low.
async fn read_byte(lines: &mut impl ProbeLines, delay: &mut impl DelayNs) -> u8 {
    let mut b = 0u8;
    for _ in 0..8 {
        tick(delay).await;
        b = (b << 1) | u8::from(lines.read(Line::Sda));
        lines.write(Line::Sclk, true);
        tick(delay).await;
        lines.write(Line::Sclk, false);
    }
    b
}

/// A command, then `out.len()` bytes read back half duplex.
async fn command_read(
    lines: &mut impl ProbeLines,
    delay: &mut impl DelayNs,
    command: u8,
    out: &mut [u8],
) {
    lines.mode(Line::Sda, Mode::Output);
    lines.write(Line::Dc, false);
    lines.write(Line::Cs, false);
    tick(delay).await;
    write_byte(lines, delay, command).await;
    lines.write(Line::Dc, true);
    lines.mode(Line::Sda, Mode::InputPullUp);
    tick(delay).await;
    for b in out.iter_mut() {
        *b = read_byte(lines, delay).await;
    }
    lines.write(Line::Cs, true);
    lines.mode(Line::Sda, Mode::Output);
}

/// All five bytes alike: a floating line, not an answer.
fn floating(ver: [u8; 5]) -> bool {
    ver.iter().all(|&b| b == ver[0])
}

/// A driven idle status (`BUSY_N` set) and a driven version.
fn uc81xx(ver: [u8; 5], flg: u8) -> bool {
    flg != 0x00 && flg != 0xFF && flg & 0x01 == 0x01 && !floating(ver)
}

/// One pass: a reset of `reset_low_ms`, then FLG and VER; whether they look like a `UC81xx`.
async fn pass(
    lines: &mut impl ProbeLines,
    delay: &mut impl DelayNs,
    reset_low_ms: u32,
) -> ([u8; 5], u8, bool) {
    lines.mode(Line::Cs, Mode::Output);
    lines.write(Line::Cs, true);
    lines.mode(Line::Sclk, Mode::Output);
    lines.write(Line::Sclk, false);
    lines.mode(Line::Dc, Mode::Output);
    lines.write(Line::Dc, false);
    lines.mode(Line::Sda, Mode::Output);
    lines.mode(Line::Busy, Mode::Input);
    lines.release_reset_hold();
    lines.mode(Line::Rst, Mode::Output);
    lines.write(Line::Rst, true);
    delay.delay_ms(2).await;
    lines.write(Line::Rst, false);
    delay.delay_ms(reset_low_ms).await;
    lines.write(Line::Rst, true);
    delay.delay_ms(30).await;
    let mut flg = [0u8];
    command_read(lines, delay, FLG, &mut flg).await;
    let mut ver = [0u8; 5];
    command_read(lines, delay, VER, &mut ver).await;
    let [flg] = flg;
    (ver, flg, uc81xx(ver, flg))
}

async fn read_mtp(lines: &mut impl ProbeLines, delay: &mut impl DelayNs) -> [u8; MTP_LEN] {
    // A dummy byte first.
    let mut raw = [0u8; MTP_LEN + 1];
    command_read(lines, delay, RMTP, &mut raw).await;
    let mut mtp = [0u8; MTP_LEN];
    if let Some(bytes) = raw.get(1..) {
        mtp.copy_from_slice(bytes);
    }
    mtp
}

fn release(lines: &mut impl ProbeLines) {
    lines.mode(Line::Sclk, Mode::Input);
    lines.mode(Line::Sda, Mode::Input);
    // Not left selected.
    lines.mode(Line::Cs, Mode::InputPullUp);
    lines.mode(Line::Dc, Mode::Input);
    // Reset has a pull-up inside the controller.
    lines.mode(Line::Rst, Mode::Input);
}

/// Runs the probe. The first pass screens with a 1 ms reset; only a match earns the second its
/// 50 ms reset, so a board that never answers pays about 66 ms. Leaves every line an input.
pub async fn probe(lines: &mut impl ProbeLines, delay: &mut impl DelayNs) -> Probe {
    let (ver1, flg, pass1) = pass(lines, delay, 1).await;
    delay.delay_ms(2).await;
    let (ver2, _, pass2) = pass(lines, delay, if pass1 { 50 } else { 1 }).await;
    let agree = ver1 == ver2;
    let mut confirmed = pass1 && pass2 && agree;
    let driven = flg != 0x00 && flg != 0xFF && flg & 0x01 == 0x01;
    let ver = if pass1 && pass2 { ver2 } else { ver1 };
    let mut mtp = None;
    if confirmed || driven {
        mtp = Some(read_mtp(lines, delay).await);
    }
    // A module whose VER reads all 0xFF while its status is driven: confirmed by a programmed MTP
    // (refresh-enable key 0xA5), or by a non-uniform MTP that repeats exactly.
    if let Some(block) = mtp
        && !confirmed
        && driven
        && agree
        && floating(ver1)
        && ver1[0] == 0xFF
    {
        if block[0] == 0xA5 {
            confirmed = true;
        } else if block.iter().any(|&b| b != block[0]) {
            confirmed = read_mtp(lines, delay).await == block;
        }
    }
    release(lines);
    let verdict = if confirmed {
        Verdict::Uc81xxConfirmed
    } else if !pass1 && !pass2 {
        Verdict::PrimaryAssumed
    } else {
        Verdict::Inconclusive
    };
    Probe {
        verdict,
        ver,
        flg,
        mtp,
    }
}
