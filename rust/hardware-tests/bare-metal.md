# Hardware tests: the bare-metal firmware

This file covers the test images of the bare-metal Rust firmware (`rust/firmware`, see
[its README](../firmware/README.md)), which run in the second OTA slot and hand the device back to
the C++ firmware, and what each image reported on the X4 Pro. Each image goes on the SD card and
is installed with Settings > System > SD Card Firmware Update.

The test images' reports (`purestwhite-bare-metal.txt` on the card) and console lines were in
Turkish until 2026-10-28; they are quoted here in today's wording. On the same day the C++
firmware's bootloader file `.crosspoint/onyukleyici.txt` became `.crosspoint/bootloader.txt`, also
with English text.

## Bare-metal first test ("rust: a bare-metal smoke test for the second OTA slot")

The first step of stage 2 of the plan: `rust/firmware`, a separate bare-metal firmware on esp-hal
and Embassy. For now it is a smoke test (today the `smoke-check` image,
`rust/firmware/src/bin/smoke_check.rs`; it was the `purestwhite-firmware` binary,
`src/bin/main.rs`): it boots once in the second OTA slot; its first job is to hand the next boot
back to the C++ firmware (otadata); then it starts esp-rtos and Embassy, runs our clock and battery
drivers (`purestwhite-rtc`, `purestwhite-gauge`) unchanged on esp-hal's real I2C (the first
versions also compared the Embassy timer with the clock chip), leaves the result in RTC slow
memory and resets. At boot, the C++ firmware writes this report to the card's root as
`purestwhite-bare-metal.txt` (`HalSystem::saveBareMetalReport`). The handback test (today the
`rollback-check` image) draws nothing on the screen: after the SD update the device stays on the
old screen for a few seconds. The smoke test now paints the screen black during the touch window,
then white (see "Bare-metal touch"). Then the C++ firmware boots again, within a minute at most.

Safety: a 60-second RTC watchdog is set up first and resets any hang; then the next boot is handed
to the C++ firmware, that is, to the slot other than the one the image booted from, which is read
from the MMU (the bootloader's rollback setting is not trusted; the C++ firmware can be in ota_0 or
in ota_1); the second core is parked; a panic writes "PANIC" to the report and resets at once. The
report is saved again after every stage, so a run that stops halfway tells how far it got. In the
emulator (`emu/baremetal-handback.txt`), SD update → bare-metal boot → back to C++ → report → one
more reset passes from start to end, also while the C++ firmware is in ota_1
(`emu/baremetal-handback-slot1.txt`); a planned panic also returns to C++. But esp-rtos crashes the
emulator at the first task switch in eink-qemu 0.9.2 (see [EMULATOR-HAL.md](../EMULATOR-HAL.md)),
so the smoke test itself can only be tried on the device.

First, this version of the C++ firmware must be installed (it writes the report); the test images
are written to its other slot and overwrite the earlier firmware there ("Start Other Firmware" in
Settings now boots the test image, not the earlier version). The files are in
`~/Documents/bembeyaz/firmware-arsiv/` (their checksums next to them).

**There is no data transfer over USB, so the device cannot be recovered over USB.** The bare-metal
images are tried only if the bootloader rolls back. A bootloader that rolls back returns to the
earlier firmware by itself at the first reset after an image that does not confirm itself, even if
the image never runs. With a bootloader that does not roll back, only the bare-metal image itself
can make the return; a bug breaks the device for good.

- [x] If the card has `.crosspoint/bootloader.txt`, delete it (a hidden folder). Install this C++
  firmware (`purestWhite-next-bare-metal-rapor.bin`) over SD as usual. When it has booted, open
  the card on a computer, read `.crosspoint/bootloader.txt` and send it to me. The risk of this
  step is the same as any usual SD update. If it says "bootloader rollback: YES", you can go on to
  the steps below; if it says "NO" or "UNKNOWN", or there is no file, **stop, do not try the
  bare-metal images**.
- [x] Put `purestwhite-baremetal-geri-donus.bin` in the card's root; Settings > System > SD Card
  Firmware Update > choose the file > Confirm. The device should restart and return to the C++
  firmware within a few seconds. Read `purestwhite-bare-metal.txt` with the card in a computer
  (the reader on the device can show an old report of the same size from its cache): it should
  say "purestWhite bare-metal rollback check", "next boot: C++ firmware", "bootloader state: N"
  and "done". Send me N (1: the bootloader also rolls back by itself, 0: it does not; "no entry"
  or another number also tells something). Turn the device off and on once more: the C++ firmware
  should boot.
  - Result (2026-10-26, device): the bootloader probe says "YES (slot app1, state 2)". Before
    de64e334 the handback test ended twice without a report (the device turned off, see the note
    on the first two tries below); with de64e334: "next boot: C++ firmware", "bootloader state:
    1", "done", and the device returned to C++ by itself. The reports are in
    `firmware-arsiv/cihaz-raporlari/`.
- [x] Do the same with `purestwhite-baremetal-deneme.bin`. Send me the whole file: expected are
  "esp-rtos: started", the clock (UTC, it should be right), the battery percentage (close to the
  one in the header), "timer: 10 s = clock 10 s" (9–11 s is normal) and "done"; "PANIC" or
  missing lines show where it stopped.
- The first two tries brought no report, most likely for this reason: the image ended with a
  system reset, this reset released GPIO1, and the device, on battery, turned off (when the power
  is cut, the report in RTC memory is lost too). The images now carry GPIO1 through the reset with
  a pad hold (`hold_power_latch`, now in `rust/firmware/src/board.rs`); the watchdog also uses the
  reset that does not touch the RTC domain.
  - Result (2026-10-26, device, first try): "esp-rtos: started", "battery: 81%", "done"; the clock
    "found, unreadable", the timer "no clock". With the VL flag set, the clock driver gives no
    time, as in the C++; the device's clock was probably never set (the files on the card are
    dated 2026-01-01). The report now tells the reasons apart and measures the timer in raw
    seconds.
  - Result (second try, with 4cef0e27): "clock: not set (VL bit)", "battery: 80%", "timer: 10 s =
    clock 10 s", "done". It is confirmed that the clock was not set; Embassy's timer agrees with
    the clock chip.
- [ ] Do not turn the device off during a test: the report waits in RTC memory and is lost when
  the power is cut. Wait without touching until the home screen comes (60 seconds at most); in the
  "Bare-metal touch" test, touch only while the screen is black.
- [ ] Before each test, delete the old `purestwhite-bare-metal.txt`. If the file did not appear,
  the image did not boot, or it reset before it could leave the report; let me know.
- [ ] If a step does not bring the device back: wait 60 seconds at most (the watchdog resets it).
  If the device never returns to the C++ firmware, the Right+Power recovery mode (the SDK's
  Down+Power) does not open either (that mode is inside the C++ firmware), and the Power button
  does nothing either (it is an input that the firmware reads); it cannot be recovered over USB
  either; stop and let me know.

## Bare-metal touch ("rust: our own GT911 touch driver, in the bare-metal firmware", "rust: show the bare-metal touch window on the panel")

`purestwhite-gt911` is the driver for the X4 Pro's GT911 touch chip; for now it runs only in the
bare-metal test image (today `smoke-check`; in the C++ firmware, touch stays in the SDK's
InputManager). After the clock and the battery, the image resets the chip and finds its address;
if the chip was found, it probes the panel controller and, on a UC8279 or UC8179, paints the whole
screen black (`purestwhite-epd`, the driver in "Bare-metal display"). For about 15 seconds, while
the screen stays black, it reads touches every 20 ms; then it paints the screen white, puts the
controller to sleep and returns to C++. On an SSD1677 the screen does not change and the window is
not visible. Because it draws on the screen, it is tried only on the X4 Pro (not on the X4
Classic). The way back is the same as in "Bare-metal first test"; the bootloader rollback must be
YES.

- [x] If the card has an old `purestwhite-bare-metal.txt`, delete it. Put
  `purestwhite-baremetal-deneme.bin` on the card (the image from this commit; with the checksum in
  the archive); Settings > System > SD Card Firmware Update > choose the file > Confirm. A few
  seconds after the update, the screen flashes a few times and stays black. Only while the screen
  is black, touch in this order: first the top left corner of the screen a few times, then the
  Home key twice, then with two fingers a few times, and last put your finger on the bottom right
  corner and hold it there until the screen starts to turn white. A few seconds after the screen
  is white, the C++ home screen should come. If the screen never turns black, wait without
  touching; C++ comes within a minute at most.
- [x] Expected in the report: "touch: 0x5d" (or 0x14), "panel UC8179: black ok T s" (or UC8279),
  "frames N most K first X,Y last X,Y home H failed E", "white ok T s" and "done": N greater than
  zero, K at least 2, H at least 1, E 0 or very small. "first" should be the top left corner and
  "last" the bottom right corner; the X values and the Y values of the two points should be
  clearly different from each other (send me the values, they confirm the direction of the axes).
  T is the refresh time in seconds: a few seconds is normal; close to 8 s means the limit is tight
  (send it to me). If the screen did not turn black, the "panel" line tells why ("skipped" on an
  SSD1677; if the touch chip was not found, no "panel" line is written at all); the window still
  stays open for 15 seconds. With "black failed" or "white failed": if T is about 8 s, the
  controller did not finish that refresh in 8 seconds (BUSY did not clear); if T is short, the bus
  gave an error; after that nothing is sent to the panel ("white ok T s, sleep failed": only the
  sleep command failed). If a line stopped halfway and C++ came about a minute later, the image
  hung there and the watchdog reset it. If it says "next boot: NOT SET (...)" and there is no
  other line, the image could not hand the boot to C++, so it reset without touching anything (the
  bootloader's rollback makes the return): send this to me. A `~` at the end of a line means the
  report did not fit.
  - Result (2026-10-26, device, first try): "touch: 0x5d", "frames 71 most 1 first 794,467 last
    794,470 home 0 failed 0", "time: 15 s = clock 15 s". The driver found the chip and read it for
    15 s without errors; because the window was not visible, nobody touched during it (the touches
    came after the C++ home screen), and the steady contact at the edge was most likely the hand
    holding the device. Home and two fingers are not tested on bare-metal yet: because the window
    was not visible, the image now shows it by painting the screen black. Because the timer and the
    clock agreed in both tries, the "time" line left the report, together with "esp-rtos:
    started", to make room for the panel lines.
  - Result (2026-10-26, 715ac43b's image, second try): "panel UC8179: black ok 1.9 s", "white ok
    1.9 s": the Rust panel driver drew on a real UC8179 for the first time (the screen went black,
    then white); a full refresh takes 1.9 s, well within the 8 s limit. "frames 3 most 0 first
    -1,-1 last -1,-1 home 0 failed 0": no contact came in the window; the owner touched once while
    the screen was black, most likely before the black refresh ended (the window opens when the
    refresh ends), then the screen turned white. The test is to be done again in the order above,
    after the screen has settled on black.
  - Result (2026-10-26, third try, same image): "black ok 1.9 s", "frames 543 most 2 first 57,462
    last 710,39 home 14 failed 0", "white ok 1.9 s". The driver read for 15 s without errors; two
    fingers were recognized; top left corner (57,462), bottom right corner (710,39): the axes
    change and the corners are at opposite ends, as the SDK's transform in the panel plane gives
    (test/gt911_touch shows that it is the same as that one). Home was counted 14 times for two
    presses: the SDK also counts a press from the rise of the 0x10 bit in a new frame
    (InputManager.cpp pollGt911), so the chip reported the key many times; most likely the finger
    held on the bottom right corner was near the touch Home key below it. The touch driver is
    confirmed on the device.

## Panel controller (information, no risk)

The X4 Pro's 800x480 panel can come with an SSD1677, UC8179 or UC8279 controller; the C++ firmware
probes the bus at boot to find which one it is. The bare-metal panel driver will target the one in
the device first.

- [x] Send me the "Display Controller" line on the Settings > System > About screen.
  - Result (2026-10-26, device): UC8179. The bare-metal display test will draw with the UC8179
    driver.

## Bare-metal display ("rust: our own UC8279 panel driver, in the bare-metal firmware", "rust: probe the panel controller in the bare-metal firmware", "rust: our own UC8179 panel driver, in the bare-metal firmware", "rust: show the bare-metal touch window on the panel", "rust: the UC8179's grayscale passes", "rust: gray bands in the bare-metal display check")

`purestwhite-epd` is the black-and-white driver for the X4 Pro's UC8279 and UC8179 panel
controllers, plus the UC8179's 4-level gray passes; for now it is only in the bare-metal
`display-check` image. The image first probes the controller (the same bytes the C++ firmware sends
at every boot: 0x71, 0x70, and 0xA2 if needed) and draws if it finds a UC8279 or UC8179. On a
UC8179 it then shows four gray bands, with CrossPoint's gray passes: two reader pages (the pass for
anti-aliased text, Overlay; on the second page the bands are in reverse order, and the page is
reached with the pass without flashing that follows a gray page), then the image viewer's pass
(Absolute), then the sleep screen's pass (Direct). On an SSD1677 (no driver for it yet) it sends
no other command; its report says which controller it is. In the emulator the pattern is drawn on
both controllers, and the bands on the UC8179 too (`emu/baremetal-display-gray.txt`); the gray
passes are the same as the SDK's at the bus level (`test/epd_panels`). So the test can be done on
any X4 Pro (not on the X4 Classic: there the C++ does a different probe); the way back is the same
as in "Bare-metal first test", and the bootloader rollback must be YES. 50 seconds after boot the
image starts no new command on the panel, cuts the running one at its first wait and returns to
C++ by itself (the watchdog acts at 60 seconds).

- [x] If the card has an old `purestwhite-bare-metal.txt`, delete it. Put
  `purestwhite-baremetal-ekran.bin` on the card (in the archive, its checksum in
  `SHA256-bare-metal.txt`); Settings > System > SD Card Firmware Update > choose the file >
  Confirm. Watch without touching. On a UC8279 or UC8179 the screen should refresh to a white
  background, a thick black frame, a big black block and bars; 2 seconds later the block should
  move with a fast refresh (in its new place it covers part of the bars; this is right). On a
  UC8279 the C++ home screen should come 3 seconds later. On a UC8179, 3 seconds later the screens
  of four wide bands come, each staying 3 seconds; from one edge of the screen to the other, the
  bands are black, dark gray, light gray, white (which edge they start from does not matter):
  1. Reader page: the screen quickly changes to three black bands and one white band; right after
     that, two black bands turn dark gray and light gray.
  2. Second reader page: the bands change to the reverse order, without the screen flashing.
  3. Image: the screen flashes and changes to three black bands and one white band, then two
     black bands turn gray.
  4. The sleep screen's pass: the screen refreshes and shows the four bands directly.

  Then C++ should come. On an SSD1677, C++ comes in a few seconds without the screen changing. The
  real test is what you see: tell me where the block and the bars are, whether a ghost image is
  left, whether the four tones can be told apart on each of the four band screens (and which one
  looks best), and whether a trace of the first page is left on the second reader page.
- [x] In the report, the "panel: ..." line names the controller (it should be the same as on the
  About screen; in the line, "k" is the probe's verdict, "ver" and "flg" are raw bytes: if they do
  not match, send them to me). Expected on a UC8279 or UC8179: "panel full: ok", "panel fast: ok"
  (on a UC8179 then "gray: ok ok ok ok": the four band screens above, in order; "gray no base:"
  means the image did not find the memory for the reader page), "panel sleep: ok" and "done"; on
  an SSD1677: "panel: skipped" and "done". "ok" only shows that the bus calls gave no error and
  that the controller finished within 8 seconds, not the drawing (the bus only writes). "failed"
  mostly shows that BUSY did not clear within 8 seconds (or that 50 seconds have passed since
  boot); the steps after it are not tried and their lines are not written. If the screen freezes,
  C++ comes about a minute later and a line is missing, the image hung there and the watchdog
  reset it. If it says "next boot: NOT SET (...)" and there is no "panel:" line, the image could
  not hand the boot to C++, so it reset without touching the panel at all (the bootloader's
  rollback makes the return): send this to me.
  - Result (2026-10-26, 1e027c71's image, device): "panel: UC8179 k1 ver 000001ffff flg 13" (the
    same as the About screen), "panel full: ok", "panel fast: ok", "gray: ok ok ok ok", "panel
    sleep: ok", "done". According to the owner, everything was drawn well: the pattern and the
    moving block are right, and the four tones look separate on each of the four band screens. The
    Rust panel driver's UC8179 black-and-white path and its four gray passes are confirmed on a
    real panel. The report is in `firmware-arsiv/cihaz-raporlari/`.

## Bare-metal SD card ("rust: read the SD card in the bare-metal firmware", "rust: our own FAT file system, read path", "rust: read several SD blocks in one card command")

The bare-metal firmware can read the card; for now it only reads. The `sd-check` image mounts the
card as the SDK does: 1-bit SDMMC, 20 MHz, a pulse on the GPIO5 enable, four tries at most. Then it
opens the FAT partition with our own file system (`purestwhite-fat`), counts the entries in the
root (the first versions also wrote their names), and reads `.crosspoint/bootloader.txt` into its
report. Its latest version also reads its own image file twice, first with a separate command for
each block, then up to 32 blocks with one command, and writes both times. It writes nothing to the
card: the file system's block device has no write, and the card driver refuses every data write;
your books and settings stay as they are. The card must be FAT32 (or FAT16): it does not read
exFAT, and says "fat: failed (Unsupported)". In the emulator the card mounts, the root is listed
and the file is read (`emu/baremetal-sd.txt`). The way back is the same as in "Bare-metal first
test"; the bootloader rollback must be YES. Only on the X4 Pro: on the X4 Classic the SD slot is
the same, but the card's enable is GPIO6, and GPIO5 is a button.

The first test was done while the file system was still `embedded-sdmmc`, the second with the
`purestwhite-fat` image; both passed (below).

`purestwhite-fat` can now also write: creating, writing and deleting files ("rust: write files on
our own FAT file system"); creating and deleting directories, renames, truncation ("rust:
directories, renames and truncation on our own FAT file system"). But `sd-check` does not use this,
and the card driver still refuses writes. For now, writing is tested only on the host, with file
system images. The device test will be done on a separate test card without books; that image will
be made so that it recognizes the card with your books and refuses to write to it.

- [x] If the card has an old `purestwhite-bare-metal.txt`, delete it. Put
  `purestwhite-baremetal-sd.bin` on the card (in the archive, its checksum in
  `SHA256-bare-metal.txt`); Settings > System > SD Card Firmware Update > choose the file >
  Confirm. The screen does not change; a few seconds later the C++ home screen should come (a
  minute at most if the card does not answer). Wait without touching.
- [x] Expected in the report: "sd: try 1, ok, M MB" (M is the card's size, about 15,000 on a 16 GB
  card; several numbers, as in "try 1 2", say that the card mounted only at a later try), "fat:
  FAT32, cluster N B" (N is the size of a cluster in bytes; mostly 32768 on a 16 GB card), "root:
  K entries: ..." (the number of entries in the root and the first names), "read: bootloader
  rollback: YES (slot app1, state 2)" and "done". If there is no "done", or another line shows up
  ("not mounted", "fat: ...", "read failed", "PANIC" …), send me the whole report; if a line
  stopped halfway at a number and C++ came about a minute later, the card did not answer in that
  try and the watchdog reset it. Check with a quick look in C++ that the card did not change and
  that your books and settings are in place.
  - Result (2026-10-26, 04e8dc8e's image with `embedded-sdmmc`, device): "sd: try 1, ok, 14910
    MB", "root: 21 entries: System Volume Information, XTCache, GO", "read: bootloader rollback:
    YES (slot app1, state 2)", "done". The card mounted at the first try; the number of entries in
    the root is the same as the count on the computer (without the report file). The card driver
    and the SDK's mount order are confirmed on a real card. The report is in
    `firmware-arsiv/cihaz-raporlari/`.
  - Result (2026-10-26, a5340013's image with `purestwhite-fat`, device): "sd: try 1, ok, 14910
    MB", "fat: FAT32, cluster 8192 B", "root: 21 entries: System Volume Information, XTCache, GO",
    "read: bootloader rollback: YES (slot app1, state 2)", "done". The cluster size and the number
    of entries in the root are the same as on the computer (`stat -f`, 21 without the report file).
    Our own FAT read path is confirmed on a real card. The report is in
    `firmware-arsiv/cihaz-raporlari/`.
- [x] Multi-block read: the same steps, with the new `purestwhite-baremetal-sd.bin`. Put the file
  in the card's root with this name: the image looks for itself there by this name. Expected in
  the report:
  - "sd: try 1, ok, M MB, cmd23 yes" or "... cmd23 no": whether the card takes CMD23, which
    announces the number of blocks in advance; both are fine, the line says which path was tried.
  - "root: K entries" (names are no longer written) and "read: bootloader rollback: YES" (the
    file's first line, up to its details in brackets).
  - "image: N B, one T1 ms, many T2 ms, fnv H" and "done". N is the size of the image file; H is
    its checksum, which should be the same as the checksum of the file on the computer (I compare
    them); T2 should be clearly shorter than T1.

  "image: DIFFERENT ..." says that the two reads brought different data. "image: one read failed
  (...)" means a card error in the block-by-block read, "image: many read failed (...)" a card
  error in the multi-block read. "image: none" says that the file was not found in the root with
  this name; "image: not found (...)" or "image: not opened (...)" say that an error came up while
  looking for it or opening it. If one of these shows up, send me the whole report.
  - Result (2026-10-26, 4b2560bf's image, device): "sd: try 1, ok, 14910 MB, cmd23 yes", "fat:
    FAT32, cluster 8192 B", "root: 21 entries", "read: bootloader rollback: YES", "image: 131536
    B, one 131 ms, many 72 ms, fnv 90991353", "done". The checksum is the same as the file's on the
    computer; the card takes CMD23, so the CMD23 path that had been held back was tried. The
    multi-block read is 1.8 times as fast as the block-by-block read (about 1.8 MB/s; the limit of
    20 MHz on one data line is 2.5 MB/s). The report is in `firmware-arsiv/cihaz-raporlari/`.

## Bare-metal buttons ("rust: buttons and deep sleep in the bare-metal firmware", "rust: count the buttons without sleeping")

The bare-metal firmware reads the buttons. The `button-check` image counts the presses of Left,
Right and Power for 40 seconds (Left and Right are the keys the SDK calls Up and Down); if you hold
Power for a second, it stops counting (if you never press long, it stops when the 40 seconds are
over, and the report says "power long: no, 40 s passed"). When Power is released, it writes the
report and returns to the C++ firmware. The image draws nothing on the screen: the screen stays on
the update page; this is normal. It does not write to the card. The way back is the same as in
"Bare-metal first test"; the bootloader rollback must be YES. In the emulator the presses are
counted (`emu/baremetal-buttons.txt`).

Note: the Left key (GPIO0) is also the boot pin. Do not hold Left while the device starts; if you
do, it enters download mode.

The first version went into deep sleep after the count. On the device (2026-10-26), neither the
Power button nor the 60-second timer woke it from sleep; the screen stayed on the update page, the
owner long-pressed Power to restart the device, and C++ came. Because a full restart clears the RTC
memory, the report was lost. In the emulator both wake-ups worked: the emulator does not imitate
deep sleep in hardware detail. Sleep will be tried again with a separate diagnostic image that
recovers by itself (`sleep-check`). In these tests, if the image hangs, the way to recover is a
long press on Power.

- [x] If the card has an old `purestwhite-bare-metal.txt`, delete it. Put
  `purestwhite-baremetal-dugme.bin` on the card (in the archive, its checksum in
  `SHA256-bare-metal.txt`); Settings > System > SD Card Firmware Update > choose the file >
  Confirm. When the update ends, the screen freezes (normal). Wait 5 seconds, then within 40
  seconds press Left 3 times and Right 2 times briefly, with half a second between presses, then
  hold Power for 2 seconds and release it. The C++ home screen should come within a few seconds.
- [x] Expected in the report: "keys: left 3, right 2, power 1", "power long: yes" and "done". If
  the numbers differ from your presses, send me the whole report. If C++ does not come within a
  minute, long-press Power to restart and tell me.
  - Result (2026-10-27, with 8a55083e's sleeping image, device): "keys: up 3, down 2, power 3",
    "power long: yes", "sleep: power button or 60 s". The owner pressed Left 3 times and Right 2
    times: the count is right; the buttons and the debouncing are confirmed on the device (Power
    was pressed several times). The counting code is the same as in the new image; only the names
    became left/right and the sleep was removed. This time it returned to C++ without the report
    being lost; the device had no C++ that writes the wake-up cause, so how it returned is unclear.
    The report is in `firmware-arsiv/cihaz-raporlari/`.

## Bare-metal sleep diagnostic ("rust: a sleep diagnostic that recovers by itself")

The first deep sleep test did not wake on the device (see "Bare-metal buttons"). The `sleep-check`
image tries sleep step by step and writes each step to the report:
1. a 3-second light sleep, with the timer;
2. a light sleep until Power is pressed (20 seconds at most), on the same path that deep sleep
   also uses;
3. a deep sleep until Power is pressed, or for 15 seconds; the wake-up boots the C++ firmware.

After a light sleep, the image goes on from where it stopped and writes what happened itself.
After the deep sleep, C++ boots and adds the wake-up and reset causes to the end of the report. The
watchdog also counts during sleep: if a step hangs, at 60 seconds it writes "sleep: not entered" to
the report if the chip is still running, and 15 seconds later resets the core (the power latch and
the report are kept, C++ boots). If that does not happen either, it resets the whole system 30
seconds later; this last resort releases the latch, so a device on battery turns off and the
report is lost. The watchdog has not yet been seen to recover from sleep on the device. The image
does not draw on the screen (the screen stays on the update page) and does not write to the card.
In the emulator all three steps work (`emu/baremetal-sleep.txt`).

- [x] First install the new C++ firmware (`purestWhite-next-uyanma.bin`) over SD as usual and see
  that the C++ home screen comes. Its only difference from the C++ on the device is that it writes
  the wake-up and reset causes at the end of sleep reports; without it this test does not tell
  much.
- [x] If the card has an old `purestwhite-bare-metal.txt`, delete it. Settings > System > SD Card
  Firmware Update > `purestwhite-baremetal-uyku.bin` > Confirm. When the update ends, the screen
  freezes (normal). Then:
  1. Wait 10 seconds, press Power briefly once (this wakes it from the light sleep).
  2. Wait 5 seconds, hold Power for 2 seconds and release it (this wakes it from the deep sleep).
  3. Touch nothing. The C++ home screen should come within a few seconds.
- [ ] If C++ does not come within 2 minutes, press Power briefly; if that does not help, long-press
  it to turn the device on. Tell me which one turned it on and how long you waited; there may be
  no report, and that is a finding too (if there is no report at all, the power was cut
  completely).
- [x] Expected in the report: "sleep: light 3 s", "light 3 s: timer, ~3000 ms", "sleep: light
  power", "light power: button, ...", "sleep: deep, power or 15 s", "wake: button" and "reset:
  DEEPSLEEP". If "wake: timer" shows up, Power did not wake it from the deep sleep; the 15-second
  timer did. If the report ends at a "sleep: ..." line followed by "reset: WDT (other)", that step
  hung and the watchdog recovered it; if "sleep: not entered" is also there, the chip never managed
  to go to sleep. Send me each report in full.
  - Result (2026-10-27, 0249e415's image and C++, device): "sleep: light 3 s", "light 3 s: timer,
    3001 ms", "sleep: light power", "light power: button, 14875 ms", "sleep: deep, power or 15 s",
    "wake: button", "reset: DEEPSLEEP". All three steps worked on the device: the light sleep woke
    with the timer and with Power, the deep sleep with Power (ext1); the watchdog was not needed
    (no "sleep: not entered"). The sleep code is the same as in the first test; the difference is
    that here two light sleeps come before the deep sleep. The first image's deep sleep was the
    first sleep after boot, and on the device such a deep sleep does not end ("Bare-metal input
    and sleep", Result 2). The first image turned the watchdog off before sleeping; the report was
    lost because the owner restarted the device with a long press on Power. The report is in
    `firmware-arsiv/cihaz-raporlari/`.

## Bare-metal PSRAM ("rust: the PSRAM in the bare-metal firmware")

The bare-metal firmware can turn on the 8 MB PSRAM. The `psram-check` image starts the PSRAM with
eight lines at 40 MHz (C++ uses 80 MHz, but for that it does a timing calibration at every boot;
esp-hal does not do this calibration yet, and at 40 MHz it is not needed), adds it to the heap,
takes almost all of it with one allocation, fills it first with a pattern, then with the inverse
pattern, and reads it back; it writes the errors and the times, then returns to the C++ firmware.
(The PSRAM setup, `psram_config` and `PSRAM_MHZ`, is now in `rust/firmware/src/board.rs`.) It does
not sleep, does not write to the card and does not draw on the screen (the screen stays on the
update page; this is normal). The way back is the same as in "Bare-metal first test". In the
emulator 8192 KB is found and both passes have no errors (`emu/baremetal-psram.txt`).

- [x] If the card has an old `purestwhite-bare-metal.txt`, delete it. Put
  `purestwhite-baremetal-psram.bin` on the card (in the archive, its checksum in
  `SHA256-bare-metal.txt`); Settings > System > SD Card Firmware Update > choose the file >
  Confirm. The screen freezes; touch nothing. The C++ home screen should come within a few
  seconds. If it does not come within a minute, long-press Power to turn the device on and tell
  me.
- [x] Expected in the report: "psram: 8192 KB, 40 MHz, ...", "heap: 8128 KB, in PSRAM", "memory:
  errors 0, ..." and "memory inverted: errors 0, ...", with "done". If "errors" is greater than
  zero, or "psram: not found", "heap: ... not allocated", "OUTSIDE PSRAM" or "PANIC" shows up,
  send me the whole report.
  - Result (2026-10-28, a1559020's image, device): "psram: 8192 KB, 40 MHz, at 0x3c020000", "heap:
    8128 KB, in PSRAM", "memory: errors 0, write 474 ms, read 689 ms", "memory inverted: errors 0,
    write 474 ms, read 689 ms", "done". The PSRAM works without errors at 40 MHz in the bare-metal
    firmware: write about 17 MB/s, read and compare about 12 MB/s. The report is in
    `firmware-arsiv/cihaz-raporlari/`.

## Bare-metal Embassy tasks ("rust: Embassy tasks in the bare-metal firmware")

Can the bare-metal firmware run several jobs at once, waking on interrupts? The `tasks-check`
image starts esp-rtos and runs three tasks side by side for 30 seconds: the buttons (Left, Right,
Power; the task wakes when a pin changes or when debouncing ends), the SD card (the task mounts it
with the interrupt-driven bus, reads the image's own file again and again, and takes the checksum
of each pass) and a heartbeat every 10 ms (it measures how late it wakes). Holding Power for a
second ends the 30 seconds early. Then the image writes the report and, when Power is released (or
after 10 seconds), returns to the C++ firmware. It does not write to the card and does not draw on
the screen (the screen stays on the update page; this is normal). It does not run in the emulator
(eink-qemu crashes on these interrupts), so its first test is on the device. The way back is the
same as in "Bare-metal first test".

- [x] If the card has an old `purestwhite-bare-metal.txt`, delete it. Put
  `purestwhite-baremetal-gorev.bin` (in the archive, its checksum in `SHA256-bare-metal.txt`) in
  the card's root with exactly this name (with another name the report says "no image");
  Settings > System > SD Card Firmware Update > choose the file > Confirm. The screen freezes.
  Within 30 seconds, press Left and Right a few times, then hold Power for 2 seconds and release
  it (if you press nothing, it ends by itself after 30 seconds). The C++ home screen should come
  within a few seconds. If it does not come within a minute, long-press Power to turn the device
  on and tell me. (Today's image looks for itself as `purestwhite-baremetal-tasks.bin`; this step
  was run with the old name.)
- [x] Expected in the report: "sd: N passes, ... KB/s, fnv ..., differed 0", "keys: left X right
  Y power Z long yes, pin P timer S" (X and Y are your presses), "heart: ... beats, latest ... us"
  (about 3000 beats in 30 seconds, fewer when Power ends it early) and "done"; I compare the fnv
  with the file's. If "differed" is greater than zero, or "sd: not mounted", "no slot", "fat
  failed", "image not found", "image not opened", "read failed", "sd: not done", "power held" or
  "PANIC" shows up, send me the whole report.
  - Result (2026-10-28, f71f09f1's image, device): "sd: 240 passes, 158 KB, 2006 KB/s, fnv
    9858b7ee, differed 0", "keys: left 7 right 7 power 1 long yes, pin 29 timer 30", "heart: 1973
    beats, latest 1253 us", "done". The checksum is the same as the file's on the computer and did
    not change in 240 passes. The interrupt-driven SD read, together with the other tasks, is about
    2.0 MB/s, not slower than the blocking path (1.8 MB/s). The buttons woke only on a pin change
    and when debouncing ended (two pin wake-ups and two timer wake-ups per press). It ended with
    Power after about 20 seconds; the heartbeat was at most 1.3 ms late, so the tasks do not starve
    the executor. The report is in `firmware-arsiv/cihaz-raporlari/`.

## Bare-metal input and sleep ("rust: input and deep sleep in the bare-metal firmware")

The bare-metal firmware reads the buttons and the touch panel every 15 ms like the C++ firmware,
and goes to sleep by the C++'s rules. The `input-check` image counts the presses of Left, Right and
Power, the touches on the screen (short tap, long press, swipe) and the Home key. When you hold
Power for longer than half a second and release it, or if you touch nothing for 60 seconds, it
goes into deep sleep. Pressing Power wakes it and the C++ firmware boots; the C++ firmware writes
the wake-up and reset causes at the end of the report. If you never press Power, the device wakes
by itself after 60 seconds. The image does not draw on the screen (the screen stays on the update
page; this is normal) and does not write to the card. The way back is the same as in "Bare-metal
first test". In the emulator the counts, the sleep and the wake-up with Power work
(`emu/baremetal-input.txt`).

Note: do not hold Right while waking the device.

- [x] If the card has an old `purestwhite-bare-metal.txt`, delete it. Put
  `purestwhite-baremetal-giris-5.bin` (in the archive, its checksum in `SHA256-bare-metal.txt`) in
  the card's root; Settings > System > SD Card Firmware Update > choose the file > Confirm. When
  the update ends, the screen freezes (normal). Wait 5 seconds. Then, without a gap longer than 60
  seconds between steps: press Left 2 times and Right 2 times briefly; tap the screen briefly
  once; hold your finger on one spot for 1 second and lift it; swipe your finger quickly to one
  side on the screen; tap the Home key briefly once. Last, hold Power for 1 second and release it:
  the device goes to sleep (the screen does not change).
- [x] Wait a few seconds, then press Power as you do to wake the C++ firmware (hold it for a
  moment; if you press too briefly, the C++ takes it for a false wake-up and sleeps again; then
  press once more). The C++ home screen should come. If you press nothing, the device wakes by
  itself after 60 seconds. If C++ does not come within 2 minutes, long-press Power to turn the
  device on and tell me.
- [x] Swipes: the same install with `purestwhite-baremetal-giris-6.bin`; after 5 seconds, swipe on
  the screen 5 times with one finger at normal speed (left, right, up, down, left), then tap the
  screen briefly once, and last hold Power for 1 second and release it; a few seconds later wake
  the device with Power. Expected in the report: "down 6 tap 1 swipe 5 long 0 home 0/0" and "lift
  6 slow 0 other 0 failed 0" ("lift" counts released contacts, "slow" the contacts that took too
  long for a swipe, "other" the contacts counted as neither a tap nor a swipe, "failed" the touch
  states that could not be read). Then "sleep: deep, power" (with step traces), "wake: button" and
  "reset: DEEPSLEEP". If there are fewer than 5 swipes, send me the whole report.
  - Result 1 (2026-10-28, 3fa3167e's image `purestwhite-baremetal-giris.bin`, device, on battery):
    "touch: 0x5d", "keys: left 3 right 2 power 1", "down: 4, tap 2, swipe 1, long 1", "home: tap
    1, long 0", "sleep: deep, power", then no wake line and "reset: WDT (other)". The counts
    worked, but the deep sleep did not end (most likely it never started): neither Power nor the
    60-second timer woke it; the watchdog reset at 90 seconds brought C++ back.
  - Check (same day, `purestwhite-baremetal-uyku.bin`, 0249e415's image, checksum 1f668a3e…, on
    battery): the light sleeps and the wake-up from deep sleep with Power passed again ("wake:
    button", "reset: DEEPSLEEP"). The setup is sound.
  - Result 2 (diagnostic image `purestwhite-baremetal-giris-2.bin`, checksum b79098b2…: a 1 s
    light sleep before the deep sleep, step traces on the sleep line): "sleep: deep, power i28000
    hz1001 a r s", "wake: button", "reset: DEEPSLEEP". No old wake-up or reject flag (bits 0 and 1
    of 0x28000 are clear), the light sleep ended by the timer at 1001 ms, the deep sleep started
    and Power woke it. When the first sleep after boot is a deep sleep, it does not end; when a
    light sleep comes first, it ends. The reports are in `firmware-arsiv/cihaz-raporlari/`
    (giris-1, uyku-2-kontrol, giris-2).
  - Result 3 (experiment image `purestwhite-baremetal-giris-4.bin`, checksum a9337bca…: no light
    sleep; the automatic hold of the digital pins is turned off just before the sleep request; the
    sleep line shows the value found for that setting after `p`): "sleep: deep, power i28000
    p10000880 a r s", "wake: button", "reset: DEEPSLEEP". The automatic hold (bit 11, 0x800) was
    found on; with it turned off, the first deep sleep without a light sleep woke with Power. The
    cause: just before the sleep request, esp-hal disconnects the digital pins that are not held,
    the flash's pins included, and if the code that runs after that is not in the cache, the chip
    hangs; it does this only while the automatic hold is on (`sleep::deep_sleep` now turns it
    off). The report is `firmware-arsiv/cihaz-raporlari/giris-4.txt`.
  - Repeat (2026-10-28, c0d0d870's image `purestwhite-baremetal-giris-4.bin`): "keys: left 0 right
    0 power 0", "down: 0, ...", "sleep: deep, idle 20 s i28000 p10000880 a r s", "wake: button",
    "reset: DEEPSLEEP". No input was counted: because the screen stayed frozen, you cannot see when
    the image starts listening, and the presses came after the 20-second wait had ended. Sleeping
    when idle and waking with Power also worked on the device. From now on the wait is 60 seconds
    (`purestwhite-baremetal-giris-5.bin`). The report is
    `firmware-arsiv/cihaz-raporlari/giris-4b-bos.txt`.
  - Full test (2026-10-28, 2f7c60a5's image `purestwhite-baremetal-giris-5.bin`): "keys: left 2
    right 2 power 1", "down: 2, tap 1, swipe 0, long 1", "home: tap 1, long 0", "sleep: deep,
    power i28000 p10000880 a r s", "wake: button", "reset: DEEPSLEEP". The buttons, the tap, the
    long press (on release it was not also counted as a tap), Home, sleep with Power and the
    wake-up are right. Power was pressed three times to wake the device: the first two presses
    were short and C++ went back to sleep (the C++'s rule). The swipe was not counted, and was not
    even seen as a contact ("down: 2"); the owner swiped with one finger at normal speed, and
    swiping works in C++. In giris-1 a swipe was counted. To tell the cause apart, `giris-6` also
    counts released contacts, contacts too long for a swipe, and touch states that could not be
    read. The report is `firmware-arsiv/cihaz-raporlari/giris-5.txt`.
  - Swipe test (2026-10-28, 474e0b73's image `purestwhite-baremetal-giris-6.bin`): "down 6 tap 1
    swipe 4 long 0 home 0/0", "lift 6 slow 1 other 0 failed 0", "keys: left 0 right 0 power 2",
    "sleep: deep, power i28000 p10000880 a r s", "wake: button", "reset: DEEPSLEEP". All six
    contacts were read, with no read errors; four of the five swipes were counted as swipes, and
    one was not counted because it took longer than 700 ms ("slow"): this is the SDK's
    `TOUCH_SWIPE_MAX_MS` rule, and the C++ application uses the same with `wasSwipe`. The contact
    not seen in giris-5 did not happen again; the cause is unknown. Input and sleep passed on the
    device. The report is `firmware-arsiv/cihaz-raporlari/giris-6.txt`.

## Bare-metal feedback ("rust: on-screen feedback in the bare-metal firmware")

For the first time, the bare-metal firmware answers input on the screen. The `feedback-check`
image (`rust/firmware/src/bin/feedback_check/`) draws four boxes at the top of the screen: Left
(←), Right (→), Power (○) and Home (a house). At each press of Left, Right or Power, and at each
short tap on Home, a small square is added under that box. If you tap the screen briefly, a dot
shows where your finger touched; if you hold your finger for a second, a ring; if you swipe, a line
with a dot at its end. If the card has `/.fonts/LiberationSerif/LiberationSerif_14.cpfont` (your
card has it), the image writes the names under the boxes (Sol, Sağ, Güç, Home: the on-screen texts
are Turkish on purpose, to exercise the font's Turkish letters), and the squares get room under
the names; after the first mark, the counts "dokun … kaydır … uzun …" show at the bottom of the
screen, and before sleep "İyi geceler" shows under the crescent. If the font cannot be read, the
image draws only the shapes. Each change takes about a second to show; a short press or tap made
while the screen refreshes is not read, so wait for each mark to show before the next one. When
you hold Power for a second and release it, a crescent shows on the screen and the device goes
into deep sleep; it wakes with Power, the C++ firmware boots and writes the wake-up and reset
causes at the end of the report. It also sleeps if you touch nothing for a minute; if you never
press Power, it wakes by itself after 60 seconds. The image does not write to the card. The way
back is the same as in "Bare-metal first test". It works in the emulator
(`emu/baremetal-feedback.txt`).

Note: do not hold Right while waking the device.

- [x] If the card has an old `purestwhite-bare-metal.txt`, delete it. Put
  `purestwhite-baremetal-geri-bildirim-2.bin` (in the archive, its checksum in
  `SHA256-bare-metal.txt`) in the card's root; Settings > System > SD Card Firmware Update >
  choose the file > Confirm. Within a few seconds the screen refreshes and shows four boxes at the
  top (←, →, ○, house) and their names under them (Sol, Sağ, Güç, Home). Hold the device upright.
- [x] Waiting for each mark to show: press Left 3 times, Right 2 times and Home once, briefly (each
  press puts a square under the box's name). Tap the screen briefly once (a dot), hold your finger
  on one spot for 1 second and lift it (a ring), and swipe your finger quickly to one side on the
  screen (a line with a dot at its end). Check whether the marks are under your finger; the counts
  at the bottom should grow with each tap, long press and swipe; check whether the Turkish letters
  in the text (ğ, ü, ç, ı, İ) are right; if you can, take a photo of the screen.
- [x] Hold Power for 1 second and release it: a square shows under the Power box, then a
  full-screen crescent with "İyi geceler" under it; the device sleeps. A few seconds later press
  Power as you do to wake the C++ firmware (hold it for a moment). The C++ home screen should
  come. If it does not come within 2 minutes, long-press Power to turn the device on and tell me.
- [ ] Expected in the report: "purestWhite feedback", "panel UC8279 ok, touch 0x5d, font yes" (by
  the panel it can also be UC8179, by the touch chip 0x14; "font none" says that the font could
  not be read: tell me, even if the file is on the card); "fast N half M failed 0 T ms" (T is the
  longest refresh, the full refresh at boot, up to a few seconds); "keys 3/2/1/1 tap 1 swipe 1
  long 1" (Left, Right, Power, Home; Power also counts the press you made for the sleep); "sleep:
  deep, power i… p… a r s"; "wake: button" and "reset: DEEPSLEEP". Send me the whole report and
  the photo. Tell me in particular if "failed" is greater than zero, if "panel …: unusable",
  "panel did not sleep", "touch 0x00", "power held", "not ready" or "PANIC" shows up, if the marks
  showed up away from your finger, or if the numbers differ from your presses.
  - Result (2026-10-28, bcb3a30c's image `purestwhite-baremetal-geri-bildirim-2.bin`, device, on
    battery): "purestWhite feedback", "panel UC8179 ok, touch 0x5d, font yes", "fast 10 half 0
    failed 0 1846 ms", "keys 0/0/1/0 tap 6 swipe 1 long 2", "sleep: deep, power i28000 p10000880 a
    r s", "wake: button", "reset: DEEPSLEEP". The owner: "her şey çalışıyor" ("everything
    works"). On the UC8179 panel, drawing and fast refresh had no errors (the longest refresh 1.8
    s, the full refresh at boot); with the font on the card, the Turkish labels, the counts and
    "İyi geceler" were drawn; the tap, swipe and long press marks showed up; sleep and wake-up with
    Power worked. Left, Right and Home were not counted in this test: the owner only touched the
    screen in this test (the buttons passed on the device in `input-check`). The report is
    `firmware-arsiv/cihaz-raporlari/geri-bildirim-2.txt`.
- [x] Font in PSRAM (D3), with the cleanup below: delete the old report on the card, put
  `purestwhite-baremetal-feedback-4.bin` in the root and install it from SD; then do the steps
  above. It replaces `purestwhite-baremetal-geri-bildirim-3.bin` (built before the reports were in
  English: skip that one). The screen and the text should look the same as with `geri-bildirim-2`;
  this version reads the font into PSRAM once at boot and after that never touches the card.
  Expected in the report: "panel UC8179 ok, touch 0x5d, font 363 KB T ms" (T is the load time in
  milliseconds; expected under one second). If "font none" shows up (the font could not be read,
  or it did not fit in PSRAM), send me the whole report.
  - Result (2026-10-29, device, on battery): "panel UC8179 ok, touch 0x5d, font 363 KB 202 ms":
    the font loaded into PSRAM in 0.2 s. Report: `firmware-arsiv/cihaz-raporlari/feedback-4.txt`.

## Cleanup and English reports ("rust: a 50-line function limit; split FAT's layout and touch polling", "purestwhite-epd: an Interface trait; the UC8179 driver in modules", "firmware: board wiring in one place; test images split by job", "purestwhite-ota: the otadata choice, tested on the host", "firmware: device reports in English")

The code was split into short functions and smaller files with its behaviour unchanged: the
panel driver's bus traces were recorded identical before and after, the FAT and touch tests pass,
and every bare-metal emulator scenario passes. The otadata choice that hands the next boot back to
the C++ firmware moved into the host-tested `purestwhite-ota` crate. The report and console lines
are now in English, and the C++ firmware looks for a last line starting with "sleep:" (it was
"uyku:") before it adds "wake: …" and "reset: …".

- [x] Flash the C++ firmware of this commit first: an image's "wake:" and "reset:" lines need it.
  An older C++ firmware still saves the report, only without those two lines.
- [x] After the first boot of this C++ firmware, `/.crosspoint/bootloader.txt` is on the card with
  "bootloader rollback: YES (slot app0, state 2)" (or `app1`, by the slot it runs in). The old
  `onyukleyici.txt` next to it can be deleted.
  - Result (2026-10-29): "bootloader rollback: YES (slot app0, state 2)" (the C++ firmware of
    a8764dbb in `app0`). The old `onyukleyici.txt` was deleted.
- [x] `feedback-check` (`purestwhite-baremetal-feedback-4.bin`, the item above) runs the shared
  start (`test_image::start`), the hand-back through `purestwhite-ota`, `Board::take`,
  `panel::open`, the touch opening and the sleep: expected as in the item above, in English, with
  "next boot: C++ firmware" as the second line.
  - Result (2026-10-29, device): "purestWhite feedback", "next boot: C++ firmware", "panel UC8179
    ok, touch 0x5d, font 363 KB 202 ms", "fast 5 half 0 failed 0 1846 ms", "keys 2/2/1/0 tap 0
    swipe 0 long 0", "sleep: deep, power i28000 p10000880 a r s", "wake: button", "reset:
    DEEPSLEEP". Every refactored path ran: the hand-back, the panel, the touch controller (found
    at 0x5d), the buttons, the sleep and the wake into the C++ firmware with its new "wake:" and
    "reset:" lines. No touch gestures were counted: the owner did not touch the screen in this run
    (the touch marks passed on the device with `geri-bildirim-2`).
  - The owner noted that after the sleep one short press of Power does not wake the device. That
    is the C++ firmware's ghost-wake check (`HalGPIO::verifyPowerButtonWakeup`): after a deep
    sleep it boots back into sleep unless Power is still held, so Power has to be held for about a
    second. Settings > Short Power Button Click > Sleep makes one click wake it.

## PSRAM at 80 MHz without tuning ("firmware: a psram-80mhz feature for a PSRAM test image")

`psram-check` built with `--features psram-80mhz` runs the PSRAM at 80 MHz (memory interface
clock 160 MHz) without the timing tuning ESP-IDF does and esp-hal 1.2.2 lacks
([esp-hal#6618](https://github.com/esp-rs/esp-hal/issues/6618)). It hands the next boot back
first, so a hang at the PSRAM's start ends in a watchdog reset into the C++ firmware. The emulator
passes it (`emu/baremetal-psram.txt`), but does not model the timing: only the device can tell.

- [ ] Cold: after the device has been off for a while, install
  `purestwhite-baremetal-psram80.bin` from SD. Expected in the report: "psram: 8192 KB, 80 MHz,
  at …", "heap: N KB, in PSRAM", "memory: errors 0, …" and "memory inverted: errors 0, …", with
  write and read times shorter than at 40 MHz (send them: they show the speed gained).
- [ ] Warm: the same after an hour of reading or charging. Any error count above zero, a missing
  "memory" line or "PANIC" means 80 MHz is not safe without tuning on this device: tell me.
  - Result, slightly warm (2026-10-29, device): "psram: 8192 KB, 80 MHz, at 0x3c020000", "heap:
    8128 KB, in PSRAM", "memory: errors 0, write 406 ms, read 647 ms", the same inverted. At 40 MHz
    (2026-10-28) the same test took 474 ms to write and 689 ms to read: 14% and 6% faster. The test
    computes a pattern for every word, so it measures the CPU's loop more than the PSRAM's bus;
    copying frames or reading a font may gain more. The cold run and a run after an hour are still
    to come. Report: `firmware-arsiv/cihaz-raporlari/2026-10-29-psram80-warm.txt`.

## The Embassy main ("firmware: the purestwhite image, the Embassy main's first device run")

The `purestwhite` image (`rust/firmware/src/bin/purestwhite/`) is the start of the real bare-metal
firmware, not a test image: the application, drawing, the panel and the buttons run as Embassy
tasks side by side, and the panel refreshes without stopping the others. There is no font yet, so
the screens show shapes only: the home screen is white with one black line near the top; the
sleep screen is a crescent. The buttons do nothing yet except Power held longer than 0.4 s, which
puts the device to sleep. Like every test image it hands the next boot back to the C++ firmware
first, and it comes back to the C++ firmware on its own: after 10 minutes it puts itself to sleep,
and its sleep ends after 10 minutes at most. On the device only (the emulator does not run
esp-rtos).

Run it on battery, with USB unplugged. Hold the device upright. Do not hold Right while waking
the device (that opens the C++ recovery mode). If the C++ home screen has not come back 27 minutes
after Confirm (or 13 minutes after the crescent), long-press Power to restart (a full power cycle:
the report is then lost) and tell me. Do not long-press before that.

- [x] Delete the old `purestwhite-bare-metal.txt` on the card. Put
  `purestwhite-baremetal-main-1.bin` (in the archive, its checksum in `SHA256-bare-metal.txt`) in
  the card's root; Settings > System > SD Card Firmware Update > choose the file > Confirm. Within
  a few seconds the screen should flash, then settle white with one black line near the top. Note
  roughly how many seconds it takes from Confirm. If the screen does not change within 30 seconds
  (it stays on the update page), or flashes again about every 20 seconds, the panel's BUSY line
  did not wake the image: hold Power for a second (it still goes to sleep, within about 30
  seconds), wake it as below and send me the report.
- [ ] Press Left, Right and Power briefly a few times: nothing should change on the screen, and
  the device should not sleep.
- [x] Hold Power for about a second and release it: within about 3 seconds a crescent should show
  with a full refresh, then the device sleeps. If no crescent comes within 30 seconds, the device
  still sleeps by itself; wake it and send me the report.
- [x] A few seconds later wake it as you wake the C++ firmware: hold Power for about a second (a
  short press does not wake it). The C++ home screen should come. There, check that touch, the
  card and a page refresh work as usual.
- [x] Expected in the report, in this order: "purestWhite", "next boot: C++ firmware", "psram
  8192 KB, frames ok", "panel UC8179" (or UC8279), "panel: 3 ok, 0 failed, 0 given up, longest
  N ms, asleep" (N a few thousand at most), "sleep: deep, power i… p… a r s", then from the C++
  firmware "wake: button" and "reset: DEEPSLEEP". "PANIC", "NOT SET", "NONE", "unusable", "NOT
  asleep", "did not rest", "no sleep", "power held", a "given up" above 0, a "!" in the sleep
  line, or a missing line: send me the report.
  - Result (2026-10-29, device): "purestWhite", "next boot: C++ firmware", "psram 8192 KB, frames
    ok", "panel UC8179", "panel: 3 ok, 0 failed, 0 given up, longest 1904 ms, asleep", "sleep:
    deep, power i28000 p10000880 a r s", "wake: button", "reset: DEEPSLEEP". Every line as
    expected: the panel's BUSY interrupt wakes the task (the longest refresh 1.9 s, none given
    up), the panel and the keys rested, and Power woke the chip into the C++ firmware. Report:
    `firmware-arsiv/cihaz-raporlari/2026-10-29-purestwhite-main-1.txt`.
- [ ] Optional, the keys during a refresh: delete the report, install it again, and hold Power
  while the home screen is still flashing. The crescent should follow right after the home screen;
  then wake it as above. Expected: the same report.
- [ ] Optional, the way back on its own: delete the report, install it again, and touch nothing.
  About 10 minutes after the white screen the crescent should come by itself, and about 10
  minutes later the C++ firmware should start without a press. Expected: "panel: … asleep",
  "sleep: deep, time up i… p… a r s", "wake: timer", "reset: DEEPSLEEP".

## The scratch partition, read only ("firmware: scratch-check, the scratch partition read only")

The update gate's SD updater (step 3 of `rust/firmware/DESIGN.md`) will write a firmware image to
the flash. Before it ever writes to a firmware slot, the erase, write and read-back path is to be
proven on a part of the flash nothing uses: the `spiffs` partition (0xC90000, 3456 KB), which the
C++ firmware's sources never open. This image only reads it and reports what is there; it writes
nothing. It hands the next boot back first and resets at once. It passes in the emulator
(`emu/baremetal-scratch.txt`: all 864 sectors erased there).

- [x] First run, `purestwhite-baremetal-scratch-1.bin`.
  - Result (2026-10-29, device): "purestWhite scratch check", "next boot: C++ firmware",
    "scratch: partition table", "done". The hand-back read the partition table, but the image's
    own read of it, through a flash storage made after the hand-back had written otadata, failed;
    the emulator passed. esp-storage 0.10 reads the flash's size over SPI1 when a storage is made,
    most likely wrongly after the ROM's write: every read was then out of bounds. Since then the
    start makes one storage before any write and keeps it for the run. Report:
    `firmware-arsiv/cihaz-raporlari/2026-10-29-scratch-1.txt`.
- [x] Delete the old `purestwhite-bare-metal.txt` on the card. Put
  `purestwhite-baremetal-scratch-2.bin` (in the archive, its checksum in `SHA256-bare-metal.txt`) in
  the card's root; Settings > System > SD Card Firmware Update > choose the file > Confirm. Within a
  few seconds the C++ firmware should come back by itself.
- [ ] Expected in the report: "purestWhite scratch check", "next boot: C++ firmware", "flash 16384
  KB", "scratch 0xc90000, 3456 KB", "erased N of 864 sectors", "first used: …", "fnv …", "done".
  Send me the report whatever it says: "erased 864 of 864" and "first used: none" mean the partition
  is unused and the write path can be proven there; anything else tells what was written there once.
  If the table still cannot be read, the report says why ("table: …", "raw …, flash … KB").
  - Result (2026-10-29, device): "flash 16384 KB", "scratch 0xfd0000, 80 KB", "erased 0 of 20
    sectors", "first used 0xfd0000: [01, 80, 01, 00, ff, ff, ff, ff]", "fnv 265e7f4e", "done".
    The single storage made before any write reads the flash: the cause above is confirmed as far
    as it goes. But the device's partition table is not `partitions.csv`: its `spiffs` is 80 KB at
    0xFD0000, every sector of it in use (most likely the factory firmware's data). It is not a
    scratch area; the next test reads the whole table. Report:
    `firmware-arsiv/cihaz-raporlari/2026-10-29-scratch-2.txt`.

## The device's partition table ("firmware: table-check, the device's partition table")

The device keeps the partition table it left the factory with; the C++ firmware's updates never
write it. scratch-check found its `spiffs` partition elsewhere and in use, so before any write is
proven on the flash, the whole table must be known: the app slots' sizes, and whether any part of
the flash is free. This image only reads the table and reports it; it writes nothing, hands the
next boot back first and resets at once. It passes in the emulator (`emu/baremetal-table.txt`,
which shows `partitions.csv`).

- [x] Delete the old `purestwhite-bare-metal.txt` on the card. Put
  `purestwhite-baremetal-table-1.bin` (in the archive, its checksum in `SHA256-bare-metal.txt`) in
  the card's root; Settings > System > SD Card Firmware Update > choose the file > Confirm. Within
  a few seconds the C++ firmware should come back by itself.
- [x] Expected in the report: "purestWhite table", "next boot: C++ firmware", "flash 16384 KB",
  then one line per partition ("<label> <type>/<subtype> <offset> <size>", in hex), "run app0" or
  "run app1", "done". Send me the report.
  - Result (2026-10-29, device): "nvs 1/02 9000 5000", "otadata 1/00 e000 2000", "app0 0/10 10000
    7e0000", "app1 0/11 7f0000 7e0000", "spiffs 1/82 fd0000 14000", "coredump 1/03 fe4000 1c000",
    "run app1". Two app slots of 0x7E0000 (not `partitions.csv`'s 0x640000), no free space, the
    image in `app1` and the C++ firmware in `app0`. The write path will be proven in the last
    megabyte of the image's own slot. Report:
    `firmware-arsiv/cihaz-raporlari/2026-10-29-table-1.txt`.

## The update gate's write path ("firmware: write-check, the write path proven in the image's own slot")

Before the update gate's SD updater ever writes a firmware slot, its erase, write and read-back
path is proven on the device where a mistake harms only this image's own slot: the image copies
itself from the start of the slot it runs from (`app1` on your device, 0x7E0000 bytes) into the
last megabyte of that same slot (0xED0000 to 0xFD0000 on the flash, just before `spiffs`), far
past its own image (about 106 KB). Every write after the hand-back (clearing the target sectors,
then the copy) goes through that slot's own flash region, which cannot reach outside it: the C++
firmware's slot (`app0`) and nvs cannot be touched. The only write outside it is the hand-back's
otadata entry, made first, as in every test image, so even if the copy went wrong the C++ firmware
would start as usual. Before the copy, the sectors it will use are written to 0x00, so that the
copy can pass only if each erase really works. It passes in the emulator
(`emu/baremetal-write.txt`, where the lines read "slot 650000+640000", because the emulator uses
`partitions.csv`).

- [x] Delete the old `purestwhite-bare-metal.txt` on the card. Put
  `purestwhite-baremetal-write-1.bin` (108,704 bytes; in the archive, its checksum in
  `SHA256-bare-metal.txt`) in the card's root; Settings > System > SD Card Firmware Update >
  choose the file > Confirm. Within a few seconds the C++ firmware should come back by itself.
- [x] Expected in the report, in this order:
  - "purestWhite write check"
  - "next boot: C++ firmware"
  - "slot 7f0000+7e0000" ("slot 10000+7e0000" instead only means the C++ firmware is now in
    `app1`: the test is just as valid)
  - "image 108704 bytes, digest ok"
  - "cleared 27 sectors"
  - "copy: 27 sectors, T ms" (an estimate: about 1500 to 2000 is typical; up to 12000 is still
    within the flash chip's limits; over 12000 is not normal)
  - "read back: same digest"
  - "done"

  "PANIC", "NOT SET", "DIGESTS DIFFER", a line that starts with "slot:", "region:", "window:",
  "measure", "identify", "clear" or "copy" followed by an error, or a report that stops early:
  send me the report. A "clear at …" line that says "not 0x00" means the flash would not program
  zeros over written data; that does not block the SD updater, which always erases first. Send the
  report in any case: T is the first measure of the write path's speed (the C++ firmware's 5.4 MB
  should take roughly 50 times as long, if the time grows with the size).
  - Result (2026-10-29, device): "slot 7f0000+7e0000", "image 108704 bytes, digest ok", "cleared
    27 sectors", "copy: 27 sectors, 726 ms", "read back: same digest", "done". The erase, the
    write and the read-back work through the slot's flash region on the device, with the
    interrupt masking and the hardware SHA; about 27 ms per 4 KB sector (erase, write, read back
    and hash), so a 5.4 MB image should take about 36 s. Report:
    `firmware-arsiv/cihaz-raporlari/2026-10-29-write-1.txt`.
- [ ] Optional, later: use the C++ firmware as usual, then install any other bare-metal image
  through the SD updater. The C++ firmware should write `app1` as before: the copy left in its
  last megabyte does no harm.

## The SD updater's dry run ("firmware: update-check, the SD updater's reading side as a dry run")

The SD updater will install the C++ firmware's release from the card when the recovery mode needs
it. You chose to go one step at a time: this image does everything up to the write, and writes
nothing (but the hand-back's otadata entry, as every test image). It reads the C++ release from the
card under the fixed name `purestwhite-recovery.bin`, checks it and works out its SHA-256 as it
reads, names it among the X4 Pro releases it knows (the archive's `purestWhite-next-*.bin`), then
does the same with the C++ firmware now in `app0` and compares them. It passes in the emulator
(`emu/baremetal-update.txt`): the card's release and the slot's are the same image, "next
a8764dbb" (the archive's `purestWhite-next-reports-en-a8764dbb.bin`, which this tree's C++ build
still is); with another release on the card it says "same: no".

The card holds two `.bin` files for this test: in the update list choose
`purestwhite-baremetal-update-1.bin`, not `purestwhite-recovery.bin`. Choosing the recovery file by
mistake only installs the C++ firmware into the other slot, as any C++ update would.

- [x] Delete the old `purestwhite-bare-metal.txt` on the card. The card's root should hold
  `purestwhite-baremetal-update-1.bin` and `purestwhite-recovery.bin` (a copy of
  `purestWhite-next-reports-en-a8764dbb.bin`; both in the archive, their checksums in
  `SHA256-bare-metal.txt`). Settings > System > SD Card Firmware Update > choose
  `purestwhite-baremetal-update-1.bin` > Confirm. Within about 15 seconds the C++ firmware should
  come back by itself (reading 5.4 MB from the card takes a few seconds).
- [x] Expected in the report, in this order:
  - "purestWhite update check"
  - "next boot: C++ firmware"
  - "card 5490944 B T ms c746bebf next a8764dbb"
  - "slot 5490944 B T ms <digest> <name>"
  - "same: yes, would write 1341 sectors"
  - "done"

  If your C++ firmware is another build, the slot line names it (or says "unknown") and the last
  line says "same: no": that is a result too, it tells which build is installed. "PANIC", "NOT
  SET", or a line starting "card:", "volume:", "slot:" or "region:" followed by an error: send me
  the report. Send it in any case: the times are the first measure of reading the card and the
  slot.
  - Result (2026-10-29, device): "card 5490944 B 3979 ms c746bebf next a8764dbb", "slot 5490944
    B 1994 ms c746bebf next a8764dbb", "same: yes, would write 1341 sectors", "done". The device
    runs the archive's `purestWhite-next-reports-en-a8764dbb.bin`, and the card's copy was read,
    checked and hashed whole: the card at about 1.4 MB/s, the slot at about 2.8 MB/s (each with
    the hardware SHA). With the write path's 27 ms per sector, a real install of this release
    should take about 45 s (the first pass about 4 s, the second about 40 s). Report:
    `firmware-arsiv/cihaz-raporlari/2026-10-29-update-1.txt`.

