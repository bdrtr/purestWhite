//! The X4 Pro's SD card for the test images, mounted as freeink-sdk's `SdmmcBlockDevice` does:
//! SDMMC slot 1 with one data line (CLK 41, CMD 42, DAT0 40, internal pull-ups) at the default
//! 20 MHz, and GPIO5, an active-low enable, pulsed high for 80 ms and then held low for 120 ms
//! before each of up to four tries, each proven by a real read of block 0. The card protocol is
//! `sdio`'s, over esp-hal's blocking slot calls ([`BlockingBus`], test-image glue: the async slot's
//! interrupts crash eink-qemu 0.9.2); the FAT volume is our `purestwhite-fat`'s, over
//! [`CardBlocks`], which can only read. On this blocking bus, reads of several blocks are one
//! open-ended CMD18 that the controller stops with CMD12, as ESP-IDF's driver reads them. Every future here is ready at its
//! first poll; the blocking calls end at esp-hal's own limits, the slowest a data phase at the controller's data
//! timeout (about 42 s at the 400 kHz the card starts at), and the RTC watchdog backs them.

use crate::ready;
use aligned::{A4, Aligned};
use block_device_driver::BlockDevice as _;
use esp_hal::Blocking;
use esp_hal::delay::Delay;
use esp_hal::dma::aligned::DmaAlignedMut;
use esp_hal::gpio::{Level, Output};
use esp_hal::sdmmc::{BlockingError, BusWidth, CommandFlags, Error, ResponseLen, Slot};
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
impl embedded_hal_async::delay::DelayNs for SpinDelay {
    async fn delay_ns(&mut self, ns: u32) {
        embedded_hal::delay::DelayNs::delay_ns(&mut self.0, ns);
    }
}

/// Slot 1 through esp-hal's blocking calls, as `sdio`'s bus, for the test images: each call runs
/// to its end and its future is ready at once. One data line, reads only (writes are refused with
/// `Unsupported`).
pub struct BlockingBus<'d> {
    slot: Slot<'d, 1, Blocking>,
    /// The count of the CMD23 kept back (see `send_command`), for the read right after it.
    count: Option<u32>,
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
        BlockingBus { slot, count: None }
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
        // `sdio` sets the count of a multi-block read with CMD23 when the card takes it. Like
        // ESP-IDF's driver, which the SDK uses, we leave the read open-ended and the controller
        // stops it itself: CMD23 is not sent, and its count only checks the read that follows.
        if C::INDEX == SET_BLOCK_COUNT {
            self.count = Some(cmd.arg());
            return Ok(<C::Resp<'a> as Response>::from_words(&[0; 4]));
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
            // `sdio` stops before every read that follows a failed one, also after a new mount,
            // where the card is idle and does not answer a stop: there was nothing to stop. The
            // status poll `sdio` sends after a stop clears the card's illegal-command flag.
            Err(BlockingError::Op(Error::ResponseTimeout)) if stop => [0; 4],
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
        // esp-hal's blocking transfer always stops a multi-block read itself (CMD12): what `sdio`
        // asks with `auto_stop`, or after a CMD23 kept back for as many blocks.
        let (index, arg, size) = (C::INDEX, cmd.arg(), cmd.block_size().len());
        let buf = &mut *cmd.buf();
        let count = self.count.take();
        let blocks = u32::try_from(buf.len() / size.max(1)).ok();
        if buf.len() > size && !auto_stop && (count.is_none() || count != blocks) {
            return Err(MmcError::Unsupported);
        }
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
        self.count = None;
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

/// The card on slot 1, in 512-byte blocks.
pub type Card<'d> = sdio::BlockDevice<sdio::sd::Card, BlockingBus<'d>, SpinDelay, 512>;

/// A block in the alignment the card's transfers take.
pub type CardBlock = Aligned<A4, [u8; 512]>;

/// The enable's pulse before a try: high (card off) for 80 ms, then low (on) for 120 ms.
pub fn power_cycle(enable: &mut Output<'_>, delay: &Delay) {
    enable.set_level(Level::High);
    delay.delay_millis(80);
    enable.set_level(Level::Low);
    delay.delay_millis(120);
}

/// Mounts the card: up to `TRIES` tries, each after a pulse of the enable and proven by a read of
/// block 0; `on_try` hears of each before it starts. The try that worked, or `None`.
pub fn mount(
    card: &mut Card<'_>,
    enable: &mut Output<'_>,
    mut on_try: impl FnMut(u8),
) -> Option<u8> {
    let delay = Delay::new();
    let mut block0: [CardBlock; 1] = [Aligned([0; 512])];
    for attempt in 1..=TRIES {
        on_try(attempt);
        power_cycle(enable, &delay);
        if !matches!(ready(card.reacquire(CLOCK_HZ)), Some(Ok(()))) {
            continue;
        }
        if matches!(ready(card.read(0, &mut block0)), Some(Ok(()))) {
            return Some(attempt);
        }
    }
    None
}

/// The mounted card as `purestwhite-fat`'s block device, which can only read: up to `RUN` blocks
/// a card read, through a buffer the controller's DMA takes.
pub struct CardBlocks<'d> {
    card: Card<'d>,
    buffer: [CardBlock; RUN],
    /// Blocks a card read takes at most, 1 to `RUN`.
    run: usize,
}

impl<'d> CardBlocks<'d> {
    #[must_use]
    pub fn new(card: Card<'d>) -> Self {
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

    /// Whether the card takes CMD23, so that `sdio` asks for it (and [`BlockingBus`] keeps it back).
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

impl purestwhite_fat::BlockDevice for CardBlocks<'_> {
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
