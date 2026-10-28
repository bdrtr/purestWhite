<p align="center">
  <img src="docs/images/purestwhite-logo.jpg" alt="A hand-drawn daisy, the purestWhite logo" width="280" />
</p>

<h1 align="center">purestWhite</h1>

<p align="center">
  E-reader firmware for the Xteink X4 Pro and X4 Classic.<br />
  Based on <a href="https://github.com/crosspoint-reader/crosspoint-reader">CrossPoint Reader</a>, moving to Rust one module at a time.
</p>

---

purestWhite is a personal fork of CrossPoint Reader. It keeps everything CrossPoint does
(EPUB rendering, KOReader progress sync, OPDS catalogs, plugins, web file transfer) and
focuses on two boards, safer networking, and a gradual rewrite of the parsing and
decoding code in Rust.

| Device | Firmware name | Build |
|---|---|---|
| Xteink X4 Pro (ESP32-S3, 8 MB PSRAM, touch) | **purestWhite next** | `pio run -e x4pro` |
| Xteink X4 Classic (same board, buttons only) | **purestWhite** | `pio run -e x4c` |

Current version: **0.0.3** (based on CrossPoint 1.6.5).

## Two firmwares in this repository

- **The C++ firmware** (`src/`, `lib/`, built with PlatformIO): CrossPoint with the changes below.
  It is the one to read books with, and the safety net: it installs firmware from the SD card.
- **The bare-metal Rust firmware** ([`rust/firmware`](rust/firmware/README.md), esp-hal and
  Embassy): a new firmware, written from scratch for the X4 Pro and tried in the second OTA slot.
  For now it is a set of test images, each of which hands the next boot back to the C++ firmware;
  its [roadmap](rust/firmware/ROADMAP.md) says what must be true before it replaces the C++ one.
  Both share the Rust crates under [`rust/crates`](rust/crates).

When the bare-metal firmware replaces the C++ one, it moves into a repository of its own, with
its history; this one then keeps only the C++ firmware.

## What is different from CrossPoint

- **Rust where it counts.** The XML, CSS, package (OPF) and ZIP parsers, DEFLATE, the PNG
  and JPEG decoders, hyphenation, bidi, paragraph layout, path and UTF-8 helpers run as
  `no_std` Rust crates under [`rust/crates`](rust/crates). The C++ headers keep their API, so the
  rest of the firmware calls them unchanged, and the existing host tests now test the Rust code.
- **HTTPS checks servers.** Certificates are verified against ESP-IDF's root bundle, with
  host-name and date checks (Settings → System → Verify Server Certificates, on by default).
- **Faster secure connections.** TLS sessions are resumed when a server allows it, and a
  KOReader sync uses one connection for all its requests.
- **Two firmwares side by side.** An SD card update writes the new firmware into the other
  OTA slot, so the previous one (for example stock CrossPoint) stays on the device.
  Settings → System → **Start Other Firmware** switches back without reflashing, after
  checking the whole image.
- **Its own name and daisy logo** on the boot and sleep screens, in the web pages, the
  hotspot name and the USB descriptor. Names that servers and plugins rely on (CrossPoint
  Sync, the plugin family, the user agent) stay CrossPoint's.

## Install

> Keep a copy of the firmware you are running. The steps below never overwrite it.

1. Build the firmware (see below).
2. Copy it to the root of the SD card, for example as `purestWhite-next-0.0.3.bin`.
3. On the device, open Settings → System → **SD Card Firmware Update** and pick the file.
   This works from stock CrossPoint too.
4. The device restarts into purestWhite. The firmware you came from stays in the other slot.

To go back, use **Start Other Firmware**. To update purestWhite without losing the other
firmware, first switch to it, then install the new file from there.

**Over Wi-Fi:** Settings → System → **Check for updates** installs the latest
[release](https://github.com/bdrtr/purestWhite/releases). It also writes into the other slot,
so it replaces the firmware kept there.

**Recovery:** hold **Down** and press **Power** while the device is off. It first offers to
start the firmware in the other slot, then lets you pick a `.bin` file from the SD card.
On the X4 Pro, do not hold **Up** at power-on: it is GPIO0 and starts the chip's USB download
mode, which leaves the screen frozen.

After installing, open the web pages at `http://purestwhite.local` (or the device's IP
address) from File Transfer. The hotspot is named after the firmware.

## Build

Requirements:

- [pioarduino PlatformIO Core](https://github.com/pioarduino/platformio-core)
- Rust with the Espressif Xtensa toolchain (`cargo +esp`, installed with
  [espup](https://github.com/esp-rs/espup))
- Python 3

```sh
git clone --recursive https://github.com/bdrtr/purestWhite
cd purestWhite
pio run -e x4pro                 # or: pio run -e x4c
pio run -e x4pro -t upload       # flash over USB
```

`scripts/build_rust.py` builds the `bembeyaz-ffi` static library for the board's MCU and
links it in. The firmware ends up in `.pio/build/<env>/firmware.bin`.

`scripts/release.sh` builds the release images (`x4pro-gh_release`, `x4c-gh_release`) for the
version in `platformio.ini` and drafts a GitHub release with them, named
`purestWhite-<version>-<board>.bin` as the updater expects. Publish the draft after the
hardware tests; devices update from the latest published release.

## Checks

Every firmware change runs:

```sh
pio run -t unit-tests                        # host tests (gtest, they link the Rust crates)
(cd rust && cargo test && cargo clippy --all-targets)
pio run -e x4pro && pio run -e x4c
emu/compare.sh                               # side by side with unmodified CrossPoint
```

`emu/compare.sh` runs a reading scenario on [eink-qemu](https://github.com/serialx/eink-qemu)
for both this tree and an unmodified CrossPoint build, and compares screenshots, heap use
and render times. Hardware tests collect by subject in
[`rust/hardware-tests/`](rust/hardware-tests/README.md) and are run on a real X4 Pro before a
release.

## Documentation

- [User guide](USER_GUIDE.md): controls, reading, sync, settings (shared with CrossPoint).
- [Rust migration](rust/README.md): method, build and emulator notes, and the porting order.
- [Bare-metal firmware](rust/firmware/README.md): the Rust firmware for the second OTA slot.
- [Hardware tests](rust/hardware-tests/README.md): what to try on the device for each change, by
  subject.
- [CrossPoint documentation](docs/): file formats, fonts, dictionaries, internals.

## Türkçe özet

purestWhite, CrossPoint Reader'ın Xteink X4 Pro ("purestWhite next") ve X4 Classic
("purestWhite") için kişisel bir çatalıdır. Ayrıştırma ve çözme kodu modül modül Rust'a
taşınıyor. HTTPS sunucu sertifikalarını doğruluyor. Cihaz iki yazılımı yan yana tutuyor;
SD kartla yükleme yapınca eski yazılım diğer yuvada kalıyor ve Ayarlar → Sistem →
Start Other Firmware ile ona geri dönülebiliyor. Kurtarma kipi: cihaz kapalıyken Aşağı
tuşunu basılı tutup Güç'e basın.

## License and credits

purestWhite is released under the [MIT license](LICENSE), like CrossPoint Reader, whose
copyright notice it keeps. It builds on the work of the
[CrossPoint Reader](https://github.com/crosspoint-reader/crosspoint-reader) community and the
[FreeInk SDK](https://github.com/Free-Ink/freeink-sdk). It is not affiliated with Xteink or
with the CrossPoint project; please report problems with this firmware here, not upstream.
