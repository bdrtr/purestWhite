# Stage 6: the platform

Stage 6 of the [porting order](../README.md) moves the platform (hal, the freeink-sdk drivers and
`main`) to bare-metal Rust on esp-hal and Embassy. At the end, Arduino goes away. The plan has
three phases:

1. The X4 Pro's drivers and the application logic are prepared in Rust and tried inside the C++
   firmware. The drivers use embedded-hal traits; no new `extern "C"` is added to ESP-IDF.
2. A separate bare-metal firmware is tried in the second OTA slot, with features still missing.
3. Once nothing is missing, the old firmware is left behind.

What stays behind FFI: wolfSSL and FreeType.

How the bare-metal firmware's code is organized, its test images, and how to build them, run them
in the emulator and put them on the device: [the firmware's README](../firmware/README.md). What
the emulator models of each HAL part: [EMULATOR-HAL.md](../EMULATOR-HAL.md).

Every `purestwhite-*` crate and `rust/firmware` have `too-many-lines-threshold = 50` in their
`clippy.toml`: no god functions.

✅ marks a part that is done. "Bare-metal only" marks a part that only the bare-metal firmware
uses.

## The first step of phase 2

`rust/firmware` (its own workspace; it builds only for the ESP32-S3). A smoke test that:

- boots once in the second OTA slot and hands the next boot back to the C++ firmware;
- starts esp-rtos and Embassy and runs our clock, fuel gauge and touch drivers unchanged on
  esp-hal's I2C (while it reads touches, it paints the UC8279 or UC8179 panel black, then white,
  with the display driver below, through blocking SPI for now);
- leaves its report to the C++ firmware in RTC slow memory (`HalSystem::saveBareMetalReport`,
  `purestwhite-bare-metal.txt` on the card).

The smoke test is now the `smoke-check` image (`src/bin/smoke_check.rs`; it was the
`purestwhite-firmware` binary). It brings up the panel with `panel::open`, the one way to do so in
`rust/firmware`.

Also `rollback-check`, which tests only this way back.

## Real-time clock ✅

HalClock's BM8563 (PCF8563 compatible), in the new `purestwhite-rtc` crate: finding it and turning
CLKOUT off (three tries), reading the BCD time, the voltage-low flag and the century bit, writing
the time, and converting from UTC to Unix time. Its ABI is `purestwhite/rtc.h`.

The bus stays in Arduino `Wire` (`lib/hal/WireBus.h`), because the touch and fuel gauge drivers
share it. The drivers see the bus and the delays through embedded-hal's `I2c` and `DelayNs`
traits. The bridge's ABI is `purestwhite/i2c.h` (write and write-read only; delays in whole
milliseconds). With esp-hal the drivers do not change: in the bare-metal firmware, all drivers on
I2C0 (the clock included, which blocks today) move to one shared async bus.

Local time, the time zone and NTP are in newlib and ESP-IDF. `test/rtc_pcf8563` compares the driver
with the SDK's at the bus level.

## Fuel gauge ✅

HalPowerManager's CW2017, in the new `purestwhite-gauge` crate:

- on the first read, checking the 80-byte battery profile and loading it if needed, a soft reset,
  and waiting for the calculation engine to be ready;
- recovering, on the next read, a gauge found asleep or restarted;
- a one-second wait after a failed first read;
- reading the percentage.

It uses embedded-hal-async, because the first read can wait up to 4 seconds; the bridge drives it
with futures that are ready at once. Its ABI is `purestwhite/gauge.h`; the state sits in
HalPowerManager as an opaque `pw_cw2017`.

The charge state (the STAT pin) and the other boards' gauges are in the SDK's `BatteryMonitor`.
`test/cw2017_gauge` compares the two with twin chips in a single session of 30,000 reads.

## Touch driver (bare-metal only)

The X4 Pro's GT911, in the new `purestwhite-gt911` crate, with embedded-hal-async: selecting the
address with reset and the INT level, probing the two addresses, the status register, reading the
four contact records in one transfer, swapping/mapping/flipping the axes, and the Home key. RST and
INT use a small `ResetPins` trait of the crate's own, because INT is first driven and then turned
into an input.

What a touch means is in the new `purestwhite-touch` crate: the single-finger classification on the
GT911 path of the SDK's `InputManager` (`serviceTouch`, `pollGt911`). That is the press and release
edges, the short touch (tap), the swipe, the long press, `suppressTouchContact`,
`isTouchTapCandidate`, `isTouchHeldAt`, `lastTouchHeldMs`, and the capacitive Home key's press,
short press and long press. A touch that a second finger joins counts as multi-contact, as in the
SDK, and gives no single-finger event. Each poll returns an `Events`, which also holds the
contact's sequence number (a consumer at the other end of a channel uses it to drop the tap of a
contact it long-pressed; the SDK's callers call `suppress`). The SDK's two- to four-finger gestures
(scroll, rotate, pinch) are not there yet.

`test/gt911_touch` feeds two model chips with the same fingers (200,000 polls in each of two
configurations: up to three fingers, more contacts reported than stored, polls without a new frame
and failed polls, the Home key, an occasional `suppress`) and after each poll compares the two
sides' queries at the same normalized points (`purestwhite/touch.h`, only for this test). It was
checked with mutations (14 of 15 mutations caught, one equivalent).

In the C++ firmware, touch stays in the SDK's `InputManager`, because there is no hook to change it
from outside: it gets wired in there when the touch state machine moves to Rust. `test/gt911_touch`
compares the SDK's GT911 path with our ABI (`purestwhite/gt911.h`, only this test for now) on the
same model chip, at the pin, delay and bus level.

## PSRAM (bare-metal only)

The X4 Pro's 8 MB octal PSRAM, with esp-hal's `Psram`, at 40 MHz. The C++ firmware (the Arduino
`dio_opi` variant) runs it at 80 MHz, but for that ESP-IDF tunes the sampling timing at every boot,
and esp-hal 1.2.2 does not do this tuning yet. 80 MHz comes once the tuning is ported. The setting
is `psram_config` in `rust/firmware/src/board.rs`, next to `PSRAM_MHZ`.

It is added to the esp-alloc heap as an external memory region (in this image). The rule in the
real firmware: DMA buffers and everything accessed while the flash is written, task stacks
included, stay in internal memory; PSRAM is given to large data only when asked for explicitly,
with esp-alloc's `ExternalMemory`. PSRAM added to the general heap silently moves ordinary
allocations there once internal memory is full.

The `psram-check` image takes nearly all of it in one allocation, fills it with a pattern and with
its inverse, and reads it back; `emu/baremetal-psram.txt`.

## Buttons (bare-metal only)

The X4 Pro's Left (GPIO0) and Right (GPIO7) keys, the SDK's Up and Down, and Power (GPIO3); active
low, with internal pull-ups. Debouncing works as in the SDK's `InputManager`: a state that stays
unchanged for longer than 5 ms passes, and the pressed and released buttons and the press durations
are kept. In the new `purestwhite-buttons` crate, with time that wraps around like `millis()`. In
the C++ firmware, the buttons stay in the SDK's `InputManager`.

The `button-check` image; `emu/baremetal-buttons.txt`. For a task that wakes only on a pin change,
`Buttons::deadline` gives the time the debounce ends.

Deep sleep uses esp-hal's `LowPower`. In the `sleep-check` diagnostic image on the device, the
light sleep woke on the timer and on Power, the deep sleep on Power (ext1)
(`emu/baremetal-sleep.txt`). That it did not wake on the first try was because the first sleep
after boot was a deep sleep (see the input and sleep section below). In the diagnostic image the
watchdog first makes a note, then a core reset that keeps the latch, and last a system reset. For
the sleep, `hand_back` deletes the bootloader's "fast boot from deep sleep" record (otherwise the
wake boots the image again), and the C++ firmware adds what woke the chip (and what reset it) when
the report's last line is the image's sleep line.

## Input and sleep (bare-metal only)

`rust/firmware`'s `input` module: the three buttons (with `purestwhite-buttons`) and the GT911 touch
panel (with `purestwhite-gt911` and `purestwhite-touch`) are polled every 15 ms, like the SDK's
`beginAsync` default. `Input::poll` waits only for I2C transfers, so it can be driven both from an
Embassy task and from a plain loop (the emulator cannot run esp-rtos). Each poll gives the button
changes, the touch events, the finger's position while dragging, and the touch candidate for
pressed feedback.

The C++ firmware's rule for sleeping with Power is in `purestwhite-buttons`'s `PowerHold`:

- sleep when Power is held longer than 400 ms (`getPowerButtonDuration` while the short-press
  action is the default; the duration is a parameter);
- not when Right is held too (that is a screenshot);
- the press that woke the device does not count until it is released (and Power held since boot
  does not count until its debounce ends either).

Button and touch activity reset the idle time; the caller keeps the time. Not ported:
`allowSleepAt`, the front light on a double press of Power on the X4 Pro, and the times that come
from the settings (the test image waits 60 s; the C++ default is 10 minutes).

The `sleep` module enters deep sleep as C++'s `HalPowerManager::startDeepSleep` does: the rails are
held as the SDK leaves them, Power wakes the chip through the low-power path, and the sleep first
waits for Power to be released.

On the device, with esp-hal 1.2.2, the first sleep after boot does not end when it is a deep sleep:
neither Power nor the timer wakes it, and most likely the sleep never starts (in `button-check`'s
first version the watchdog was off and the owner restarted the device; in `input-check`'s first run
the watchdog reset it after 90 seconds). With a light sleep first, it starts and wakes
(`sleep-check`, `input-check`'s second run).

The cause: just before the sleep request, esp-hal disconnects every digital pin without a hold, the
flash's (GPIO 27-32) included (ESP-IDF skips the flash pins). Then, if the running code is not in
the cache, the chip cannot reach the flash and hangs. esp-hal does this only while the digital
pins' auto-hold is on. This RTC setting survives the reset, and it comes on from the C++ firmware.
`sleep::deep_sleep` turns it off. Confirmed on the device (`input-check`'s fourth run: the setting
was found on, and with it off the first deep sleep, with no light sleep before it, woke on Power).

In the phase-2 images the RTC watchdog counts in sleep too (`boot::watch_sleep`: first a core
reset that keeps the latch, then a system reset); this is shared with `sleep-check`.

The `input-check` image, `emu/baremetal-input.txt`: in the emulator the counts, the sleep when
Power is held, and the wake on Power passed.

## On-screen feedback (bare-metal only)

The first image that answers input on screen, `feedback-check`:

- four key boxes at the top (Left ←, Right →, Power ○, Home with a house);
- for each press (for Home, each tap) a small square under the box;
- for a tap, a dot where the finger touched; for a long press, a ring; for a swipe, a line with a
  dot at its end;
- before sleep, a crescent moon with a full refresh.

Drawing uses `bembeyaz-gfx`, upright as the device is held (480×800). The touch point comes from the
panel pixel through `Canvas::panel_to_logical` (the integer inverse of C++'s
`GfxRenderer::tapToLogical`; its floating-point twin is `tap_to_logical`). The panel is driven with
`purestwhite-epd`. Refreshes are blocking fast refreshes, with a half refresh after every 15 fast
ones; a press or a touch that starts and ends while a refresh runs is not read. The sleep goes
through `boot::report_and_sleep`; the panel is first put into deep sleep, as in C++. There is no
floating point in the bare-metal path, because eink-qemu 0.9.2 crashes on floating-point operations
("[Digital Signature] Invalid padding"). `emu/baremetal-feedback.txt`: passed in the emulator (63
steps, the marks under the finger, the 16th refresh a half one).

What every phase-2 image shares is in `rust/firmware`'s `boot` module, which the real firmware's
image uses too while the C++ firmware is the known-good one: the start (`start`: the watchdog armed,
the latch held, the report begun, the next boot handed back to C++), the report, and the deep sleep
bounded by a timer and by the watchdog counting through it, marked on the report's last line. What
only the test images use (the touch setup without an executor) is in its `test_image` module, away
from the real firmware's path. The otadata choice behind handing back is in the `purestwhite-ota`
crate, tested on the host.

Text in the SD card's font (D2): `bembeyaz-font`'s new `face` module reads one style of a `.cpfont`
directly from the file (the header and the style's interval table once; each glyph's record and
bitmap while a line is laid out), for short on-screen texts. The pen moves like C++'s `drawText`,
rounding each glyph's 12.4 fixed-point advance to the nearest pixel. `SdCardFont`'s page cache,
kerning, ligatures and combining marks are not there (book text must be laid out with them). A
character missing from the font is skipped (C++ draws a filled box instead). The host tests use the
emulator's `LiberationSerif_14.cpfont`.

`Writer` (now in `purestwhite-render`'s `text` module) holds only the style; the font's bytes come
from a `Source` at each call. The firmware loads the font whole into memory and draws it through
`bembeyaz_font::Memory`, which it passes to `Writer` at each call. Glyphs are drawn upright with
`bembeyaz-gfx`.

When the card has `/.fonts/LiberationSerif/LiberationSerif_14.cpfont`, `feedback-check` writes the
boxes' names under them (Sol, Sağ, Güç, Home), the counts "dokun … kaydır … uzun …" at the bottom,
and "İyi geceler" under the moon. Without the card or the font it draws only the shapes (the
report's start line ends in `font N KB N ms` or `font none`). These on-screen texts are Turkish on
purpose: they exercise the font's Turkish letters. It passed in the emulator with Turkish letters.

The font is loaded whole into PSRAM at boot (D3: `storage::load_whole`, an `async fn` that takes
the allocator as a parameter and does not depend on the executor; here with esp-alloc's
`ExternalMemory`; the card's DMA reads into its own internal buffer and the bytes are copied from
there; in the emulator 363 KB in 334 ms). It is drawn from memory through `bembeyaz_font::Memory`:
drawing never touches the card, so it stays the same under Embassy. PSRAM is added to the heap
after the internal heap: ordinary allocations go to internal memory first and spill into PSRAM if
internal memory is full (harmless in a test image; in the real firmware PSRAM must be kept in a
separate heap, or the buffers on the DMA and flash path must be allocated explicitly from internal
memory).

Next: the Embassy main program. In the real firmware's sleep, the SD lines must also be held low,
as in the SDK (the test images do not hold them).

## SD card (bare-metal only, read only for now)

The X4 Pro's SDMMC slot, as in the SDK's `SdmmcBlockDevice`: 1 bit, CLK 41, CMD 42, DAT0 40,
ESP-IDF's default 20 MHz, a pulse of the GPIO5 enable before each try, four tries, each one proven
by a real read of block 0.

The card protocol is the `sdio` crate's, over esp-hal's slot: calls without interrupts
(`BlockingBus`) for images that also run in the emulator, and interrupt-driven calls (`AsyncBus`,
device only: eink-qemu 0.9.2 crashes on SDMMC and GPIO interrupts) for Embassy tasks. Several
blocks are read in one command (up to 32 blocks, 16 KiB; a longer read is several commands), copied
from a buffer the DMA can reach. On both buses, as in ESP-IDF's driver, the controller itself stops
the open-ended CMD18 with CMD12; `sdio`'s CMD23 is not sent to the card (esp-hal's interrupt-driven
slot could send it, but this is the path tested on the device and the one the SDK uses).

The FAT volume is our own `purestwhite-fat` (by the owner's decision, instead of a third-party
crate; following Microsoft's FAT specification):

- FAT16 and FAT32 (an "unsupported" error for FAT12 and exFAT);
- the MBR's first FAT partition (even with an exFAT or NTFS partition before it), or a card with no
  partition table;
- UTF-16 long names (the checksum and the order of the parts are checked; the non-ASCII bytes of
  short names are CP437);
- path lookup without regard to case, like the C++ firmware's SdFat, so that it finds the same file:
  each UTF-16 unit is upcased with SdFat's own table (Turkish ı does not become I; ı and i are
  different names; the table was compared with SdFat's on all 65,536 units), and the leading
  spaces and the trailing dots and spaces of the name looked up do not count; from the root or
  from a directory;
- reading consecutive clusters straight into the caller's buffer in one device call; seeking
  forward and back.

A broken chain (a cluster out of range, a cluster that points to itself, a chain that ends early),
an inconsistent boot sector (a volume that overflows or is longer than its partition, a FAT too
small for its clusters), a directory entry without a cluster, and a file larger than the volume are
errors, not hangs or overflows. A directory is walked for at most 65,536 entries, a file chain for
at most as many clusters as the volume has.

Writing goes through a separate trait (`BlockWrite`), only on a device that provides it:

- creating a file with a long name (the short name, like SdFat's, from ASCII only, with a `~N` tail
  when needed; a long name only when the name is not 8.3 itself);
- writing at a position and growing; shortening (`truncate`); deleting;
- creating a directory (with `.` and `..`; in one under the root, `..` is 0) and deleting an empty
  directory;
- renaming and moving (a directory cannot be moved under itself; a moved directory's `..` is
  updated; the new entry is prepared first, then the old one is deleted and the new one written, so
  an interruption can lose the entry but never leaves two entries on the same chain);
- growing a chain and a directory with clusters when needed (a directory's new cluster is zeroed);
- every FAT change going to the FAT copies in order (on FAT32 with mirroring off, only to the one in
  use).

As in SdFat, a file's entry (size, first cluster) is written on `sync` or `close`. FAT32's FSInfo
free cluster count is set to "unknown" at the first FAT change (SdFat does not keep it up to date,
so it cannot be trusted). The write order (data, FAT, entry; for a deleted entry, first the short
entry, then the long name parts from last to first) is such that an operation cut between two
blocks leaves at most lost clusters, a file shorter than its chain, an orphaned long name, or a `..`
that points at the old parent directory. No entry points at a free cluster, and no two entries
share a cluster (a scenario with create, write, delete, directory, rename and truncate is tested by
cutting it at every write point).

An open file must not be deleted or renamed (as in SdFat). If it is deleted, its next write and
`sync` are refused when its entry has been reused with another short name or cluster, and it does
not write into another file's clusters. A full volume is remembered until a cluster becomes free.
Within one mount, only one writer (our layer or SdFat) may write to the card, because each keeps
its own FAT cache and free cluster hints; if both write, they give the same cluster to two files.
`Volume::serial()` gives the volume's serial number (to recognize the book card in W3).

In `sd-check` the device is read only. The crate is tested with images made by `mkfs.fat` and
mtools (FAT16/32, FAT32 with few clusters, with and without MBR, clusters of 1/4/8 blocks, Turkish
long names, a directory over several clusters, deleted entries, a fragmented file) and with images
corrupted by hand. What is written is checked after each test's writes with `fsck.fat -n` (the FAT
copies included) and read back by its long name with mtools (both FAT types, MBR, a full FAT16 root,
a full volume, a growing directory of 200 files, short name tails past the first 1024, clusters
above 65535). It was checked with mutations. The tests need dosfstools and mtools and fail without
them (`PW_FAT_SKIP_TOOLS=1` skips them).

Later: trying writes on the device (on a separate test card), exFAT, and reading without the buffer
copy (`block-device-driver`'s aligned block trait; the buffer's place then moves to the caller
too). `embedded-sdmmc`, used for a while in the first slice, was removed.

`rust/firmware/src/sd.rs`, the `sd-check` image, `emu/baremetal-sd.txt`. When the write path comes,
`KeptCount`, `CardBlocks` and `mount_with` will move into a `purestwhite-sdcard` crate, to be tested
on the host with a fake `MmcBus`.

## Display driver (bare-metal only)

The X4 Pro panels with a UC8279 and with a UC8179: the SDK's `Uc8279X4Driver` and `Uc8179Driver`.
The controller is found in `purestwhite-epd::probe` with the SDK's `XteinkDetect` probe (FLG/VER
half-duplex, MTP if needed), and `test/epd_probe` compares it with the SDK's at the line level.

The driver, in the new `purestwhite-epd` crate, with embedded-hal-async: the bus framing, the reset
pulse and the BUSY waits come from the SDK's `EpdBus`; init, full/half/fast refresh, the white fill
of the hidden gates and deep sleep from `Uc8279X4Driver` and `Uc8179Driver`. The UC8179's 4-level
gray passes are there too (`Uc8179Driver`'s Overlay, Absolute and Direct modes: the reader's
antialiased text, the image viewer and the sleep screen; custom waveform tables; the non-flashing
`XTF_PRE_BW_MID` transition after a gray page; a buffer given to the driver instead of the SDK's
black-and-white base copy in PSRAM). The drivers take any `Interface` (the bus operations they
use), which `Bus` implements over embedded-hal; the UC8179 driver is split into submodules.

The UC8279's gray path and the SSD1677 controller are not there yet. In the C++ firmware, the panel
stays in the SDK's `FreeInkDisplay`.

`test/epd_panels` compares with the SDK's bus and its two drivers at the level of chip-select
frames, reset, BUSY reads and waits, and the gray passes in two builds, with and without PSRAM
(`EpdGrayTest`, `EpdGrayNoBaseTest`) (`purestwhite/epd.h`, only these tests for now). The
`display-check` image, without esp-rtos, draws the pattern in the emulator, and on a UC8179 the
bands of the four gray passes too: `emu/baremetal-display.txt` and
`emu/baremetal-display-gray.txt`.

## Embassy tasks (device only)

Three tasks at once on esp-rtos, none of them polling:

- the buttons wake on a pin level interrupt or when the debounce ends;
- SD reads the image's own file again and again over the interrupt-driven bus;
- a 10 ms heartbeat measures whether the executor starves.

The real input and storage layers will keep their state inside the tasks and talk over embassy-sync
channels; the global atomics in this image are only for a one-off diagnosis. The `tasks-check`
image.
