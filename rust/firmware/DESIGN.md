# Design of the Embassy main

How the first real main of the bare-metal firmware is built: its layers, tasks, memory, sleep, the
update gate, and the host simulator that runs the same code on a PC. Steps 1 to 3 of the
[roadmap](ROADMAP.md). Reviewed (architecture and embedded) on 2026-10-29 before any code; the
review's findings are folded in.

## Goals of the first main

1. One image, `purestwhite`, that replaces the test images as the thing that grows. During phase 2
   it hands the next boot back to the C++ firmware first, like every test image.
2. The update gate from its first version (below), within what phase 2 allows.
3. A page loop to measure the battery (gate 1) on equal terms with the C++ firmware.
4. Every part that does not depend on timing, interrupts, sleep or the flash runs in the host
   simulator too.

## Layers and crates

```
purestwhite-app        no_std, no async, no hardware: the application's state machine (pure)
purestwhite-render     no_std: draws a View into a frame (bembeyaz-gfx, bembeyaz-font) (pure)
purestwhite-services   no_std, host-buildable: the device traits and the service loops
        rust/firmware   esp-hal board, concrete #[task] wrappers, the main
        rust/sim        std board, concrete #[task] wrappers, scenarios, PNG frames
```

- **`purestwhite-app`** (host-tested with tables of events): `App::handle(event) -> Commands`.
  Events: raw key and touch results already classified by the services (below), timers, the
  battery level, job results, "frame shown". Commands: show this `View`, run this storage job,
  start a timer, sleep. It emits `View`s (pure data describing a screen), never pixels, and refers
  to files and buffers by handle, never by content. The refresh cycle is its state too: every
  15th refresh is a half one, as in the C++ reader (`ReaderUtils::displayWithRefreshCycle`).
- **`purestwhite-render`**: draws a `View` into a frame with `bembeyaz-gfx` and `bembeyaz-font`;
  pure, and tested on the host by probing pixels (stored reference images may come later).
- **`purestwhite-services`**: the device traits, defined by what the services need, and generic
  service loops (`async fn run<S: Screen>(…)`), each handed only the ends of the channels it
  uses. Its dependencies: `embassy-sync`, `embassy-time`
  without a time driver, `embedded-hal-async`, and the `purestwhite-*` drivers. `#[task]`
  functions cannot be generic, so each binary declares thin concrete wrappers that call
  `run::<ItsType>`. Allocators come in as parameters (the simulator has no PSRAM heap).
- **Traits, at the raw level** so that the simulator drives the real classifiers:
  - `Screen`: show a frame with a refresh, the UC8179's gray passes, deep sleep (the reader will
    use gray), and wait for BUSY;
  - `Keys`: the sampled button bits; the service runs `purestwhite-buttons`;
  - `TouchPanel`: `purestwhite_gt911::Reading`s; the service runs `purestwhite-touch`;
  - `Card`: `purestwhite_fat::BlockDevice` (and `BlockWrite` only where writing is allowed);
  - `Gauge`, `Clock`: readings; `Power`: arming wake sources, holding rails, sleeping.

## Tasks and data

| Task | Owns | Receives | Sends |
| --- | --- | --- | --- |
| app | `App` | events | views, storage jobs, timers, sleep |
| draw | the renderer, the fonts (`&'static [u8]`) | views, an empty frame | a drawn frame |
| panel | `Screen`, the panel's lines and reset | drawn frames | the frame back, "shown" |
| input | `Keys`, `TouchPanel`, the Power pin | sleep requests | key and touch events |
| storage | the `Card`, the FAT volume, the SD supply | jobs | results by handle |
| power | the watchdog, the deep sleep, the supplies no task uses | sleep | rest requests |
| backstop (phase 2) | nothing | time | a sleep request, a reset |
| gauge and clock (step 4) | `Gauge`, `Clock` | timers | battery level, time |

- **Frames**: two frames of 48,000 bytes move as `&'static mut [u8; FRAME_BYTES]` between draw
  and panel through channels (draw → panel, panel → draw after the refresh), which also gives
  backpressure.
- **Views**: the app's views go through a slot that holds one (`Views`): a newer view replaces
  one not yet drawn and keeps the stronger refresh. So the app never waits for the panel, and the
  panel never spends a refresh on a view that is already out of date. Drawing also takes back a
  drawn frame the panel has not taken yet and draws the newer view into it. A fast view dropped
  this way was already counted by the refresh cycle, so the next half refresh comes a little
  early, which is the safe side. The panel answers each frame with an event to the app (`Shown`,
  or `ScreenFailed`). The app waits only for events, so these answers cannot close a cycle of
  full channels.
- **A panel failure**: the app shows the current screen again with a full refresh, at most twice
  in a row; when a newer view is already on its way, nothing is shown again, and that view gets
  the full refresh unless the panel shows a later one first. The `Screen` implementation brings
  the panel back (the drivers' `begin`) first. The drivers also flash the first refresh after
  `begin`; the app's own full refresh is kept on purpose, so that its refresh cycle and the
  simulator see it. On a dead panel each attempt costs up to the panel loop's limit (20 s in the
  `purestwhite` image); whether the app stops trying after the retries until a key is pressed
  is still open.
- **Executor**: the loops' futures are not `Send` (the channel ends are not), so they run on the
  thread-mode executor; the second core stays parked.
- **Storage jobs**: the reader's parsers (`bembeyaz_io::Source`, zip, the chapter parser, the
  section cache) read synchronously. The storage task therefore runs whole jobs (open a book,
  parse a chapter, read a page) over a synchronous `Source`: the device-proven `BlockingBus`,
  yielding to the executor between chunks. It holds a `WakeLock` for each job: the SD controller
  does not take one in esp-hal 1.2.2 and its interrupt does not wake the chip, so an awaited
  transfer could stall in automatic light sleep.
- **The shared I2C bus** (touch, clock, gauge): an `embassy-embedded-hal` shared-bus `I2cDevice`
  per user, locked per transaction, so the gauge's internal waits do not hold the bus away from the
  touch; the bus is recovered (nine clock pulses) when SDA is stuck. The touch supply (GPIO2) is
  owned with the bus and stays on whenever the bus is used.
- Channels are bounded; a full channel makes the sender wait.

## Memory

- Internal RAM: every task's stack, DMA buffers (the card's blocks), and everything touched while
  the flash is written (the cache is off then: no PSRAM access, no DMA from PSRAM). The internal
  heap holds only those. Its size is planned for Wi-Fi at gate 2 (73 KB will not do).
- PSRAM: a separate heap (`esp_alloc::EspHeap::empty()` with the PSRAM added as its region, used
  through allocator-api2), never the global allocator, so a full internal heap fails instead of
  spilling. It holds the fonts, the two frames, decoded pages and images. A frame is allocated
  there as a slice, leaked, then turned into `&'static mut [u8; FRAME_BYTES]`: never as a boxed
  array, which is built on the stack first (48 KB).
- The panel's SPI reads a frame with the CPU (as today) or through a small internal bounce
  buffer; never GDMA straight from PSRAM until alignment and cache write-back are designed.

## Sleep

- Sleep is a two-phase handshake. The app asks for the sleep screen, and sets the sleep signal
  only once the panel has answered for it (shown, or failed: a broken panel does not keep the
  device awake). The signal has one listener, the power task, which asks each service in turn
  to quiesce (the panel's deep sleep, the volume flushed, input stopped, no I2C transfer in
  flight); each answers, handing back the pin it holds for the rails. Then the power task arms
  Power, holds the rails and sleeps.
- Since sleep waits for the panel's answer, every `Screen::show` must end: the panel loop bounds
  each call to the panel itself (a timeout through `embassy-time` turns into `ScreenFailed`, and
  the next call begins the panel again), not each `Screen` implementation.
- Light sleep (esp-rtos's idle hook) needs the input task to stop polling when idle: it polls
  every 15 ms only while a button or a touch is active (the `Buttons` and `Touch` deadlines) and
  otherwise waits on the pins. Wake sources go through the low-power path: Power (GPIO3), the
  touch INT (GPIO10), BUSY (GPIO6). esp-hal's low-power wake is level-based, so a short GT911 INT
  pulse may be missed: the INT mode is checked on the device, with a slow fallback poll.
  `ESP_RTOS_CONFIG_LIGHT_SLEEP_MIN_US` is raised from 1000 µs. Not yet: the `purestwhite`
  image runs without light sleep, and awaits BUSY and the buttons as levels (a level wait has no
  race between the read and the wait; whether it wakes from light sleep is checked then).
- The watchdog: armed first, fed by the power task on a period that proves the executor runs;
  esp-hal pauses it in sleep, so it counts awake time. The real main uses core resets only
  (`ResetCore`), never a reset of the RTC domain, which would release the power latch on battery.
  During phase 2 the image's deep sleep is the exception (`boot::watch_sleep`): the watchdog
  counts through it, and a system reset is the last resort for a chip stuck asleep; it goes away
  in phase 3, with the bounded sleep.
- The second core is parked at start in every image: esp-rtos counts a core outside the
  scheduler as idle, and flash writes need it parked.

## The update gate

The rules are the roadmap's; phase 2 restricts them further, because during phase 2 the slot that
is not running holds the C++ firmware: the known-good one, with the proven SD updater.

### Boot order

1. Arm the RTC watchdog; hold the power latch; park the second core.
2. Phase 2: hand the next boot back to the C++ firmware (`purestwhite-ota`), before anything else
   that could fail, the recovery key included.
3. Read the recovery key (Right held at power-on).
4. Check the boot counter (below).
5. Start the tasks. "Healthy" needs no input from the owner: the first screen shown and the tasks
   running for 30 s, or an orderly sleep reached. Healthy clears the counter. The panel's answers
   reach only the app, so the app turns the first `Shown` into a command (with the 30 s timer
   once timers exist); no second listener is added to the panel's answers.

### The boot counter

- At a reserved RTC address documented in both firmwares, clear of the C++ firmware's
  `RTC_NOINIT`/`RTC_DATA` areas and wake stub, the report (0x5000_1F00) and the bootloader's record
  (0x600F_FFFC); kept with a magic and an inverted copy, and cleared on a power-on reset so that
  leftover RAM is not read as a count.
- It counts only abnormal resets, by the reset reason (watchdog, panic, brownout); a wake from
  deep sleep is not one.
- At three in a row: the minimal recovery mode first (below). A switch of slots goes only to a slot
  whose image validates, or the bootloader's own fallback could ping-pong between them.

### The SD updater (recovery mode)

Minimal: board, card, FAT, flash, otadata and a line of text; no fonts, books or network.

- **Phase 2: it may install only the C++ firmware's own release image**, recognised by the
  SHA-256 that ESP-IDF appends to it, which the Rust image carries for each known release, and
  refuses any other file. The app descriptor cannot tell it: the C++ image's names the Arduino
  framework that built it (`arduino-lib-builder`, `ee57070` for 0.0.3), not the firmware, whose
  own name is the `purestWhite-firmware:` string `OtherFirmware.cpp` finds by scanning. Writing the C++ firmware over itself with a proven-identical
  image is the only write it can make to the known-good slot.
- Before that is ever done on the device, the erase, write and verify path is proven on scratch
  flash: the `spiffs` partition at 0xC90000 (`partitions.csv`), after a device test has read it
  and confirmed the C++ firmware does not use it (its sources do not open it).
- Checks before writing: the appended SHA-256, the chip id (9, ESP32-S3), the app descriptor's
  magic, and that the image measured from its segments fits the 0x640000 slot; the battery not
  low. `purestwhite_ota::image::ImageCheck` checks the image as it streams in (the digest itself
  is the caller's, with the chip's SHA unit); it passes every release in the archive. The watchdog is fed between flash chunks (erasing and writing 6.4 MB can take longer than
  its 60 s; esp-storage runs each operation with interrupts off).
- The write goes to the slot that is not running, is read back and checked, and only then is
  otadata changed (`purestwhite-ota`); a failure at any step leaves otadata as it was.
- A path without the screen or touch: holding the recovery key long installs the one file with a
  fixed name (the C++ release image) without a menu, so recovery does not depend on the panel or
  the touch working.
- Phase 3 (promotion) lifts the phase-2 limit only after the Rust firmware's own update path has
  installed the identical known-good image once.

## Battery measurement (gate 1)

- Both firmwares show the same pre-rendered pages: full-frame images on the card, shown by the
  Rust page loop and by the C++ firmware's image viewer (BMP), every 30 s, for one hour each. The
  app refreshes fast with a half refresh every 15th; the C++ image viewer refreshes differently
  (mostly half refreshes, `BmpViewerActivity.cpp`), so step 4 makes both use the same refreshes
  before measuring. Neither parses a book, so the comparison is the
  platform's power alone.
- The gauge's charge and voltage every 5 minutes (12 samples) go into the RTC report (they fit in
  its 248 bytes), so nothing is written to the card before gate 4; the books card is never
  written.

## The host simulator (`rust/sim`)

Step 1: the script's lines (`battery`, `time`, `press`, `sleep`) go straight to the app as events;
`Screen` is the only device trait so far. These words stand for events the services have already
classified; when the classifiers come, words for raw samples are added beside them. The loops run in
one `block_on` (`embassy-futures`). After the first screen and after each event, the run waits until
nothing is left to handle, draw or show (`Links::is_idle`), and fails if that does not happen within
10,000 turns of the loops; it ends after the last event, or once the app has asked to sleep and the
panel loop has put the panel to rest (later lines are not sent). Embassy's `std` executor comes when
the app needs timers; the rest of this section comes with the step that needs it.

Embassy's `std` executor runs the app, draw and service loops over a simulated board:

- `Screen`: frames written to PNG files (optionally a window), each refresh noted with its kind
  and modeled time;
- `Keys` and `TouchPanel`: raw samples from a scenario script (`press:left`, `tap:240,400`,
  `swipe:left`, `wait:500`, the eink-qemu words where they overlap), classified by the real
  `purestwhite-buttons` and `purestwhite-touch`;
- `Card`: a FAT image file through `purestwhite-fat`, made with `mkfs.fat` and `mtools` as the FAT
  tests do; the flash and otadata through `purestwhite-ota` over a fake flash;
- `Gauge`, `Clock`: scripted values; `Power`: sleep is noted, not done.

It checks the app, the renderer, the classifiers, the channels and their backpressure, FAT on an
image, and the otadata decisions. It cannot check timing and refresh durations (modeled only),
interrupts, sleep and wake, whether memory lands in internal RAM or PSRAM, DMA, bus contention,
or the real flash: those stay on the device and in eink-qemu (boot, flashing from the card, the
hand-back).

## Steps

1. `purestwhite-app`, `purestwhite-render` and `purestwhite-services` with one screen, the
   traits and their tests; `rust/sim` showing the screen as a PNG. Done: the home and sleep
   screens, the refresh cycle, the `Screen` trait, the app, draw and panel loops with the
   panel's answers and the first half of the sleep handshake; `pw-sim` with
   `rust/sim/scenarios/`. The other traits come with the steps that use them.
2. The firmware board and the `purestwhite` image (hands back first) showing the screen on the
   device; the input loop that stops polling when idle; the sleep handshake. Written, for the
   device test: the image with the app, draw, panel and key tasks, frames in a PSRAM heap of
   their own, the panel's calls bounded by a time limit in the panel loop, the panel and the keys
   resting before the deep sleep, and a phase-2 backstop task (it asks for the sleep after 10
   minutes; the sleep itself is bounded by a timer and the watchdog). The key loop is in the
   firmware lib (`keys`), on the pins, not a service over a `Keys` trait: the simulator does not
   run it, and its timing is host-tested in `purestwhite-buttons` (`sample_in`). The `Keys` trait
   comes when the simulator needs raw samples. Touch, the font, the battery and the clock come
   later; health (step 3) and the battery log (step 4) get tasks of their own rather than growing
   the power task.
3. The update gate: the boot counter, the recovery mode with the phase-2 limits, the scratch-flash
   proof on the device.
4. The battery page loop and its RTC log; the two-firmware measurement. Before it, who owns the
   report: now the power task does, and the phase-2 backstop writes it once, just before a
   reset; a battery log in its own task needs a shared report in `boot` or an RTC area of its own.

Each step: host tests, sim scenarios, `cargo clippy` with the 50-line limit, the emulator's
hand-back scenarios, and the reviews the project's rules ask for (memory, build and flash
changes).
