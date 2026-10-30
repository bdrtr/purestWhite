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
| panel | `Screen`, the panel's lines and reset | drawn frames | the frame back, "shown", the shown flag |
| input | `Keys`, `TouchPanel`, the Power pin | sleep requests | key and touch events |
| storage | the `Card`, the FAT volume, the SD supply | jobs | results by handle |
| power | the watchdog, the deep sleep, the supplies no task uses, the healthy mark | sleep, the shown flag | rest requests |
| backstop (phase 2) | nothing | time | a sleep request, a reset |
| timers | a `Clock` (embassy-time's on the device) | the app's armings | firings |
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
- **Timers**: the app arms a timer with a duration and an id (`Command::Arm`), never reading a
  clock; the arming goes into a slot per timer (`Timers`, like `Views`: a newer arming replaces
  the older, and the app never waits). The timer loop (`run_timers`) gives each arming its time
  when it sees it and sends `Event::Fired` with its id; the app acts only on its latest arming's
  id, so a firing already on its way when a key armed the timer again is out of date. The loop
  runs on a `Clock`: embassy-time's on the device (`EmbassyClock`), the script's in the simulator.
  The first timer is the idle sleep: ten minutes without a key press put the device to sleep, as
  the C++ firmware's default auto-sleep (`IDLE_MS`). The `purestwhite` image does not run the
  timer loop yet (its armings stay in the slot): during phase 2 the backstop's sleep stands in,
  and the loop comes with the next device test of the image.
- Channels are bounded; a full channel makes the sender wait.

## Memory

- Internal RAM: every task's stack, DMA buffers (the card's blocks), and everything touched while
  the flash is written (the flash cannot be read through the cache then: no code or data from flash,
  no PSRAM access, no DMA from PSRAM). The internal heap holds only those. Its size is planned for
  Wi-Fi at gate 2 (73 KB will not do).
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
is not running holds the C++ firmware: the known-good one, with the proven SD updater. One
exception, which the owner accepted for the SD updater's install (2026-10-29): while the updater
writes the C++ firmware's release over itself (about 40 s), that slot is not known-good; the way
back is then the updater in the other slot and the release on the card.

The device keeps the partition table it left the factory with, not `partitions.csv` (the C++
firmware's updates never write the table). On the owner's X4 Pro (table-check, 2026-10-29): nvs at
0x9000, otadata at 0xE000, `app0` at 0x10000 and `app1` at 0x7F0000 (0x7E0000 bytes each), a
`spiffs` of 80 KB at 0xFD0000 holding data, a `coredump` of 112 KB at 0xFE4000; no free space.
Slot sizes therefore come from the device's table, never from `partitions.csv`.

### Boot order

1. Arm the RTC watchdog; hold the power latch; park the second core; make SPI1 select the flash
   alone (its second chip select is the PSRAM's), whatever ran before.
2. Phase 2: hand the next boot back to the C++ firmware (`purestwhite-ota`), before anything else
   that could fail, the recovery key included.
3. Count the run (the boot counter, below).
4. Read the recovery key: Right held as the image starts (five samples 10 ms apart). During
   phase 2 the image starts when the C++ firmware restarts into it, and at power-on the key is the
   C++ firmware's (Right + Power, its own recovery mode).
5. Start the tasks. "Healthy" needs no input from the owner: the first screen shown, and then
   either the tasks running for 30 s or an orderly sleep reached. A run that never shows a frame
   is never healthy. Healthy clears the counter. In the `purestwhite` image the panel task wraps
   the screen and sets a flag once a frame is shown (it sees the screen's own result, so neither
   the app nor the services' channels change), and the power task marks the run healthy once the
   flag is set and 30 s have passed (`healthy: 30 s`), or, with the flag set, once the panel and
   the keys rest before the sleep (`healthy: sleep`). During phase 2 the count is only noted
   (`runs: N unfinished`, `, recovery due`): the hand-back has already made the next boot the C++
   firmware's, and with the C++ firmware intact the recovery mode could only write the same
   release again. Phase 3, without the hand-back, is to enter the recovery mode at three, and to
   count the run right after arming the watchdog, so that a crash in the start counts too.

### The boot counter

- Its record (`purestwhite-bootcount`, kept by `boot::begin_run` and `boot::finish_run`) is in RTC
  slow memory at 0x5000_1EF0, the 16 bytes below the report (0x5000_1F00). That is clear of the C++
  firmware's RTC data, which ends at 0x5000_17BC in its current build, and of the bootloader's
  record (0x600F_FFFC). The C++ firmware's report saver names the 16 bytes in a comment; its
  overlap check covers only the report, and is to cover them too at its next release (changing it
  now would change the C++ build, which must stay the release the SD updater knows).
- The record holds a magic and an inverted copy of its value, so that memory never written is not
  read as a count. RTC slow memory keeps it through resets, watchdog core resets and deep sleeps,
  and loses it in a power cut. A power-on reset seen by `begin_run` starts from zero; during phase
  2 a power-on starts the C++ firmware, not an image, so there it is the power cut that clears it.
- It counts runs in a row that ended before they were healthy, not reset reasons: each run marks
  itself running at its start and clears the mark once it is healthy, and the next run counts a
  mark left behind, whatever ended that run (a panic, a reset, a hang the watchdog ends, a
  brownout that keeps RTC memory). During phase 2 a crashed run is followed by the C++ firmware,
  so the next bare-metal run sees the C++ firmware's restart as its reset reason, not the crash.
  An orderly sleep after a frame was shown is healthy, so its wake is not counted.
- At three in a row: the minimal recovery mode first (below). A switch of slots goes only to a slot
  whose image validates, or the bootloader's own fallback could ping-pong between them. The one
  exception is `boot-check`, a test image that selects itself on purpose once its own digest no
  longer holds, and only while the C++ firmware's slot holds a known release.

### The SD updater (recovery mode)

Minimal: board, card, FAT, flash, otadata and a line of text; no fonts, books or network.

- **Phase 2: it may install only the C++ firmware's own release image**, recognised by the SHA-256
  that ESP-IDF appends to it, which the Rust image carries for each known release, and refuses any
  other file. The app descriptor cannot tell it: the C++ image's names the Arduino framework that
  built it (`arduino-lib-builder`, `ee57070` for 0.0.3), not the firmware, whose own name is the
  `purestWhite-firmware:` string `OtherFirmware.cpp` finds by scanning. It writes the known-good
  slot only with a known release, and only over the same release (proven identical: the write
  path on the real slot, changing nothing) or over a slot whose image no longer passes and which
  the bootloader has just refused (a write cut short: otadata selects that slot, yet the
  bootloader started the updater); a slot holding another image that passes, or one that reads as
  broken while the bootloader booted the updater by selection, is left alone
  (`purestwhite_ota::plan`). An image passes when its segments, chip, app descriptor, checksum
  and appended SHA-256 hold: what a bootloader that checks images checks.
- The erase, write and verify path was proven on scratch flash first: the last megabyte of the
  slot the image runs from, far past its own image, through that partition's own flash region
  (which cannot write outside it), where a mistake harms only the image's own slot (write-check,
  2026-10-29). The device's `spiffs` holds data and its table has no free space (scratch-check
  and table-check, 2026-10-29).
- Checks before writing: the appended SHA-256, the chip id (9, ESP32-S3), the app descriptor's
  magic, and that the image measured from its segments fits the slot; the battery not low, which is
  not done yet: no image reads the fuel gauge, and install-check's device test asks for USB power
  instead. `purestwhite_ota::image::ImageCheck` checks the image as it streams in (the digest itself
  is the caller's, with the chip's SHA unit); it passes every release in the archive. The watchdog
  is fed between flash chunks: write-check measured about 27 ms per 4 KB sector on the device
  (erase, write, read back and hash; 2026-10-29); install-check wrote the 5.4 MB release in 54 s,
  about 40 ms a sector with the card read (2026-10-29), and its first pass reads the card in about 4
  s. Erasing 64 KB blocks would be several times faster, but each block erase keeps interrupts
  masked for its whole length (about 0.15 s, up to 2 s on common 128 Mbit parts). Each ROM flash
  call runs with interrupts masked, through esp-storage's `critical-section` feature, which the
  firmware turns on itself (esp-bootloader-esp-idf leaves it off): a handler run from flash while
  the flash is busy would fetch garbage, so interrupts wait up to a sector erase (about 0.4 s).
  `sha::HwSha` finishes a digest dropped unfinished, so that the next starts on an idle unit.
- The write goes to the slot that is not running, is read back and checked, and only then is
  otadata changed (`purestwhite-ota`); a failure at any step leaves otadata as it was. During
  phase 2 otadata already selects the C++ firmware's slot (the hand-back comes first), so a write
  cut short (power lost, a reset) leaves the selected slot without an image that passes. A
  bootloader that checks the image it boots then finds none there and falls back to the updater
  in the other slot, which writes the release again. The C++ build's bootloader checks on every
  boot; a deep-sleep wake normally skips the check and boots the image booted last, but the
  hand-back drops that record, so the wake reads otadata and checks the image too. Whether the
  device's own bootloader checks images was tested by `boot-check` (2026-10-29): it is not the C++
  build's (15600 bytes, ESP-IDF v6.0.1, built Aug 27 2026), it rolls back, and after a software
  reset it refused an image whose digest no longer held and started the C++ firmware. A
  bootloader set to skip the check only at power-on would pass that too, so the repair after a
  power loss stays unproven. When the slot is left without an image that
  passes and the updater cannot write it (the card or its release missing), it sleeps until Power
  or a timer rather than reset straight back into itself (`updater::end`).
- Two passes over the file, because during phase 2 the slot written is the known-good one: the
  first (`purestwhite_ota::digest::Identify`) checks the image and works out its digest without
  writing anything, and only a known release's digest goes on; the second
  (`purestwhite_ota::slot::SlotWriter`) erases, writes and reads back each 4 KB sector through the
  partition's own flash region (`esp-bootloader-esp-idf`'s `FlashRegion`, which cannot write
  outside it), and its digest must be the first pass's. Host-tested on a fake NOR flash: a refused
  image erases nothing, a bit that does not take is found when read back.
- A path without the screen or touch: holding the recovery key long installs the one file with a
  fixed name (the C++ release image) without a menu, so recovery does not depend on the panel or
  the touch working.
- The updater resets into the C++ firmware only when its slot holds an image that passes and
  either the bootloader did not refuse that slot or the updater wrote the image itself
  (`purestwhite_ota::plan::resets`). After a fallback, a slot that reads as passing is left alone
  (`refused: Disagree`): the bootloader and the updater's check disagree, and writing the same bytes
  would not change that; the updater sleeps instead, so it never resets straight back into itself.
- Done so far (`updater`, the `purestwhite` image's `recovery`): before any task starts, the image
  runs the updater instead when the bootloader fell back to it (it refused the C++ firmware's slot)
  or when Right is held as it starts; it notes `recovery: fell back` or `recovery: Right held`, then
  the updater's lines, and resets into the C++ firmware or sleeps (bounded). The run counts as
  healthy when it ends in that reset. Only the Right entry can be tested on the device without
  breaking the C++ firmware's slot; the fell-back entry shares the updater with install-check, whose
  repair passes in the emulator, but the `purestwhite` image itself does not run in eink-qemu 0.9.2,
  so it is build-only so far. No line of text on the panel yet.
- The updater's steps are in `purestwhite-updater`, which touches no hardware: the slot is an
  `embedded-storage` NOR flash, the card a `purestwhite-fat` volume opened through its `Card` trait,
  the SHA unit a `Hashes`, the report a `Notes`; the firmware's `updater` lends it the device's
  parts. Its host tests are the recovery mode's scenarios, on a card formatted by mkfs.fat and the
  slot in memory: the same release written over itself, a slot the bootloader refused repaired, a
  write cut short then repaired on the next run, each refusal (an unknown image, another release,
  a broken slot the bootloader did not refuse, a slot that passes after a fall back, an image too
  big for the slot, an unreadable slot) writing nothing, and no card or no release changing
  nothing; each checks the lines noted and whether the firmware then resets or sleeps. What they
  cannot check is the key, the image's start and the real flash and card: those stay on the device
  and in the emulator's install-check scenarios.
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

Now: the script's event lines (`battery`, `time`, `press`, `sleep`) go straight to the app as
events; `Screen` is the only device trait so far. These words stand for events the services have
already classified; when the classifiers come, words for raw samples are added beside them. The
loops, the timer loop included, run in one `block_on` (`embassy-futures`). After the first screen
and after each step, the run waits until nothing is left to handle, draw or show and every arming
has been seen by the timer loop (`Links::is_idle`), and fails if that does not happen within
10,000 turns of the loops; it ends after the last step, or once the app has asked to sleep and the
panel loop has put the panel to rest (later lines are not taken). The clock is the simulator's
own and moves only on `wait:S` (seconds, as eink-qemu's `wait`): to each timer due meanwhile in
turn, so that each fires at its time, then to the wait's end. A scenario of hours runs at once and
the same every time; each shot keeps the time it was shown. Embassy's `std` executor is not used:
its clock is the PC's. The rest of this section comes with the steps that need it.

Later, the app, draw and service loops run over a simulated board:

- `Screen`: frames written to PNG files (optionally a window), each refresh noted with its kind
  and modeled time;
- `Keys` and `TouchPanel`: raw samples from a scenario script (`press:left`, `tap:240,400`,
  `swipe:left`, the eink-qemu words where they overlap), classified by the real
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
   device; the input loop that stops polling when idle; the sleep handshake. Written, for the device
   test: the image with the app, draw, panel and key tasks, frames in a PSRAM heap of their own, the
   panel's calls bounded by a time limit in the panel loop, the panel and the keys resting before
   the deep sleep, and a phase-2 backstop task (it asks for the sleep after 10 minutes; the sleep
   itself is bounded by a timer and the watchdog). The key loop is in the firmware lib (`keys`), on
   the pins, not a service over a `Keys` trait: the simulator does not run it, and its timing is
   host-tested in `purestwhite-buttons` (`sample_in`). The `Keys` trait comes when the simulator
   needs raw samples. Touch, the font, the battery and the clock come later. Health (step 3) is a
   check in the power task (its watchdog loop, and its sleep handshake once the loops rest); the
   battery log (step 4) gets a task of its own rather than growing the power task.
3. The update gate: the boot counter, the recovery mode with the phase-2 limits, the scratch-flash
   proof on the device. Done so far: the image check, the two passes and the slot writer
   (`purestwhite-ota`), and `write-check`, which proved the write path on the device in the last
   megabyte of its own slot (2026-10-29: 27 sectors in 726 ms, the same digest read back), and
   `update-check`, the SD updater's reading side as a dry run, which passed on the device (the
   card's release and the C++ slot identified as the same image, "next a8764dbb"; the card read in
   3979 ms, the slot in 1994 ms; nothing written), `boot-check`, which showed the device's own
   bootloader refusing an image that does not pass after a software reset, and `install-check`, the
   install within the phase-2 plan (cut-short writes passed in the emulator), which wrote the C++
   firmware's release over itself on the device and read it back the same (2026-10-29: 1341 sectors
   in 54 s); and the boot counter (`purestwhite-bootcount`, `count-check`: three unfinished runs in
   a row note `recovery due` in the emulator, and the count survives the C++ firmware running
   in between, `emu/baremetal-count-cpp.txt`); and the recovery mode's first slice (`updater`, the
   `purestwhite` image's `recovery`; its device test waits), whose steps and scenarios run on the
   host (`purestwhite-updater`). Left: its line of text on the panel, and the check at power-on.
4. The battery page loop and its RTC log; the two-firmware measurement. Before it, who owns the
   report: now the power task does, and the phase-2 backstop writes it once, just before a
   reset; a battery log in its own task needs a shared report in `boot` or an RTC area of its own.

Each step: host tests, sim scenarios, `cargo clippy` with the 50-line limit, the emulator's
hand-back scenarios, and the reviews the project's rules ask for (memory, build and flash
changes).
