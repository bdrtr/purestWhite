# Hardware tests

Changes that passed the emulator and the host tests but have not been tried on the X4 Pro yet.
Each new commit adds its own item to the file for its subject. All of them are tried in one
flash, with the latest commit; an item that was tried is ticked (`[x]`) or removed. If a problem
shows up, write down the item it showed up in: that commit is the one to go back to.

```sh
pio run -e x4pro -t upload && pio device monitor   # keep the serial port open for the log
```

When an earlier version is needed for comparison: `../crosspoint-base` is unmodified CrossPoint,
`../crosspoint-prev` is 609d473.

The bare-metal firmware's test images do not go over USB (the device has no USB data): they go on
the SD card and are flashed with Settings > System > SD Card Firmware Update; see
[the firmware's README](../firmware/README.md).

## By subject

- [Releases and first boot](releases.md): names, logo, versions, the two firmwares side by side,
  the first boot after a cache change.
- [Network](network.md): server certificates, TLS session resumption, time sync, KOSync, update
  over Wi-Fi, BLE remotes.
- [Platform](platform.md): Rust allocations, the real-time clock and the fuel gauge drivers.
- [Parsing](parsing.md): XML, paths, the package document, container.xml, JSON, TOC readers, XTC,
  KOReader positions, the Library, text to XHTML, web paths.
- [Caches](caches.md): section files and pages, the book metadata cache, the clipping store.
- [Text layout](layout.md): paragraph layout, CSS, hyphenation, bidirectional text, tables, the
  chapter parser and its pieces.
- [Fonts](fonts.md): DEFLATE, inflate, the SD card font readers, the compressed built-in fonts.
- [Images](images.md): PNG, JPEG and BMP, covers, the sleep overlay, dithering and image sampling.
- [Drawing](drawing.md): drawing primitives, shapes, pictures, glyphs, screen region copies.
- [Dictionary](dictionary.md): the dictionary index, its XHTML and plain-text definitions,
  dictzip.
- [The bare-metal firmware](bare-metal.md): the test images for the second OTA slot.

[The 0.0.3 test plan](plan-0.0.3.md) groups the items open at release 0.0.3 into sessions that can
be run on the device together.
