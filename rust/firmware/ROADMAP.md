# Roadmap of the bare-metal firmware

Where the bare-metal firmware goes next, and what has to be true before the C++ firmware is left
behind. Agreed with the owner on 2026-10-29.

## Where it stands

Phase 2 of the [platform plan](../porting/6-platform.md): a separate firmware in the second OTA
slot. On the device so far: the panel (UC8279, UC8179 with its gray passes), touch, the buttons,
deep sleep, the PSRAM (at 40 MHz), reading the SD card, Embassy tasks, text from the card's font.
Not started: Wi-Fi, HTTPS, BLE, the web server (file transfer, WebDAV), update over Wi-Fi, the
network side of KOSync and OPDS, writing to the card on the device, and the whole user interface
(menus, settings, themes, languages, the reader): about 41,000 lines of C++ in `src/`.

## The decision gate before phase 3

Phase 3 leaves the C++ firmware behind; it is the one step that cannot be taken back. Before it,
four things are measured on the device:

1. **Battery.** The same reading session (for example one hour, a page every 30 s) on the C++
   firmware and on the bare-metal one, measured with our fuel gauge driver: the bare-metal one
   must not use more.
2. **Wi-Fi and HTTPS.** Download a file over HTTPS on the bare-metal firmware and write it to the
   card (wolfSSL through FFI, as planned).
3. **BLE.** A page-turn remote works on the bare-metal firmware.
4. **Writing to the card on the device** (W3, on a spare test card, never the books card).

If all four pass, phase 3 goes ahead. If one does not, the fallback is Rust on ESP-IDF
(`esp-idf-svc`): Wi-Fi, BLE and power management stay ESP-IDF's, and the application and our
drivers are Rust. The drivers are written against the embedded-hal traits, so they move to either
path unchanged.

## Order of work

1. The Embassy main: input, panel and card as tasks over channels, a dedicated PSRAM heap (DMA
   buffers and anything used while the flash is written stay in internal RAM), async panel waits,
   the shared I2C bus behind a mutex.
2. The update gate (below), in the main from its first version.
3. The battery measurement (gate 1): the cheapest check and the one that decides most.
4. Wi-Fi and HTTPS (gate 2), then BLE (gate 3), then writing to the card (gate 4).
5. Only then the user interface and the rest of the features.

## Battery: how the bare-metal firmware could lose, and what keeps it from losing

The C++ firmware does not use automatic light sleep (`CONFIG_PM_ENABLE` is off). When idle for 3 s
it lowers the CPU clock to 80 MHz on the X4 Pro (10 MHz without PSRAM) and raises it again on the
next key press (`HalPowerManager`). The bare-metal firmware could use more power by:

- **A fixed CPU clock.** esp-hal 1.2.2 sets the CPU clock only at start (`esp_hal::Config`); it has
  no call to change it while running. At 240 MHz an idle core still keeps the PLL running.
- **Idling without sleeping.** esp-rtos 0.4.0 has an automatic light-sleep idle hook
  (`ESP_RTOS_CONFIG_LIGHT_SLEEP_MIN_US`: light sleep when the next timer is far enough away,
  otherwise a plain wait for interrupt). Every wake source (Power, the touch INT line, the panel's
  BUSY) has to be set for light sleep, or a press is missed; drivers that must not be interrupted
  take a wake lock. The idle hook freezes the other core while one sleeps (a known esp-rtos
  limitation); we run on one core.
- **Rails and pins left on in sleep.** The SD lines must be held low in sleep as the SDK does (the
  test images do not do it yet); unused pins must not float.
- **Longer active time.** The PSRAM at 40 MHz instead of 80 MHz makes PSRAM-heavy work (fonts,
  images) take longer.
- **Wi-Fi.** ESP-IDF's Wi-Fi power save is well tuned; esp-radio's is younger (esp-hal tracks
  modem power-down in light sleep as open work).

Measured with the same session on both firmwares, each of these shows up; automatic light sleep
is also where the bare-metal firmware can do better than the C++ one.

## PSRAM at 80 MHz

ESP-IDF tunes the PSRAM's sampling timing at every boot above 40 MHz; in esp-hal 1.2.2 that step,
`mspi_timing_psram_tuning`, is an empty stub (`src/psram/esp32s3.rs`: "currently we only support
!SPI_TIMING_PSRAM_NEEDS_TUNING"). Without it, 80 MHz may work on one chip at room temperature and
fail on another or when warm: memory errors, not crashes. Others ran 80 MHz by setting
`core_clock: SpiTimingConfigCoreClock160m` (esp-hal issue #3956), still without tuning.

Steps:

1. An issue in esp-hal asking for the tuning (none is open for it; the owner decides before
   anything is posted).
2. Port ESP-IDF's tuning (`components/esp_hw_support/mspi_timing_tuning.c`, Apache-2.0) for the
   S3's octal PSRAM into esp-hal, as a pull request: write a pattern through SPI1, try each
   sampling setting, take the middle of the settings that read back right.
3. Until then, 80 MHz only as an experiment: `psram-check` at 80 MHz with `core_clock` 160 MHz,
   run cold and warm; the real firmware stays at 40 MHz.

## Emulator

eink-qemu 0.9.2 cannot run esp-rtos, and crashes on floating point and on GPIO and SDMMC
interrupts. The Embassy firmware therefore loses the emulator check the C++ firmware has.

1. **A host simulator of our own.** The firmware's layers do not depend on the executor, and the
   drivers use the embedded-hal traits, so the firmware's logic runs on the PC: Embassy on `std`, a
   simulated board in place of `board.rs` (the panel draws into a PNG or a window, buttons and
   touch come from a script, the SD card is an image file read by `purestwhite-fat`, the clock and
   the gauge are fakes). Scenarios and screenshots as with `emu/compare.sh`, fast and repeatable.
   It cannot check timing, interrupts, sleep or the flash.
2. **eink-qemu for the rest.** Booting, flashing from the card and handing the next boot back stay
   eink-qemu scenarios. eink-qemu is open source and active, but its issues are turned off: fixes
   for esp-rtos (system timer alarms, interrupts), floating point and the GPIO and SDMMC
   interrupts would go as pull requests, or into a fork of our own.

## The update gate stays open

The device has no USB data: if no firmware on it can install another from the SD card, it cannot
be recovered. The bootloader's rollback is a second net, not the plan: the rules below keep the
gate open without it, and without changing the bootloader or the partition table (two app slots,
`app0` and `app1`).

1. **One slot always holds a known-good firmware** whose SD update works. An update is only ever
   written into the other slot, never into the slot that runs.
2. **A new firmware hands back first.** Its first act, before anything that could fail, is to
   select the known-good slot for the next boot (`purestwhite-ota`, as every test image does now).
   A crash, hang, watchdog reset or power loss then lands in the known-good firmware.
3. **Promotion only on proof.** A new firmware selects itself for the next boot only after its
   self-test passed, the owner confirmed it on the screen, and it installed a firmware from the SD
   card once (so its own update path is proven). Then the other slot becomes the slot for the next
   update.
4. **A recovery key read first.** Holding a key at power-on goes straight to a minimal SD updater
   (board, card, FAT, flash, otadata, a line of text), before fonts, books or the network, so that
   a fault in the rest of the firmware cannot block it. The C++ firmware has this (Right + Power);
   the Rust firmware gets it in its first main.
5. **A crash loop falls back by itself.** A boot counter in RTC memory: a firmware that resets
   several times in a row without reaching a healthy state selects the other slot. The watchdog is
   armed first, so a hang becomes a reset.
6. **Safe writes.** The image is written and verified (its checksum) in the other slot before
   otadata changes; the new otadata entry goes into the other otadata sector, so a power loss
   leaves the old entry in force (`purestwhite-ota`).
7. **Tested every time.** The emulator's hand-back scenarios (`emu/baremetal-handback.txt`, from
   either slot) and a recovery-key scenario run for every release.
