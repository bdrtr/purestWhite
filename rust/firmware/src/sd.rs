//! The X4 Pro's SD card for the test images, mounted as freeink-sdk's `SdmmcBlockDevice` does:
//! SDMMC slot 1 with one data line (CLK 41, CMD 42, DAT0 40, internal pull-ups) at the default
//! 20 MHz, and GPIO5, an active-low enable, pulsed high for 80 ms and then held low for 120 ms
//! before each of up to four tries, each proven by a real read of block 0. The card protocol is
//! `sdio`'s, over esp-hal's slot: its blocking calls ([`BlockingBus`], whose futures are all ready
//! at their first poll, for images that run in eink-qemu 0.9.2, which the slot's interrupts crash)
//! or its interrupt-driven ones ([`AsyncBus`], for Embassy tasks). The FAT volume is our
//! `purestwhite-fat`'s, over [`CardBlocks`], which can only read. On both buses, reads of several
//! blocks are one open-ended CMD18 that the controller stops with CMD12, as ESP-IDF's driver reads
//! them (see [`KeptCount`]). The calls end at esp-hal's own limits, the slowest a data phase at the
//! controller's data timeout (about 42 s at the 400 kHz the card starts at), and the RTC watchdog
//! backs them.

use crate::ready;
use aligned::{A4, Aligned};
use block_device_driver::BlockDevice as _;
use embedded_hal::digital::OutputPin;
use embedded_hal_async::delay::DelayNs;
use esp_hal::delay::Delay;
use esp_hal::dma::aligned::DmaAlignedMut;
use esp_hal::gpio::Output;
use esp_hal::peripherals::{GPIO40, GPIO41, GPIO42};
use esp_hal::sdmmc::{
    BlockingError, BusWidth, CommandFlags, Error, ResponseLen, SdHostController, Slot, SlotConfig,
};
use esp_hal::{Async, Blocking};
use purestwhite_fat::Volume;
use sdio::{MmcError, Response};

/// The card's clock once it is up: ESP-IDF's `SDMMC_FREQ_DEFAULT`, the SDK's.
pub const CLOCK_HZ: u32 = 20_000_000;
/// Tries at mounting, each after a pulse of the enable.
pub const TRIES: u8 = 4;
/// CMD12, which stops a transfer.
const STOP_TRANSMISSION: u8 = 12;
/// CMD23, which sets the block count of the transfer after it.
const SET_BLOCK_COUNT: u8 = 23;
/// Blocks a card read takes at most, through [`CardBlocks`]' buffer: 16 KiB.
pub const RUN: usize = 32;

/// esp-hal's busy-wait delay as embedded-hal-async's.
pub struct SpinDelay(pub Delay);

#[allow(clippy::unused_async, reason = "blocking calls, done at once")]
impl DelayNs for SpinDelay {
    async fn delay_ns(&mut self, ns: u32) {
        embedded_hal::delay::DelayNs::delay_ns(&mut self.0, ns);
    }
}

/// The count of a CMD23 that a bus keeps back. `sdio` sets the count of a multi-block read with
/// CMD23 when the card takes it. Like ESP-IDF's driver, which the SDK uses, the buses here leave the
/// read open-ended and the controller stops it itself: CMD23 is not sent, and its count only
/// checks the read that follows.
#[derive(Default)]
struct KeptCount(Option<u32>);

impl KeptCount {
    /// A CMD23's response, which is not sent: its count kept for the next read.
    fn keep<R: Response>(&mut self, count: u32) -> R {
        self.0 = Some(count);
        R::from_words(&[0; 4])
    }

    /// Whether a read of `len` bytes in `size`-byte blocks may be stopped by the controller: a
    /// single block, or what `sdio` asked to be stopped (`auto_stop`), or as many blocks as the
    /// CMD23 kept back. Takes that count either way.
    fn check(&mut self, len: usize, size: usize, auto_stop: bool) -> Result<(), MmcError> {
        let count = self.0.take();
        let blocks = u32::try_from(len / size.max(1)).ok();
        if len > size && !auto_stop && (count.is_none() || count != blocks) {
            return Err(MmcError::Unsupported);
        }
        Ok(())
    }
}

/// The response taken for a CMD12 the card did not answer. `sdio` sends a stop before every read
/// that follows a failed one, also right after a mount, when the card is idle, has nothing to stop
/// and does not answer; the status poll `sdio` sends next clears the card's illegal-command flag.
fn stop_unanswered<R: Response>() -> R {
    R::from_words(&[0; 4])
}

/// Slot 1 through esp-hal's blocking calls, as `sdio`'s bus, for images that must also run in
/// eink-qemu: each call runs
/// to its end and its future is ready at once. One data line, reads only (writes are refused with
/// `Unsupported`).
pub struct BlockingBus<'d> {
    slot: Slot<'d, 1, Blocking>,
    count: KeptCount,
}

/// A command's response as esp-hal's engine takes it.
fn response_len(len: sdio::ResponseLen) -> ResponseLen {
    match len {
        sdio::ResponseLen::Zero => ResponseLen::None,
        sdio::ResponseLen::R48 => ResponseLen::Short,
        sdio::ResponseLen::R136 => ResponseLen::Long,
    }
}

impl<'d> BlockingBus<'d> {
    #[must_use]
    pub fn new(slot: Slot<'d, 1, Blocking>) -> Self {
        BlockingBus {
            slot,
            count: KeptCount::default(),
        }
    }

    /// A data read into `buf`, in blocks of `block_size` bytes.
    fn read(
        &mut self,
        index: u8,
        arg: u32,
        buf: &mut [u8],
        block_size: usize,
    ) -> Result<[u32; 4], MmcError> {
        let size = u16::try_from(block_size).map_err(|_| MmcError::BlockSize)?;
        let count =
            u32::try_from(buf.len() / block_size.max(1)).map_err(|_| MmcError::BlockSize)?;
        let buf = DmaAlignedMut::new(buf).map_err(MmcError::from)?;
        Ok(self
            .slot
            .read_blocks_blocking(index, arg, buf, size, count)?)
    }
}

#[allow(clippy::unused_async, reason = "blocking calls, done at once")]
impl sdio::MmcBus for BlockingBus<'_> {
    async fn send_command<'a, C>(&mut self, cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::ControlCommand + 'a,
    {
        if C::INDEX == SET_BLOCK_COUNT {
            return Ok(self.count.keep(cmd.arg()));
        }
        // As esp-hal's own bus: CMD12 stops a transfer, the others wait for one.
        let stop = C::INDEX == STOP_TRANSMISSION;
        let flags = CommandFlags {
            wait_complete: !stop,
            stop_abort: stop,
            busy: <C::Resp<'a> as Response>::BUSY,
        };
        let words = match self.slot.command_blocking(
            C::INDEX,
            cmd.arg(),
            response_len(<C::Resp<'a> as Response>::LEN),
            <C::Resp<'a> as Response>::CRC,
            flags,
        ) {
            Ok(words) => words,
            Err(BlockingError::Op(Error::ResponseTimeout)) if stop => return Ok(stop_unanswered()),
            Err(e) => return Err(e.into()),
        };
        Ok(<C::Resp<'a> as Response>::from_words(&words))
    }

    async fn read_blocks<'a, C>(
        &mut self,
        mut cmd: C,
        auto_stop: bool,
    ) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::BlockReadCommand + 'a,
    {
        // esp-hal's blocking transfer always stops a multi-block read itself (CMD12).
        let (index, arg, size) = (C::INDEX, cmd.arg(), cmd.block_size().len());
        let buf = &mut *cmd.buf();
        self.count.check(buf.len(), size, auto_stop)?;
        let words = self.read(index, arg, buf, size)?;
        Ok(<C::Resp<'a> as Response>::from_words(&words))
    }

    // A write path must take `count` too (and see `sdio`'s ACMD23), or a stale count carries over.
    async fn write_blocks<'a, C>(
        &mut self,
        _cmd: C,
        _auto_stop: bool,
    ) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::BlockWriteCommand + 'a,
    {
        Err(MmcError::Unsupported)
    }

    async fn read_bytes<'a, C>(&mut self, mut cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::ByteReadCommand + 'a,
    {
        let (index, arg, size) = (C::INDEX, cmd.arg(), cmd.byte_count());
        let words = self.read(index, arg, &mut *cmd.buf(), size)?;
        Ok(<C::Resp<'a> as Response>::from_words(&words))
    }

    async fn write_bytes<'a, C>(&mut self, _cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::ByteWriteCommand + 'a,
    {
        Err(MmcError::Unsupported)
    }

    async fn init_idle(&mut self, hz: u32) -> Result<(), MmcError> {
        self.count = KeptCount::default();
        self.slot.set_bus_low_level(BusWidth::Bit1, hz)?;
        Ok(self.slot.send_init_sequence()?)
    }

    fn set_bus(&mut self, width: sdio::BusWidth, hz: u32) -> Result<(), MmcError> {
        if !matches!(width, sdio::BusWidth::W1) || hz > CLOCK_HZ {
            return Err(MmcError::Unsupported);
        }
        Ok(self.slot.set_bus_low_level(BusWidth::Bit1, hz)?)
    }

    fn supports_mmc(&self) -> bool {
        true
    }

    fn supports_auto_stop(&self) -> bool {
        true
    }

    fn supports_frequency(&self) -> u32 {
        CLOCK_HZ
    }
}

/// Slot 1 through esp-hal's interrupt-driven calls, as `sdio`'s bus, for Embassy tasks: what
/// [`BlockingBus`] does, around esp-hal's own bus. One data line at no more than `CLOCK_HZ`, reads
/// only (writes are refused with `Unsupported`).
pub struct AsyncBus<'d> {
    slot: Slot<'d, 1, Async>,
    count: KeptCount,
}

impl<'d> AsyncBus<'d> {
    #[must_use]
    pub fn new(slot: Slot<'d, 1, Async>) -> Self {
        AsyncBus {
            slot,
            count: KeptCount::default(),
        }
    }
}

impl sdio::MmcBus for AsyncBus<'_> {
    async fn send_command<'a, C>(&mut self, cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::ControlCommand + 'a,
    {
        if C::INDEX == SET_BLOCK_COUNT {
            return Ok(self.count.keep(cmd.arg()));
        }
        match self.slot.send_command(cmd).await {
            // esp-hal reports a response timeout, a busy line that never frees and a missing card
            // all as `Timeout`, so here a CMD12 that timed out in any of these ways counts as
            // unanswered (the blocking bus takes only a response timeout).
            Err(MmcError::Timeout) if C::INDEX == STOP_TRANSMISSION => Ok(stop_unanswered()),
            result => result,
        }
    }

    async fn read_blocks<'a, C>(
        &mut self,
        mut cmd: C,
        auto_stop: bool,
    ) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::BlockReadCommand + 'a,
    {
        // esp-hal's interrupt-driven transfer stops a read only when asked: a multi-block one is
        // asked to, also after a CMD23 kept back.
        let (len, size) = (cmd.buf().len(), cmd.block_size().len());
        self.count.check(len, size, auto_stop)?;
        self.slot.read_blocks(cmd, len > size).await
    }

    // As `BlockingBus`'s: a write path must take `count` too.
    async fn write_blocks<'a, C>(
        &mut self,
        _cmd: C,
        _auto_stop: bool,
    ) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::BlockWriteCommand + 'a,
    {
        Err(MmcError::Unsupported)
    }

    async fn read_bytes<'a, C>(&mut self, cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::ByteReadCommand + 'a,
    {
        self.slot.read_bytes(cmd).await
    }

    async fn write_bytes<'a, C>(&mut self, _cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: sdio::ByteWriteCommand + 'a,
    {
        Err(MmcError::Unsupported)
    }

    async fn init_idle(&mut self, hz: u32) -> Result<(), MmcError> {
        self.count = KeptCount::default();
        self.slot.init_idle(hz).await
    }

    fn set_bus(&mut self, width: sdio::BusWidth, hz: u32) -> Result<(), MmcError> {
        if !matches!(width, sdio::BusWidth::W1) || hz > CLOCK_HZ {
            return Err(MmcError::Unsupported);
        }
        self.slot.set_bus(width, hz)
    }

    fn supports_mmc(&self) -> bool {
        true
    }

    fn supports_auto_stop(&self) -> bool {
        true
    }

    fn supports_frequency(&self) -> u32 {
        CLOCK_HZ
    }
}

/// The card on slot 1 over the bus `B`, in 512-byte blocks.
pub type SdCard<B, D> = sdio::BlockDevice<sdio::sd::Card, B, D, 512>;
/// The card over the blocking bus.
pub type Card<'d> = SdCard<BlockingBus<'d>, SpinDelay>;
/// The card over the interrupt-driven bus.
pub type AsyncCard<'d> = SdCard<AsyncBus<'d>, embassy_time::Delay>;

/// A block in the alignment the card's transfers take.
pub type CardBlock = Aligned<A4, [u8; 512]>;

/// Mounts the card: up to `TRIES` tries, each after a pulse of the enable (high, card off, for
/// 80 ms, then low, on, for 120 ms, timed by `delay`) and proven by a read of block 0; `on_try`
/// hears of each before it starts. The try that worked, or `None`.
pub async fn mount_with<B: sdio::MmcBus, D: DelayNs>(
    card: &mut SdCard<B, D>,
    enable: &mut impl OutputPin,
    delay: &mut impl DelayNs,
    mut on_try: impl FnMut(u8),
) -> Option<u8> {
    let mut block0: [CardBlock; 1] = [Aligned([0; 512])];
    for attempt in 1..=TRIES {
        on_try(attempt);
        // An enable that fails to switch shows as a failed try.
        let _ = enable.set_high();
        delay.delay_ms(80).await;
        let _ = enable.set_low();
        delay.delay_ms(120).await;
        if card.reacquire(CLOCK_HZ).await.is_err() {
            continue;
        }
        if card.read(0, &mut block0).await.is_ok() {
            return Some(attempt);
        }
    }
    None
}

/// [`mount_with`] over the blocking bus, its pulses busy-waited.
pub fn mount(card: &mut Card<'_>, enable: &mut Output<'_>, on_try: impl FnMut(u8)) -> Option<u8> {
    ready(mount_with(
        card,
        enable,
        &mut SpinDelay(Delay::new()),
        on_try,
    ))
    .flatten()
}

/// The card's FAT volume over the blocking bus.
pub type CardVolume<'d> = Volume<CardBlocks<BlockingBus<'d>, SpinDelay>>;

/// Why the card's volume could not be opened.
#[derive(Debug)]
pub enum OpenError {
    /// The controller gave no slot 1.
    Slot,
    /// No try at mounting the card worked.
    NotMounted,
    /// The bus did not finish at once (it always does).
    NotReady,
    Volume(purestwhite_fat::Error<MmcError>),
}

/// The card's FAT volume through `controller`'s slot 1 on its pins and the blocking bus, mounted
/// as the C++ firmware mounts it ([`mount`], `enable` the card's supply); `on_try` hears of each
/// try at the card, which a card that stalls can make long.
///
/// # Errors
/// The slot, the card or its volume could not be had.
pub fn open_volume<'d>(
    controller: &'d SdHostController<'_>,
    (clk, cmd, dat0): (GPIO41<'d>, GPIO42<'d>, GPIO40<'d>),
    enable: &mut Output<'_>,
    on_try: impl FnMut(u8),
) -> Result<CardVolume<'d>, OpenError> {
    let slot = controller
        .slot::<1>(SlotConfig::default())
        .map_err(|_| OpenError::Slot)?
        .with_clk(clk)
        .with_cmd(cmd)
        .with_data0(dat0);
    let mut card: Card<'d> = Card::new_uninit(BlockingBus::new(slot), SpinDelay(Delay::new()));
    mount(&mut card, enable, on_try).ok_or(OpenError::NotMounted)?;
    ready(Volume::mount(CardBlocks::new(card)))
        .ok_or(OpenError::NotReady)?
        .map_err(OpenError::Volume)
}

/// The mounted card as `purestwhite-fat`'s block device, which can only read: up to `RUN` blocks
/// a card read, through a buffer the controller's DMA takes.
pub struct CardBlocks<B: sdio::MmcBus, D: DelayNs> {
    card: SdCard<B, D>,
    buffer: [CardBlock; RUN],
    /// Blocks a card read takes at most, 1 to `RUN`.
    run: usize,
}

impl<B: sdio::MmcBus, D: DelayNs> CardBlocks<B, D> {
    #[must_use]
    pub fn new(card: SdCard<B, D>) -> Self {
        CardBlocks {
            card,
            buffer: core::array::from_fn(|_| Aligned([0; 512])),
            run: RUN,
        }
    }

    /// The card's size in blocks.
    #[must_use]
    pub fn blocks(&self) -> u32 {
        u32::try_from(self.card.card().csd.block_count()).unwrap_or(u32::MAX)
    }

    /// Whether the card takes CMD23, so that `sdio` asks for it (and the bus keeps it back).
    #[must_use]
    pub fn takes_cmd23(&self) -> bool {
        self.card.card().scr.supports_cmd23()
    }

    /// Reads at most `blocks` blocks a card read from now on, 1 to `RUN`: with 1, every read is a
    /// single-block one (CMD17).
    pub fn set_run(&mut self, blocks: usize) {
        self.run = blocks.clamp(1, RUN);
    }
}

impl<B: sdio::MmcBus, D: DelayNs> purestwhite_fat::BlockDevice for CardBlocks<B, D> {
    type Error = MmcError;

    async fn read(
        &mut self,
        start: u32,
        blocks: &mut [purestwhite_fat::Block],
    ) -> Result<(), MmcError> {
        let mut at = start;
        for chunk in blocks.chunks_mut(self.run) {
            let buffer = self
                .buffer
                .get_mut(..chunk.len())
                .ok_or(MmcError::BlockSize)?;
            self.card.read(at, buffer).await?;
            for (to, from) in chunk.iter_mut().zip(buffer.iter()) {
                to.copy_from_slice(&**from);
            }
            let read = u32::try_from(chunk.len()).map_err(|_| MmcError::BlockSize)?;
            at = at.checked_add(read).ok_or(MmcError::BlockSize)?;
        }
        Ok(())
    }
}
