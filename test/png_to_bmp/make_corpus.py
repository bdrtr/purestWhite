"""Writes the PNG files PngToBmpCorpusTest reads besides test/png_decoder/corpus: what the cover
converter (lib/PngToBmpConverter) handles on its own: 16-bit samples of every color type, a palette
past 256 entries or read twice, indices past the palette, a chunk length near 4 GiB, IDAT split
around another chunk, IEND before IDAT, its 2048x3072 size limit, an unknown filter type
mid-image, unknown compression and filter methods, and pictures large enough to scale and dither.

    python3 test/png_to_bmp/make_corpus.py test/png_to_bmp/corpus

The files are committed; rerun only to change the corpus (and record a new hash)."""
import random
import struct
import sys
import zlib
from pathlib import Path

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
for old in out.glob("*.png"):
    old.unlink()

SIGNATURE = b"\x89PNG\r\n\x1a\n"
CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}


def chunk(kind, data, length=None):
    n = len(data) if length is None else length
    return struct.pack(">I", n) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def ihdr(width, height, depth, color, compression=0, filter_method=0, interlace=0):
    return chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, depth, color, compression, filter_method, interlace))


def rows(width, height, depth, color, pick, filters=(0,)):
    """Raw scanlines (each with its filter byte) of pick(x, y, channel) samples."""
    channels = CHANNELS[color]
    bits = channels * depth
    pitch = (width * bits + 7) // 8
    step = max(1, bits // 8)
    data = bytearray()
    prev = bytes(pitch)
    for y in range(height):
        line = bytearray()
        acc = nbits = 0
        for x in range(width):
            for c in range(channels):
                v = pick(x, y, c)
                if depth == 16:
                    line += struct.pack(">H", v)
                elif depth == 8:
                    line.append(v)
                else:
                    acc = (acc << depth) | v
                    nbits += depth
                    if nbits == 8:
                        line.append(acc)
                        acc = nbits = 0
        if nbits:
            line.append(acc << (8 - nbits))
        f = filters[y % len(filters)]
        data.append(f)
        data += filtered(f, bytes(line), prev, step)
        prev = bytes(line)
    return bytes(data)


def filtered(f, line, prev, step):
    out = bytearray()
    for i, v in enumerate(line):
        a = line[i - step] if i >= step else 0
        b = prev[i]
        c = prev[i - step] if i >= step else 0
        if f == 1:
            p = a
        elif f == 2:
            p = b
        elif f == 3:
            p = (a + b) // 2
        elif f == 4:
            q = a + b - c
            pa, pb, pc = abs(q - a), abs(q - b), abs(q - c)
            p = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
        else:
            p = 0
        out.append((v - p) & 0xFF)
    return bytes(out)


def png(header, *body):
    return SIGNATURE + header + b"".join(body) + chunk(b"IEND", b"")


def idat(raw):
    return chunk(b"IDAT", zlib.compress(raw, 9))


def wave(x, y, c):
    return (x * 7 + y * 13 + c * 61 + (x * y) % 37) % 256


def wave16(x, y, c):
    return wave(x, y, c) * 256 + (x * 31 + y) % 256


rng = random.Random(3)
noise = [[[rng.randrange(256) for _ in range(3)] for _ in range(120)] for _ in range(80)]
FILTERS = (0, 1, 2, 3, 4)
palette = bytes(v for i in range(256) for v in (i, (i * 3) % 256, 255 - i))

files = {}
for color in (0, 2, 4, 6):
    files[f"c{color}d16_33x17.png"] = png(ihdr(33, 17, 16, color), idat(rows(33, 17, 16, color, wave16, FILTERS)))
files["plte_300_entries.png"] = png(
    ihdr(16, 16, 8, 3), chunk(b"PLTE", palette + palette[:132]), idat(rows(16, 16, 8, 3, lambda x, y, c: (x * 16 + y) % 256))
)
files["plte_twice.png"] = png(
    ihdr(8, 8, 8, 3),
    chunk(b"PLTE", palette[:24]),
    chunk(b"PLTE", palette[600:624]),
    idat(rows(8, 8, 8, 3, lambda x, y, c: (x + y) % 8)),
)
files["index_past_palette.png"] = png(
    ihdr(12, 6, 4, 3), chunk(b"PLTE", palette[300:312]), idat(rows(12, 6, 4, 3, lambda x, y, c: (x + y * 3) % 16))
)
files["chunk_length_4gib.png"] = png(
    ihdr(8, 4, 8, 0), chunk(b"teXt", b"ab", length=0xFFFFFFFE), idat(rows(8, 4, 8, 0, wave))
)
z = zlib.compress(rows(40, 20, 8, 2, wave, FILTERS), 9)
files["idat_around_text.png"] = png(ihdr(40, 20, 8, 2), chunk(b"IDAT", z[:50]), chunk(b"tEXt", b"k\0v"), chunk(b"IDAT", z[50:]))
files["iend_before_idat.png"] = SIGNATURE + ihdr(8, 4, 8, 0) + chunk(b"IEND", b"") + idat(rows(8, 4, 8, 0, wave))
for name, (w, h) in {"width_2048": (2048, 2), "width_2049": (2049, 2), "height_3072": (1, 3072), "height_3073": (1, 3073)}.items():
    files[f"{name}.png"] = png(ihdr(w, h, 8, 0), idat(rows(w, h, 8, 0, wave)))
files["rgba16_2048.png"] = png(ihdr(2048, 2, 16, 6), idat(rows(2048, 2, 16, 6, wave16)))
files["filter_255_mid.png"] = png(ihdr(8, 4, 8, 0), idat(rows(8, 4, 8, 0, wave, (0, 1, 255, 2))))
files["compression_1.png"] = png(ihdr(8, 4, 8, 0, compression=1), idat(rows(8, 4, 8, 0, wave)))
files["filter_method_1.png"] = png(ihdr(8, 4, 8, 0, filter_method=1), idat(rows(8, 4, 8, 0, wave)))
files["noise_120x80.png"] = png(ihdr(120, 80, 8, 2), idat(rows(120, 80, 8, 2, lambda x, y, c: noise[y][x][c], FILTERS)))
files["gradient_gray_200x300.png"] = png(
    ihdr(200, 300, 8, 0), idat(rows(200, 300, 8, 0, lambda x, y, c: (x + y) * 255 // 498, FILTERS))
)
files["gray2_tall_17x90.png"] = png(ihdr(17, 90, 2, 0), idat(rows(17, 90, 2, 0, lambda x, y, c: (x + y // 7) % 4)))

for name, data in files.items():
    (out / name).write_bytes(data)
print(len(files), "files")
