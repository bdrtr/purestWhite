# Hardware tests: platform

These tests cover how aligned Rust allocations get their memory, and the first two slices of the
HAL move to Rust: the real-time clock driver and the fuel gauge driver.

## Aligned Rust blocks ("fix: aligned Rust allocations follow malloc's PSRAM rule")

- [ ] No visible change: Rust blocks aligned wider than 4 bytes (the XML parsers, the OPF reader)
  are now cut from a `malloc` block. Open a few books whose cache was deleted and move between
  chapters; also browse the OPDS catalog: the log must show no `Rust panic`, and there must be no
  restart or freeze.
- [x] With a BLE remote connected, open a book for the first time: the remote must stay connected,
  and page turning must work.

## Real-time clock in Rust ("rust: our own real-time clock driver")

The first slice of the HAL move. `HalClock` now reads and writes the clock with the new
`purestwhite-rtc` crate, not with the SDK's `Rtc` driver (BM8563 / PCF8563: finding the chip, BCD
time, the low-voltage flag, the century bit, conversion from UTC to Unix time). The bus is shared
with touch and the fuel gauge, so it stayed on Arduino `Wire`, with the same transfers the SDK
makes. Local time, the time zone and NTP did not change. `test/rtc_pcf8563` compares the two
drivers on the SDK's recording `Wire` mock (finding and retry, 20,000 reads, 20,000 writes,
200,000 time conversions; all the same; it caught 5 of the 5 bugs put into the Rust code). In the
emulator, `emu/clock.txt` shows 09:00 in the header and, one minute later, 09:01 in the status
bar; the screens are the same as at the earlier commit. Flash shrank by 1.3 KB on x4pro and by
1.4 KB on x4c (the code for the SDK's other clock chips is no longer linked). The boot log says
"RTC found" instead of "SDK RTC found".

The steps are the same for the X4 Pro and the X4 Classic. In the Turkish UI, some labels stay in
English ("System", "Show in Header", "Time Zone").

- [ ] The boot log must show `[CLK] RTC found` and `RTC clock read:` lines with the right date.
  Settings > System must show a "Clock" entry: it is added only when the clock chip is found.
- [ ] Turn on Settings > System > Clock > "Show in Header" and Settings > Reader > Customise Status
  Bar > Clock: the clock in the header and in the book's status bar must be right and must
  advance.
- [ ] Settings > System > Clock > "Sync Clock Now" (if Wi-Fi is off, the network selection opens;
  when you leave the screen, the device restarts quietly): the clock must be right; it must stay
  right after you turn the device off and on (the time was written to the chip).
- [ ] Change the time zone ("Time Zone"): the time shown must move to the new zone at once.
- [ ] Take a clipping: the last "Added on" line of `My Clippings.txt` in the card's root must have
  today's date and time.
- [ ] With the clock on in the status bar, turn pages by touch for a while: the clock must keep
  updating, and the serial port must not show `i2cWriteReadNonStop returned Error` or an
  "Unfinished Repeated Start" line (the clock, touch and the fuel gauge are on the same bus).

## Fuel gauge in Rust ("rust: our own fuel gauge driver")

The second slice of the HAL. `HalPowerManager` now reads the battery percentage with the new
`purestwhite-gauge` crate, not with the SDK's `BatteryMonitor` (CW2017: on the first read, it
checks the 80-byte battery profile and loads it if needed, does a soft reset, and waits until the
calculation engine is ready; on the next read, it recovers a gauge that it finds asleep or
restarted; after a failed first read, it waits one second). Drivers are now written with
embedded-hal's traits: the clock with `I2c` and `DelayNs`, the fuel gauge with the traits of
embedded-hal-async (because its first read can wait up to 4 seconds); both will run unchanged on
esp-hal in the bare-metal firmware. The battery is now read by one task at a time, with a lock (the
render task and the main task both read it). The charge state (GPIO21) did not change.
`test/cw2017_gauge` compares the two drivers in one session of 30,000 reads, with twin mock chips
that see the same faults and corruptions (sleep, restart, lost profile, invalid percentage, full
charge): each read's result, writes, transaction count and wait are the same; it caught 9 of the 9
bugs put into the Rust code. In the emulator, `emu/battery.txt` changes the percentage from 80% to
42% and to 100%, and shows it in the header; the screens are the same as at the earlier commit.
Flash grew by 2.3 KB on x4pro and on x4c (the async driver's state machine is 1.9 KB); nothing
shrank, because the SDK's CW2017 path stays linked as a fallback for other boards. The clock also
goes through the new bridge: the clock steps in the
[previous section](#real-time-clock-in-rust-rust-our-own-real-time-clock-driver) test this too.

The steps are the same for the X4 Pro and the X4 Classic. Both boards get the profile that the SDK
stores for the X4 Pro's cell (as before); it is good to compare the X4 Classic's percentage once
with the original firmware. The user cannot trigger the gauge's recovery from sleep or the profile
reload (unless the battery runs fully down or is removed); they are tested with
`test/cw2017_gauge` and the emulator.

- [ ] After boot, the battery percentage in the header must be realistic (not 0%). If the gauge
  was restarted, the first screen may come up to 4 seconds late; when the screen comes, the
  buttons must respond at once.
- [ ] Read for a while: the percentage must fall slowly; when you plug in the charger, it must
  rise, and the charging icon must show.
- [ ] Wake the device from deep sleep (waking is a new boot: the first read checks the profile,
  and writes nothing if the profile is already right): the percentage must be right at once.
- [ ] With Settings > Display > Hide Battery % > "Never" chosen, the reader's status bar must show
  the percentage; while you turn pages by touch, the serial port must not show an
  `i2cWriteReadNonStop returned Error` line (the fuel gauge, the clock and touch are on the same
  bus).
