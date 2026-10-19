"""Writes the PNG files PngDecoderCorpusTest reads: every color type and bit depth, palettes with
and without tRNS, color-key transparency, each filter type, IDAT split into many chunks, large
ancillary chunks, compression levels 0, 1 and 9, interlaced, 16-bit and too-wide images (which
the decoder refuses), and broken headers, chunk lengths and data.

    python3 test/png_decoder/make_corpus.py test/png_decoder/corpus

The files are committed; rerun only to change the corpus (and record new hashes)."""
import struct
import sys
import zlib
from pathlib import Path

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
for old in out.glob("*.png"):
    old.unlink()

SIGNATURE = b"\x89PNG\r\n\x1a\n"


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def ihdr(width, height, depth, color, interlace=0):
    return chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, depth, color, 0, 0, interlace))


CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}


def sample(x, y, c, depth, seed):
    v = (x * 37 + y * 91 + c * 53 + seed * 17 + (x * y) % 29) % 251
    return v % (1 << depth) if depth < 8 else (v if depth == 8 else v * 257 + x)


def raw_rows(width, height, depth, color, filters, seed):
    channels = CHANNELS[color]
    bits = channels * depth
    pitch = (width * bits + 7) // 8
    bpp = max(1, bits // 8)
    rows = []
    prev = bytes(pitch)
    for y in range(height):
        acc = 0
        nbits = 0
        line = bytearray()
        for x in range(width):
            for c in range(channels):
                v = sample(x, y, c, depth, seed)
                if depth == 16:
                    line += struct.pack(">H", v & 0xFFFF)
                elif depth == 8:
                    line.append(v & 0xFF)
                else:
                    acc = (acc << depth) | v
                    nbits += depth
                    if nbits == 8:
                        line.append(acc)
                        acc = nbits = 0
        if nbits:
            line.append(acc << (8 - nbits))
        f = filters[y % len(filters)]
        rows.append(bytes([f]) + filter_line(f, bytes(line), prev, bpp))
        prev = bytes(line)
    return b"".join(rows)


def filter_line(f, line, prev, bpp):
    out = bytearray()
    for i, v in enumerate(line):
        a = line[i - bpp] if i >= bpp else 0
        b = prev[i]
        c = prev[i - bpp] if i >= bpp else 0
        if f == 0 or f > 4:
            p = 0
        elif f == 1:
            p = a
        elif f == 2:
            p = b
        elif f == 3:
            p = (a + b) // 2
        else:
            pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
            p = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
        out.append((v - p) & 0xFF if f <= 4 else v)
    return bytes(out)


def png(width, height, depth, color, filters=(0, 1, 2, 3, 4), seed=0, level=6, idat_split=0,
        before=(), after=(), palette=None, trns=None, interlace=0, end=True):
    data = zlib.compress(raw_rows(width, height, depth, color, filters, seed), level)
    parts = [SIGNATURE, ihdr(width, height, depth, color, interlace)]
    parts += [chunk(k, d) for k, d in before]
    if palette is not None:
        parts.append(chunk(b"PLTE", palette))
    if trns is not None:
        parts.append(chunk(b"tRNS", trns))
    if idat_split:
        for i in range(0, len(data), idat_split):
            parts.append(chunk(b"IDAT", data[i : i + idat_split]))
    else:
        parts.append(chunk(b"IDAT", data))
    parts += [chunk(k, d) for k, d in after]
    if end:
        parts.append(chunk(b"IEND", b""))
    return b"".join(parts)


def palette(n):
    return bytes((i * 7 + c * 85) % 256 for i in range(n) for c in range(3))


files = {}
for color, depths in {0: (1, 2, 4, 8), 2: (8,), 3: (1, 2, 4, 8), 4: (8,), 6: (8,)}.items():
    for depth in depths:
        for width, height in ((1, 1), (3, 2), (7, 5), (8, 3), (9, 4), (33, 17), (64, 40)):
            kw = {}
            if color == 3:
                kw["palette"] = palette(1 << depth)
            name = f"c{color}d{depth}_{width}x{height}"
            files[name] = png(width, height, depth, color, seed=width + depth, **kw)
# Filters, one type per image and an unknown one.
for f in range(6):
    files[f"filter{f}_rgb"] = png(19, 9, 8, 2, filters=(f,), seed=f)
    files[f"filter{f}_rgba"] = png(19, 9, 8, 6, filters=(f,), seed=f + 1)
    files[f"filter{f}_gray2"] = png(23, 7, 2, 0, filters=(f,), seed=f + 2)
# Transparency.
files["pal_trns_partial"] = png(16, 8, 4, 3, palette=palette(16), trns=bytes(range(0, 200, 25)))
files["pal_trns_full"] = png(16, 8, 8, 3, palette=palette(256), trns=bytes((i * 3) % 256 for i in range(256)))
files["pal_short_palette"] = png(16, 8, 8, 3, palette=palette(10))
files["gray_trns"] = png(16, 8, 8, 0, trns=struct.pack(">H", 37))
files["gray4_trns"] = png(16, 8, 4, 0, trns=struct.pack(">H", 5))
files["rgb_trns"] = png(16, 8, 8, 2, trns=struct.pack(">HHH", 37, 128, 200))
files["rgb_trns_odd_length"] = png(16, 8, 8, 2, trns=b"\x00\x01\x02\x03")
files["trns_before_plte"] = (
    SIGNATURE + ihdr(8, 4, 2, 3) + chunk(b"tRNS", b"\x10\x20") + chunk(b"PLTE", palette(4))
    + chunk(b"IDAT", zlib.compress(raw_rows(8, 4, 2, 3, (0,), 0))) + chunk(b"IEND", b"")
)
# Chunk layout.
files["idat_1byte"] = png(40, 20, 8, 2, idat_split=1)
files["idat_7bytes"] = png(40, 20, 8, 6, idat_split=7)
files["idat_3000"] = png(300, 40, 8, 2, idat_split=3000, level=0)
files["big_text_before"] = png(30, 10, 8, 2, before=[(b"tEXt", b"Comment\x00" + b"x" * 5000)])
files["big_iccp_before"] = png(30, 10, 8, 6, before=[(b"iCCP", b"icc\x00\x00" + bytes(range(256)) * 20)])
files["many_small_before"] = png(30, 10, 8, 0, before=[(b"gAMA", b"\x00\x00\xb1\x8f")] * 40)
files["chunks_after_idat"] = png(30, 10, 8, 2, after=[(b"tEXt", b"a\x00b"), (b"zzzz", b"q" * 3000)])
files["no_iend"] = png(30, 10, 8, 2, end=False)
files["junk_after_iend"] = png(30, 10, 8, 2) + b"\x00" * 100 + b"junk"
for level in (0, 1, 9):
    files[f"level{level}_wide"] = png(1200, 6, 8, 2, level=level, seed=level)
    files[f"level{level}_tall"] = png(5, 300, 8, 0, level=level, seed=level)
# Sizes near the decoder's row buffer (16416 bytes for two rows).
files["wide_gray_8000"] = png(8000, 2, 8, 0, level=9)
files["wide_rgba_2040"] = png(2040, 2, 8, 6, level=9)
files["wide_rgba_2100"] = png(2100, 2, 8, 6, level=9)
files["wide_rgb_5470"] = png(5470, 2, 8, 2, level=9)
# Refused features.
files["interlaced"] = png(16, 16, 8, 2, interlace=1)
files["depth16_gray"] = png(8, 4, 16, 0)
files["depth16_rgba"] = png(8, 4, 16, 6)
# Broken files.
good = png(20, 10, 8, 2)
files["bad_signature"] = b"\x89PNX" + good[4:]
files["bad_ihdr_crc"] = good[:29] + bytes([good[29] ^ 1]) + good[30:]
files["bad_ihdr_length"] = good[:8] + struct.pack(">I", 14) + good[12:]
files["ihdr_not_first"] = SIGNATURE + chunk(b"gAMA", b"\x00\x00\xb1\x8f") + good[8:]
files["empty"] = b""
files["signature_only"] = SIGNATURE
files["header_only"] = good[:33]
for cut in (40, 45, 60, 80, len(good) - 13, len(good) - 5):
    files[f"cut_{cut}"] = good[:cut]
data = zlib.compress(raw_rows(20, 10, 8, 2, (0, 1, 2, 3, 4), 0))
files["idat_short"] = SIGNATURE + ihdr(20, 10, 8, 2) + chunk(b"IDAT", data[: len(data) // 2]) + chunk(b"IEND", b"")
files["idat_missing_rows"] = SIGNATURE + ihdr(20, 12, 8, 2) + chunk(b"IDAT", data) + chunk(b"IEND", b"")
files["idat_extra_rows"] = SIGNATURE + ihdr(20, 8, 8, 2) + chunk(b"IDAT", data) + chunk(b"IEND", b"")
files["idat_bad_zlib_header"] = SIGNATURE + ihdr(20, 10, 8, 2) + chunk(b"IDAT", b"\x78\x00" + data[2:]) + chunk(b"IEND", b"")
files["idat_corrupt_middle"] = (
    SIGNATURE + ihdr(20, 10, 8, 2) + chunk(b"IDAT", data[:30] + bytes([data[30] ^ 0x55]) + data[31:]) + chunk(b"IEND", b"")
)
files["idat_bad_adler"] = SIGNATURE + ihdr(20, 10, 8, 2) + chunk(b"IDAT", data[:-1] + bytes([data[-1] ^ 1])) + chunk(b"IEND", b"")
files["chunk_length_past_end"] = SIGNATURE + ihdr(20, 10, 8, 2) + struct.pack(">I", 100000) + b"IDAT" + data
files["zero_width"] = SIGNATURE + chunk(b"IHDR", struct.pack(">IIBBBBB", 0, 10, 8, 2, 0, 0, 0)) + chunk(b"IEND", b"")
files["zero_height"] = SIGNATURE + chunk(b"IHDR", struct.pack(">IIBBBBB", 10, 0, 8, 2, 0, 0, 0)) + chunk(b"IEND", b"")
files["unknown_color_type"] = SIGNATURE + chunk(b"IHDR", struct.pack(">IIBBBBB", 4, 4, 8, 5, 0, 0, 0)) + chunk(b"IEND", b"")

for name, data in files.items():
    (out / f"{name}.png").write_bytes(data)
print(len(files), "files,", sum(len(d) for d in files.values()), "bytes")
