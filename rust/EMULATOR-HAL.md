# Emulator and HAL inventory

The safety net of [stage 6](porting/6-platform.md) (the HAL and the SDK drivers into Rust) will be
`emu/compare.sh`: it runs the same scenario on the old and the new build in eink-qemu and compares
the screens. For each part of the HAL, this document records what the emulator really models, what
is missing, and what must always be tried on the device (2026-10-25).

eink-qemu is a third-party project: `serialx/eink-qemu` (GPLv2, based on QEMU 11.1.2); we only use
its release archives (`../tools/eink-qemu-0.7.0`). Its own rule is "model the hardware from the
datasheets and the board, not from one firmware's driver"; so a model we add must imitate the X4
Pro's hardware, not our driver.

## Part by part

| HAL part | X4 Pro hardware | In eink-qemu | Our scenarios | Missing for the HAL port |
| --- | --- | --- | --- | --- |
| `HalDisplay` | UC8279 (0x68) panel, GPSPI2 + GDMA, BUSY | Yes: panel commands, partial window (0x90), BUSY time, refresh count (`refreshes`, `refresh-ns`); ghosting optional | All (every `shot`) | Nothing. The waveform and how ghosting looks: on the device |
| `HalGPIO`: buttons | Left GPIO0 (the SDK's Up), Right GPIO7 (the SDK's Down), Power GPIO3 | Yes (`press:up/down/power`, long press) | `reading`, `sleep`, `open-book`... | Nothing |
| `HalGPIO`: touch | GT911 (I2C 0x5D, INT GPIO10, power GPIO2), capacitive Home | Yes (`touch`, `swipe`, `press:home`); no multi-touch | `ble-settings`, `settings`, `library` | Nothing (multi-touch is not used) |
| `HalStorage` | SDMMC 1-bit, GPIO5 power | Yes: FAT32 card, read and write; speed measured on the X3 | Every scenario that copies books and the card | Card errors (a read error, a removed card, a broken FAT) cannot be produced |
| `HalPowerManager`: sleep | Deep sleep, ext0/ext1 wake-up by a button, RTC memory | Yes: deep and light sleep, timer and pin wake-up, RTC memory is kept | `sleep` | Nothing. Wake-up latency and current: on the device |
| `HalPowerManager`: battery | CW2017 fuel gauge (I2C 0x63) | Yes: percentage and voltage, changed at run time with `hmp:qom-set /machine/cw2017 soc N` | `battery` (80% → 42% → 100%) | Nothing |
| `HalPowerManager`: charging | STAT GPIO21 (high = charging) | **No**: the pin always reads 1 ("charging") | None | The charge state does not change; charging and not charging cannot be told apart in a test |
| `HalClock` | BM8563 RTC (I2C 0x51) | Yes: the host's clock or `--rtc` (default 2026-01-01 09:00) | `clock` (the clock in the header and in the status bar); all with a fixed `--rtc` | Nothing |
| `HalFrontlight` | LEDC PWM, cool GPIO8 (channel 4), warm GPIO9 (channel 5) | **No** ("open" in the upstream project) | None | The duty cycle of brightness and color temperature cannot be read |
| `HalHaptics` | No motor (only on the Metalio) | Not needed | — | The HAL's empty path, with a host test |
| `HalTiltSensor` | No IMU (`ImuType::None`) | Not needed | — | The HAL's empty path, with a host test |
| `HalMemory`, `HalSystem` | PSRAM 8 MB, panic dump | PSRAM present (modelled 60–90 % fast); heap walk (`heap`) | `clipping-large` | Nothing. Absolute timing: on the device |
| Wi-Fi (SDK) | ESP32-S3 Wi-Fi | A fake open access point (`PICSimLabWifi`), NAT or bridge; no WPA and no signal | None | No scenario (web server, WebDAV, OPDS, KOSync are possible); WPA and a real network: on the device |
| BLE page turner (SDK) | NimBLE + Espressif's closed controller | The radio turns on, a scan finds nothing; no link layer (in progress in the upstream project) | `ble-settings` | Pairing and page turning are never in the emulator, only on the device |
| USB | USB-CDC log | Log present; no USB storage | — | Not used |

## Known emulator limits and the version

`emu/compare.sh` uses 0.9.2 (`../tools/eink-qemu-0.9.2`; 0.7.0 stays next to it). The releases in
between matter for the HAL work:

- **0.9.1**: on the X4 Pro, the restart after the OTA slot changes boots in the same session
  (restarts that reset only the CPU keep the other core and the shared peripherals). In 0.7.0
  `ESP.restart` always hung; the update and "Start Other Firmware" flows could not be tried in the
  emulator. Tried: `silentRestart` (`RTC_SW_CPU_RST`) on leaving the Wi-Fi session loops in the ROM
  in 0.7.0, and opens Home in 0.9.2 (`emu/restart.txt`).
- **0.9.0**: in scripted runs `--sleep off` is the default: when the guest is idle, the emulator
  jumps the clock to the next timer. In this mode a `swipe` gets longer in guest time and the
  firmware does not count it as a swipe (the page is not turned); so `compare.sh` passes
  `--sleep on`.
- **0.8.0**: an explicit choice between NAT and bridge for Wi-Fi.

## Always on the device

The emulator never gives these fully right; in the HAL port they also go into
[the hardware tests](hardware-tests/README.md) as device tests:

- Absolute timing: e-ink refresh times, paths that depend on PSRAM, wake-up latency from deep
  sleep.
- Power: battery drain, sleep current, charging.
- Real SD card behavior: slow or broken cards, inserting and removing a card.
- How the front light looks, how ghosting looks.
- Real Wi-Fi (WPA, a weak signal, dropped connections) and BLE pairing.

## Suggested order

In the order of porting the HAL (first the parts the emulator shows fully):

1. **Upgrade eink-qemu to 0.9.2 ✅**: 23 scenarios pass on both builds and their screens are the
   same; the flow that restarts became a scenario with `emu/restart.txt`.
2. **Pure parts** (`HalClock` ✅ `purestwhite-rtc`): the emulator is enough. `HalMemory`
   (ESP-IDF's heap functions) and `HalSystem` (the panic handler in IRAM) are not ported: in the
   bare-metal firmware, esp-hal's allocator and error tracing tools take their place.
3. **`HalStorage`**: the emulator shows the happy path fully; the error paths are covered with host
   tests (a fake file system), because producing a card error in the emulator needs a change to
   QEMU's SD model.
4. **`HalGPIO`** (buttons, touch) and **`HalDisplay`**: the emulator is complete.
5. **`HalPowerManager`**: sleep and battery in the emulator (fuel gauge ✅ `purestwhite-gauge`);
   the charge pin needs a model added to the emulator (small: make GPIO21 drivable from outside).
6. **`HalFrontlight`**: needs an LEDC model added to the emulator (medium-sized: give the duty
   cycle of the two channels as a readable property); otherwise only on the device.
7. **Wi-Fi and BLE** (parts of the SDK): for Wi-Fi, web scenarios can be written with the fake
   access point; BLE only on the device.

Adding the charge pin and LEDC to the emulator needs our own build of eink-qemu from its source
(or a contribution to the upstream project). This is the owner's decision.

## The bare-metal firmware (stage 2)

The images of `rust/firmware` boot in eink-qemu with the C++ build's bootloader and partition
table; `esp_hal::init`, the highest clock speed, `esp-alloc`, and flash read and write
(`esp-storage`) work. `emu/baremetal-handback.txt` tests from start to end the switch to the second
slot with an SD update and the return to the C++ firmware; `emu/baremetal-handback-slot1.txt` does
the same while the C++ firmware is in ota_1. `emu/baremetal-display.txt` shows the full and the
fast refresh of the bare-metal UC8279 and UC8179 drivers with screenshots, and
`emu/baremetal-display-gray.txt` the UC8179's four gray passes (`display-check`, without esp-rtos;
the image first probes the controller; for the UC8179, `eink-qemu compare ... --panel uc8179`;
eink-qemu has no SSD1677 model for the X4 Pro, so a bus that does not answer is tested only in
`test/epd_probe`'s model). **But esp-rtos does not run in eink-qemu 0.9.2**: at the first task
switch in `esp_rtos::start`, the stack pointer slides into the peripheral area (0x6002_9xxx), the
interrupt entries spill the register windows over the SDMMC and DS registers, and QEMU crashes
(signal 11). The same happens when SYSTIMER is given as the timer instead of TIMG0 (versions tried:
esp-hal 1.2.2, esp-rtos 0.4.0); it may be a gap in the emulation of the in-CPU Software0 interrupt
(level 1) that esp-rtos uses on the S3. Until this is fixed, everything that uses Embassy can only
be tried on the device. esp-hal's SDMMC slot also crashes in interrupt-driven (async) mode, in a
different way: QEMU's digital signature (DS) model says "Invalid padding/digest" and QEMU exits
with signal 8. Floating-point operations in a bare-metal image crash it with the same message too
(in `feedback-check`, converting the touch point with `f32`; it passes with the integer
`Canvas::panel_to_logical`): the bare-metal path does not use floating point; on the device,
esp-hal keeps the floating-point unit on (`float-save-restore`). The slot works with its blocking
calls: `emu/baremetal-sd.txt` mounts the card with the bare-metal `sd-check` image, opens the FAT
partition, lists the root with its long names and reads `.crosspoint/bootloader.txt`. The SD model
has no data timeout: a data read that gets no answer (e.g. a block outside the card) makes the
emulator wait forever; on the device it ends with the controller's timeout.

eink-qemu's RTC_CNTL model has no RTC watchdog (RWDT) and no power latch: neither the watchdog's
reset nor the device turning off when GPIO1 is released on battery shows in the emulator; they
show only on the device.
