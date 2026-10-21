"""Writes BMP files that exercise the BMP reader (bembeyaz-bmp) through the image viewer, which
dithers images whose palette is not near the display's gray levels:

- 1-1bit.bmp: 1 bit with an inverted palette (index 1 is black), odd width
- 2-2bit.bmp: 2 bits, the display's four gray levels (no dithering)
- 3-4bit.bmp: 4 bits, 16 grays, top-down
- 4-8bit-palette.bmp: 8 bits, a color palette
- 5-24bit.bmp: 24 bits, a color gradient, width not a multiple of 4
- 6-32bit.bmp: 32 bits BI_BITFIELDS (BGRA, no masks: the reader ignores them), top-down
- 7-cut.bmp: 24 bits with its pixel data cut short
- overlay/card: a 32-bit BMP with transparency as /sleep-overlay.bmp, and the settings file that
  picks the transparent sleep screen (emu/sleep.txt)

  python3 emu/make_bmp_files.py OUT_DIR
"""
import struct
import sys
from pathlib import Path

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)


def bmp(width, height, bpp, rows, palette=(), compression=0, top_down=False):
    """rows: one bytes object per row, top row first."""
    row_bytes = (width * bpp + 31) // 32 * 4
    data = b"".join(r.ljust(row_bytes, b"\0") for r in (rows if top_down else reversed(rows)))
    offset = 54 + 4 * len(palette)
    header = b"BM" + struct.pack("<IHHI", offset + len(data), 0, 0, offset)
    info = struct.pack("<IiiHHIIiiII", 40, width, -height if top_down else height, 1, bpp, compression,
                       len(data), 2835, 2835, len(palette), 0)
    return header + info + b"".join(bytes((b, g, r, 0)) for r, g, b in palette) + data


def pack(values, bpp):
    out, acc, bits = bytearray(), 0, 0
    for v in values:
        acc = (acc << bpp) | v
        bits += bpp
        if bits == 8:
            out.append(acc)
            acc, bits = 0, 0
    if bits:
        out.append(acc << (8 - bits))
    return bytes(out)


W, H = 300, 400
files = {}
files["1-1bit.bmp"] = bmp(W + 1, H, 1, [pack([((x // 20) + (y // 20)) & 1 for x in range(W + 1)], 1) for y in range(H)],
                          palette=[(255, 255, 255), (0, 0, 0)])
files["2-2bit.bmp"] = bmp(W, H, 2, [pack([(x * 4 // W + y // 100) % 4 for x in range(W)], 2) for y in range(H)],
                          palette=[(0, 0, 0), (85, 85, 85), (170, 170, 170), (255, 255, 255)])
files["3-4bit.bmp"] = bmp(W, H, 4, [pack([(x * 16 // W) ^ (y * 16 // H) for x in range(W)], 4) for y in range(H)],
                          palette=[(g * 17, g * 17, g * 17) for g in range(16)], top_down=True)
palette8 = [((i * 7) % 256, (i * 13) % 256, (255 - i)) for i in range(256)]
files["4-8bit-palette.bmp"] = bmp(W, H, 8, [bytes((x + y) % 256 for x in range(W)) for y in range(H)], palette=palette8)
files["5-24bit.bmp"] = bmp(W + 3, H, 24, [b"".join(bytes((y * 255 // H, (x * 255 // W) % 256, 128)) for x in range(W + 3))
                                          for y in range(H)])
files["6-32bit.bmp"] = bmp(W, H, 32, [b"".join(bytes((x % 256, y % 256, (x + y) % 256, 255)) for x in range(W))
                                      for y in range(H)], compression=3, top_down=True)
cut = bmp(W, H, 24, [bytes([y % 256] * 3 * W) for y in range(H)])
files["7-cut.bmp"] = cut[: len(cut) // 2]
for name, data in files.items():
    (out_dir / name).write_bytes(data)
    print(out_dir / name)

# A ring, opaque on the edge and fading inward, over a transparent background.
overlay = []
for y in range(H):
    row = bytearray()
    for x in range(W):
        d = ((x - W / 2) ** 2 + (y - H / 2) ** 2) ** 0.5
        alpha = 255 if 110 < d < 140 else max(0, 255 - int(abs(d - 125) * 4)) if d < 140 else 0
        row += bytes((x % 256, y % 256, 40, alpha))
    overlay.append(bytes(row))
card = out_dir / "overlay" / "card"
(card / ".crosspoint").mkdir(parents=True, exist_ok=True)
(card / "sleep-overlay.bmp").write_bytes(bmp(W, H, 32, overlay, compression=3))
(card / ".crosspoint" / "settings.json").write_text('{"sleepScreen": 7}\n')
print(card)
