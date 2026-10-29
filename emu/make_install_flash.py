"""Writes 16 MiB flash images for the SD updater's install (rust/firmware's install-check) with
the C++ firmware in app0 and install-check in app1, as eink-qemu's own flash image of the C++
build lays them out (partitions.csv):

- start.bin: app1 selected, new, as the C++ firmware's switch leaves it after installing the image
- cut-first.bin, cut-mid.bin, cut-half.bin: app0 selected, as after the hand-back, and app0 as a
  write cut short leaves it: its first sector erased, its 601st erased, its 701st half written
- start-cut.bin: app1 selected, new, and app0 with its 601st sector erased: the bootloader boots
  install-check by selection, not by falling back, so it must not write app0 on that run

  mkdir -p build/baremetal/flash
  ~/Documents/bembeyaz/tools/eink-qemu-0.9.2/qemu/bin/eink-qemu flash --machine x4pro .pio/build/x4pro BASE.bin
  python3 emu/make_install_flash.py BASE.bin build/baremetal/a-install-check.bin OUT_DIR

See emu/baremetal-repair.txt and emu/baremetal-reset-mid-write.txt for the runs.
"""
import struct
import sys
from pathlib import Path

SECTOR = 4096
APP0, APP1, OTADATA = 0x10000, 0x650000, 0xE000
STATE_NEW, STATE_VALID = 0, 2

base, installer, out = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])


def seq_crc(seq):
    """ESP-IDF's esp_rom_crc32_le(UINT32_MAX, seq, 4), as purestwhite-ota's seq_crc."""
    crc = 0
    for b in struct.pack("<I", seq):
        crc ^= b
        for _ in range(8):
            crc = (crc >> 1) ^ (0xEDB88320 if crc & 1 else 0)
    return ~crc & 0xFFFFFFFF


def entry(seq, state):
    """An otadata entry selecting slot (seq - 1) % 2, in `state`."""
    e = bytearray(b"\xff" * 32)
    struct.pack_into("<I", e, 0, seq)
    struct.pack_into("<I", e, 24, state)
    struct.pack_into("<I", e, 28, seq_crc(seq))
    return bytes(e)


def make(name, otadata_sector, seq, state, damage=None):
    data = bytearray(base.read_bytes())
    image = installer.read_bytes()
    data[APP1:APP1 + len(image)] = image
    at = OTADATA + otadata_sector * SECTOR
    data[at:at + 32] = entry(seq, state)
    if damage:
        damage(data)
    (out / name).write_bytes(data)


def erase(sector, start=0):
    def damage(data):
        at = APP0 + sector * SECTOR + start
        data[at:APP0 + (sector + 1) * SECTOR] = b"\xff" * (SECTOR - start)
    return damage


out.mkdir(parents=True, exist_ok=True)
make("start.bin", 1, 2, STATE_NEW)
make("cut-first.bin", 0, 1, STATE_VALID, erase(0))
make("cut-mid.bin", 0, 1, STATE_VALID, erase(600))
make("cut-half.bin", 0, 1, STATE_VALID, erase(700, SECTOR // 2))
make("start-cut.bin", 1, 2, STATE_NEW, erase(600))
