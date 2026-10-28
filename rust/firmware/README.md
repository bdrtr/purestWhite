# The bare-metal firmware

`rust/firmware` is a bare-metal Rust firmware for the Xteink X4 Pro (ESP32-S3), on esp-hal and
Embassy. It is phase 2 of the platform plan (see [Stage 6: the platform](../porting/6-platform.md)):
a separate firmware, tried in the second OTA slot beside the C++ firmware.

For now it is a set of test images (`src/bin/`); the real firmware's `main` is still to come (see
the [roadmap](ROADMAP.md): what comes next, and what must be true before the C++ firmware is left
behind). A test image runs once from the second OTA slot. First it hands the next boot back to the
C++ firmware; then it does its check, leaves a report in RTC slow memory and resets (or sleeps, and
the wake boots the C++ firmware). The C++ firmware saves the report to the SD card.

It is its own Cargo workspace and builds only for the ESP32-S3 (`xtensa-esp32s3-none-elf`, with the
`esp` toolchain from `rust-toolchain.toml` and `build-std` for `core` and `alloc`). The crates in
`rust/crates` build for the host too; the firmware uses them by path. Direct dependencies are
pinned to exact versions in `Cargo.toml` (esp-hal `=1.2.2`, esp-rtos `=0.4.0`, ...).

## Code layout

`src/lib.rs` holds the device layers. The layers do not depend on an executor: esp-rtos does not
run in the emulator, so an image without one drives their async calls over blocking buses with
`ready` (in `lib.rs`), which runs a future whose awaits are all ready at once.

- `board`: the X4 Pro's wiring in one place. `Board::take` splits the chip's peripherals into the
  device's parts, handed over as they are, not yet set up: `Boot` (the RTC watchdog's timer, the
  power latch GPIO1, the flash and the second core), `PanelParts` (SPI2: SCLK 12, MOSI 11, CS 13,
  DC 18, RST 14, BUSY 6), `CardParts` (SDMMC slot 1: CLK 41, CMD 42, DAT0 40; its supply GPIO5,
  active low), `I2cParts` (I2C0: SDA 39, SCL 38; the touch panel's supply GPIO2, active low),
  `TouchParts` (GT911: RST 4, INT 10), `ButtonParts` (Left GPIO0, Right GPIO7, Power GPIO3), the
  PSRAM, the RTC's low-power control, and `RtosParts` (what esp-rtos runs on). Also
  `hold_power_latch`, `psram_config` and `PSRAM_MHZ` (40).
- `input`: the three buttons and the GT911 touch panel. `Input::poll` reads both once, as the SDK's
  `InputManager::update()` does, and gives what changed: button changes, a request to sleep (Power
  held longer than `POWER_HOLD_MS`, 400 ms), touch events, the point held and the tap candidate.
  The caller polls every `POLL_MS` (15 ms), from an Embassy task or a plain loop.
- `sleep`: deep sleep as the C++ firmware enters it (`HalPowerManager::startDeepSleep`): Power
  armed through the low-power path, the rails held, the digital pads' auto-hold turned off, then the
  sleep. The module's doc explains why the auto-hold must be off.
- `panel`: the panel for the test images: the controller probe and the `purestwhite-epd` driver for
  the controller found, over blocking SPI and delays whose futures are ready at once. `panel::open`
  is the one way to bring up the panel. A driver call may take at most 8 s; `finish_by` sets a time
  after which no call starts.
- `sd`: the SD card, mounted as freeink-sdk's `SdmmcBlockDevice` does. The card protocol is
  `sdio`'s over esp-hal's slot: `BlockingBus` (runs in the emulator) or `AsyncBus`
  (interrupt-driven, for Embassy tasks, device only). `CardBlocks` gives `purestwhite-fat` the
  card's blocks, read only.
- `storage`: `load_whole` reads a file from the card whole into memory from a given allocator
  (PSRAM through esp-alloc's `ExternalMemory` for a large file), for data that is read often and
  never changed, such as a font.
- `boot`: the start every image shares while the C++ firmware is the known-good one (below).
- `test_image`: what only the test images use, out of the real firmware's way (below).

What does not touch hardware is in `rust/crates`, where it builds and is tested on the host:
`purestwhite-app` (the application's state machine), `purestwhite-render` (draws its screens; its
`Writer` and `Text` draw text in one style of a `.cpfont`) and `purestwhite-services` (the device
traits and the loops between them). [DESIGN.md](DESIGN.md) explains how they fit together, and the
[host simulator](#run-in-the-host-simulator) runs them on a PC.

### boot

- `start`: the start every image shares. It arms the RTC watchdog (60 s, then a core reset) before
  anything that could hang, holds the power latch, begins the report with the image's title and
  saves it, then hands the next boot back and notes `next boot: C++ firmware`. If handing back
  fails, it notes `next boot: NOT SET (<why>)` and resets at once.
- `hand_back`: selects the C++ firmware's OTA slot (the one this image was not booted from) in
  otadata, as valid. The choice of the entry is in the `purestwhite-ota` crate, tested on the host;
  here the partition table and the flash are read and written. Before that it drops the
  bootloader's record of this image, so that a wake from deep sleep boots what otadata selects
  rather than this image again, and parks the second core.
- `Report`: the report in the last 256 bytes of RTC slow memory (magic "PWBT", length, then up to
  248 bytes of text), saved again after every stage. Text past its room is dropped and the last
  byte becomes `~`. After a panic, every image adds `PANIC` to the saved report and resets.
- `arm_watchdog`: the RTC watchdog, 60 s, then a core reset, which keeps the latch and the report.
- `watch_sleep`: keeps the RTC watchdog counting through a phase-2 image's deep sleep, so that a
  sleep that never ends still ends: first a core reset, which comes back to the C++ firmware; then
  a system reset, which on battery turns the device off. A firmware whose sleeps last hours must
  not call it.
- `report_and_sleep`, `sleep_when_released`: a deep sleep bounded by its timer and `watch_sleep`
  (`BoundedSleep`), and the sleep line `sleep: deep, <why>` at the end of the report, with a mark
  for each step of the sleep, so that a sleep that never ends shows its step;
  `sleep_when_released` first waits for Power to be up (or notes `power held`).

### test_image
- `open_touch`, `note_touch`: the touch panel through blocking calls, for images without an
  executor, and its report line (`touch: 0x5d` or `touch: none`).

## Test images

Each one is a binary in `src/bin/`. Images without esp-rtos also run in the emulator, each with
its scenario in `emu/`.

- `rollback-check`: the smallest image. It hands the next boot back, reports whether the device's
  bootloader rolls back on its own (`bootloader state: N`) and resets. Scenarios:
  `emu/baremetal-handback.txt`, and `emu/baremetal-handback-slot1.txt` with the C++ firmware in the
  second slot.
- `smoke-check`: starts esp-rtos and Embassy, runs the clock, fuel gauge and touch drivers on
  esp-hal's I2C, reads touches for 15 s with the panel black, then paints it white. Needs esp-rtos:
  device only.
- `button-check`: counts presses of Left, Right and Power for up to 40 s; Power held for a second
  ends the count early. `emu/baremetal-buttons.txt`.
- `display-check`: probes the panel's controller and draws a test pattern on a UC8279 or UC8179 (a
  full refresh, then a fast one); on a UC8179 four gray bands follow, in CrossPoint's grayscale
  passes. `emu/baremetal-display.txt` (UC8279), `emu/baremetal-display-gray.txt` (UC8179).
- `sd-check`: reads the card, read only: mounts it, opens the FAT volume, counts the root's entries,
  reads a small file in `.crosspoint`, and reads its own file one block per command and then many,
  timing both. `emu/baremetal-sd.txt`.
- `sleep-check`: a light sleep of 3 s on the timer, a light sleep until Power (or 20 s), then a deep
  sleep until Power (or 15 s). `emu/baremetal-sleep.txt`.
- `psram-check`: starts the PSRAM at 40 MHz, adds it to the heap, and fills nearly all of it twice
  through one allocation (a pattern and its inverse), reading it back. `emu/baremetal-psram.txt`.
- `tasks-check`: three Embassy tasks side by side for up to 30 s: the buttons, the card read again
  and again over the interrupt-driven bus, and a 10 ms heartbeat. Needs esp-rtos: device only
  (eink-qemu 0.9.2 also crashes on the GPIO and SDMMC interrupts).
- `input-check`: polls the buttons and the touch panel every 15 ms and counts what they give until
  the C++ firmware would sleep, then sleeps deeply until Power (or 60 s). `emu/baremetal-input.txt`.
- `feedback-check` (`src/bin/feedback_check/`): draws what the input gives on the panel, with the
  card's font loaded into PSRAM for the texts, then a moon before a deep sleep.
  `emu/baremetal-feedback.txt`.

Each report starts with the image's title (`purestWhite bare-metal <name> check`, or
`purestWhite feedback`) and `next boot: C++ firmware`. Images that reset end it with `done`; images
that sleep end it with the sleep line, and the C++ firmware then adds `wake: ...` and `reset: ...`.

## Build

```sh
cd rust/firmware
cargo build --release
```

This builds every image into `rust/firmware/target/xtensa-esp32s3-none-elf/release/<image>`. To
make a `.bin` the device can flash, from the repository root:

```sh
mkdir -p build/baremetal
espflash save-image --chip esp32s3 --flash-size 16mb \
    rust/firmware/target/xtensa-esp32s3-none-elf/release/sd-check build/baremetal/sd-check.bin
```

## Run in the emulator

A scenario starts in this tree's C++ firmware, which flashes the image from the card with SD Card
Firmware Update, as on the device. So build the C++ firmware first:

```sh
pio run -e x4pro
emu/compare.sh emu/baremetal-sd.txt x4pro build/baremetal/sd-check.bin
```

`emu/compare.sh` also runs the scenario on the unmodified CrossPoint build (`../crosspoint-base`).
That side fails at its first wait for the image's output, as expected: it never starts the image.
Each scenario's header shows its own commands: `feedback-check` also needs the font on the card
(`emu/books/LiberationSerif_14.cpfont=/.fonts/LiberationSerif`, made with
`lib/EpdFont/scripts/fontconvert_sdcard.py`), and the UC8179 run of `display-check` calls eink-qemu
itself with `--panel uc8179`.

## Run in the host simulator

`rust/sim` (`pw-sim`) runs the application, the renderer and the service loops on a PC, driven by
a scenario script (`rust/sim/scenarios/`, one event per line: `battery:N`, `time:HH:MM`,
`press:left|right|power|home`, `sleep`; `#` starts a comment; see
[script.rs](../sim/src/script.rs)), and
writes each frame the panel shows as an upright PNG. It checks what does not depend on hardware;
timing, interrupts and sleep stay on the device and in the emulator. From `rust/`:

```sh
cargo run -p purestwhite-sim --bin pw-sim -- \
    --font crates/bembeyaz-font/testdata/noto-serif-14.cpfont sim/scenarios/home.txt
```

It writes to `target/sim-out` (`--out DIR` for another directory), first removing the shots of an
earlier run there, and prints the files it wrote (`0001-full.png`, `0002-fast.png`, ...: the
order and the refresh) and `sleep` when the application asked to sleep. A run stops at the sleep
request; later lines are not sent. Without `--font` only the shapes are drawn.
`cargo test -p purestwhite-sim` runs every scenario, and checks each one's refreshes in its own
test in `rust/sim/src/tests.rs` (a new scenario gets one there too).

## Put an image on the device

The device has no USB data, so there is no way to recover it over USB. Put on it only images that
hand the next boot back to the C++ firmware first; every image in `src/bin/` does. The C++ firmware
writes `/.crosspoint/bootloader.txt` once (`bootloader rollback: YES|NO|UNKNOWN (slot X, state N)`):
whether the bootloader rolls back an image that never confirms itself, which decides whether a
bare-metal test in the other slot is safe without USB.

1. Copy the `.bin` to the SD card. `sd-check` and `tasks-check` read their own file from the card's
   root, so copy them as `purestwhite-baremetal-sd.bin` and `purestwhite-baremetal-tasks.bin`.
2. On the device: Settings > System > SD Card Firmware Update, and select the file. The C++
   firmware flashes it into the other OTA slot and restarts into it.
3. The image runs and resets. An image that sleeps wakes on Power or on its timer. Either way the
   C++ firmware boots next.
4. The report is in `/purestwhite-bare-metal.txt` on the card (and in the C++ firmware's log).

The device runs and their results are in
[the bare-metal hardware tests](../hardware-tests/bare-metal.md).

## Rules

- Functions stay short and single-purpose: `clippy.toml` sets `too-many-lines-threshold = 50`
  (it counts code lines only), here and in every `purestwhite-*` crate. No god functions.
- The library and every image have `#![warn(clippy::pedantic)]`. After a change here or in a
  `purestwhite-*` crate, run `cargo clippy --release -- -D warnings` in `rust/firmware`, and
  `cargo test` and `cargo clippy --all-targets` in `rust/`.
- No floating point in the bare-metal path: eink-qemu 0.9.2 crashes on floating-point operations.
- In the real firmware, DMA buffers and everything accessed while the flash is written (task
  stacks included) stay in internal memory; PSRAM goes to large data only when asked for
  explicitly (esp-alloc's `ExternalMemory`).
