# Hardware tests: releases and first boot

These tests cover the first boot after a cache change, the two firmwares side by side in the OTA
slots, the firmware's name and logo, and the checks for versions 0.0.2 and 0.0.3.

## First boot

- [ ] **The book cache is rebuilt** (5f321eb, `BOOK_CACHE_VERSION` 11). Open a book that was
  opened before: the log must show `Cache version mismatch: expected 11, got 10` once, and then
  indexing. Close the book and open it again: there must be no indexing. The reading position
  must be kept. Every book is paginated again once, on its first open; this is normal.

## Two firmwares side by side ("feat: start the firmware in the other OTA slot")

The SD card update writes the new firmware to the free slot; the old one (e.g. CrossPoint) stays
in the other slot. Settings > System > Start Other Firmware (Turkish UI: "Diğer yazılımla başlat")
checks that firmware (segments, checksum, SHA-256, chip, board) and restarts into it without
flashing again. Recovery mode (boot with Right + Power; Right is the SDK's Down) first offers the
same choice, then lets you pick a file from the SD card. In the emulator: a flash with the switch
written to it started CrossPoint on a cold boot; a damaged slot was refused with "The other
firmware is damaged"; the SD update wrote the CrossPoint image to app1 byte for byte. The emulator
does not support a restart from software, so the restart itself after the switch can only be tried
on the device.

- [x] While CrossPoint runs, put this firmware in the root of the SD card with File Transfer
  (hotspot), and flash it with CrossPoint's SD Card Firmware Update: Bembeyaz must start; the
  settings and books must still be there.
- [x] Settings > System must show "Start Other Firmware"; the confirmation dialog must say
  "CrossPoint <version>". After you confirm: "Validating firmware..." and then a restart: the
  device must start with CrossPoint.
- [x] In CrossPoint, choose the same `firmware.bin` again with SD Card Firmware Update: Bembeyaz
  must start. After the round trip, books must open (the cache may be rebuilt).
- [x] Recovery mode: with the device off, hold Right and press Power. It must first ask "Restart
  into the other firmware?"; after Cancel, the SD file picker must open.
- [ ] If Left (the SDK's Up) is held at boot, the screen freezes (USB download mode). Write down
  how you get out of it (a long press on Power, unplugging and plugging the charging cable...).

## Name: purestWhite / purestWhite next, version 0.0.1

2026-10-20: the owner flashed 2b45110 (purestWhite next 0.0.1) on the X4 Pro with CrossPoint's SD
update and tried the ticked items; no problems.

The firmware's name is now "purestWhite" on the X4 Classic and "purestWhite next" on the X4 Pro;
its version is 0.0.1 (the version no longer has the "-x4pro" suffix). The names that servers and
plugins know stayed CrossPoint: CrossPoint Sync, the plugin family "crosspoint", the
"CrossPoint-ESP32-" user agent, Calibre's discovery reply.

- [x] The boot and sleep screens must show "purestWhite next" (X4 Pro), and "0.0.1" at the bottom
  at boot.
- [x] In Settings > System > About, the Firmware line must be "purestWhite next 0.0.1".
- [x] File Transfer > Create Hotspot: on the phone, the network must show up as "purestWhite next";
  `http://purestwhite.local` must open, and the page title and header must be "purestWhite".
- [ ] After you connect with Join a Network, the device must show up in the router's list as
  "purestWhite-<MAC>"; `http://purestwhite.local` must open.
- [ ] Calibre Wireless: the CrossPoint Reader plugin in Calibre must still find the device.
- [x] Progress sync with CrossPoint Sync must work as before.
- [ ] When the device is connected to a computer over USB, the device name must be
  "purestWhite_next" and the manufacturer "purestWhite" (Linux: `lsusb -v`).
- [x] Start Other Firmware: if CrossPoint is in the other slot, the confirmation must say
  "CrossPoint 1.6.x"; if this version of this firmware or a later one is there, it must say
  "purestWhite next 0.0.1". Bembeyaz builds from before this version carry no identity, so they
  show as "CrossPoint 1.6.5-x4pro".

## Logo: daisy

The boot and sleep logo and the web icon are now a daisy (`src/images/logo.svg`; the device image
is made with `scripts/generate_logo.py`, with 2-pixel lines).

- [x] On the boot screen, the daisy must be upright and centered; the lines must not be broken.
- [ ] On the sleep screen (Dark), the daisy must show with white lines; also with the Light sleep
  screen setting.
- [x] The daisy icon must show in the web page's header and in the browser tab.

## Version 0.0.2 ("release: 0.0.2")

Since 0.0.1: TLS session resumption, one connection per sync in KOSync, no wait on 204/304/HEAD
replies, an update channel over Wi-Fi (bdrtr/purestWhite releases), the JSON and table of contents
readers in Rust, a damaged cache string no longer crashes the reader, a table of contents label
keeps at most 1 KB, long OPDS descriptions are shown cut. This version's files wait on GitHub as a
draft; it will be published after the earlier unticked items are tried.

- [ ] In About, the Firmware line must be "purestWhite next 0.0.2", and "0.0.2" at the bottom at
  boot.
- [ ] After the draft is published, Check for Updates on a device that runs 0.0.1 must offer
  0.0.2.

## Version 0.0.3 ("release: 0.0.3")

Since 0.0.2, in Rust: container.xml, BMP, XTC, KOReader positions, the library (text, reading and
writing the index), TXT/Markdown conversion, SD card fonts (header, kerning, glyphs, intervals,
page set, advance widths), the compressed built-in fonts, the section file and its pages, the book
metadata cache, the clipping store, GfxRenderer's drawing (primitives, rounded shapes, polygons,
pictures, glyphs, screen copies), the image pipeline (dithering, cached rows, JPEG and PNG
sampling, cover scaling, size probe), the dictionary (index, XHTML, plain text, dictzip), and
these pieces of the chapter parser: word cutting, style, font size, block style, element
classification, page carry, table, list, framed box, drop cap, image size and carried footnotes.
Also a few fixes (the empty bytes of page links, short SD font glyphs, clipping of bottom-up
pictures, the KOReader position after a long element) and speed in the clipping store. All
emulator scenarios were compared with 0.0.2 (the results are in
[the 0.0.3 test plan](plan-0.0.3.md)); the device test follows that plan.

- [x] In About, the Firmware line must be "purestWhite next 0.0.3", and "0.0.3" at the bottom at
  boot; Start Other Firmware must show the earlier firmware by name ("purestWhite next 0.0.2").
- [ ] After 0.0.3 is published on GitHub, Check for Updates on a device that runs 0.0.2 must
  offer 0.0.3.
