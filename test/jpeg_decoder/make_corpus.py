"""Writes the JPEG files JpegDecoderCorpusTest reads: grayscale and color at 4:4:4, 4:2:2 and
4:2:0, sizes that are and are not whole MCUs, low to high quality, standard and optimized Huffman
tables, restart intervals, progressive scans, four-component (CMYK) files, metadata segments
(EXIF, ICC, comments), large and wide images, and broken markers, segment lengths and data.

    python3 test/jpeg_decoder/make_corpus.py rust/crates/bembeyaz-jpeg/testdata/corpus

The files are committed. The test's hash was recorded from JPEGDEC, which is no longer built:
changing the corpus loses that check."""
import io
import random
import struct
import sys
from pathlib import Path

from PIL import Image, ImageDraw

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
for old in out.glob("*.jpg"):
    old.unlink()


def picture(mode, width, height, seed):
    """Gradients, shapes, text-like strokes and a little noise: every frequency gets used."""
    rng = random.Random(seed)
    im = Image.new("RGB", (width, height))
    px = im.load()
    for y in range(height):
        for x in range(width):
            px[x, y] = ((x * 255) // max(1, width - 1), (y * 255) // max(1, height - 1), ((x + y) * 7) % 256)
    draw = ImageDraw.Draw(im)
    for _ in range(max(1, width * height // 400)):
        x, y = rng.randrange(width), rng.randrange(height)
        r = rng.randrange(1, max(2, min(width, height) // 3 + 1))
        color = tuple(rng.randrange(256) for _ in range(3))
        if rng.random() < 0.5:
            draw.ellipse([x - r, y - r, x + r, y + r], fill=color)
        else:
            draw.line([x, y, x + rng.randrange(-r, r + 1), y + rng.randrange(-r, r + 1)], fill=color, width=1)
    for _ in range(width * height // 50):
        x, y = rng.randrange(width), rng.randrange(height)
        px[x, y] = tuple(rng.randrange(256) for _ in range(3))
    return im.convert(mode)


def jpeg(im, **kw):
    b = io.BytesIO()
    im.save(b, "JPEG", **kw)
    return b.getvalue()


files = {}
SIZES = ((1, 1), (7, 5), (8, 8), (9, 9), (16, 16), (17, 9), (33, 17), (100, 60), (319, 201))
for w, h in SIZES:
    files[f"gray_{w}x{h}"] = jpeg(picture("L", w, h, w + h), quality=75)
    for sub, name in ((0, "444"), (1, "422"), (2, "420")):
        files[f"rgb{name}_{w}x{h}"] = jpeg(picture("RGB", w, h, w * h + sub), quality=75, subsampling=sub)
# Quality and Huffman tables.
for q in (5, 30, 95, 100):
    files[f"gray_q{q}"] = jpeg(picture("L", 64, 48, q), quality=q)
    files[f"rgb420_q{q}"] = jpeg(picture("RGB", 64, 48, q + 1), quality=q, subsampling=2)
files["gray_optimized"] = jpeg(picture("L", 120, 80, 1), quality=80, optimize=True)
files["rgb444_optimized"] = jpeg(picture("RGB", 120, 80, 2), quality=80, subsampling=0, optimize=True)
files["rgb420_optimized"] = jpeg(picture("RGB", 120, 80, 3), quality=80, subsampling=2, optimize=True)
# Restart intervals.
for blocks in (1, 3, 7):
    files[f"gray_restart{blocks}"] = jpeg(picture("L", 90, 50, blocks), quality=70, restart_marker_blocks=blocks)
    files[f"rgb420_restart{blocks}"] = jpeg(
        picture("RGB", 90, 50, blocks + 9), quality=70, subsampling=2, restart_marker_blocks=blocks
    )
files["rgb444_restart_rows"] = jpeg(picture("RGB", 90, 50, 4), quality=70, subsampling=0, restart_marker_rows=1)
# Progressive.
for w, h in ((7, 5), (33, 17), (100, 60), (319, 201)):
    files[f"prog_gray_{w}x{h}"] = jpeg(picture("L", w, h, w), quality=75, progressive=True)
    for sub, name in ((0, "444"), (2, "420")):
        files[f"prog_rgb{name}_{w}x{h}"] = jpeg(
            picture("RGB", w, h, w + sub), quality=75, subsampling=sub, progressive=True
        )
files["prog_optimized_restart"] = jpeg(
    picture("RGB", 100, 60, 5), quality=75, subsampling=2, progressive=True, optimize=True, restart_marker_blocks=4
)
# Four components (Adobe CMYK, as some EPUB covers are).
files["cmyk_64x40"] = jpeg(picture("CMYK", 64, 40, 6), quality=75)
files["prog_cmyk_64x40"] = jpeg(picture("CMYK", 64, 40, 7), quality=75, progressive=True)
# Metadata segments before the frame.
exif = Image.Exif()
exif[0x0112] = 6  # orientation: rotate 90
exif[0x010F] = "bembeyaz"
files["exif_orientation"] = jpeg(picture("RGB", 48, 32, 8), quality=75, exif=exif.tobytes())
files["icc_profile"] = jpeg(picture("RGB", 48, 32, 9), quality=75, icc_profile=bytes(range(256)) * 40)
files["comment"] = jpeg(picture("L", 48, 32, 10), quality=75, comment=b"x" * 3000)
# Large and wide images: the 1/2, 1/4 and 1/8 scales matter here.
files["large_rgb420"] = jpeg(picture("RGB", 800, 600, 11), quality=80, subsampling=2)
files["large_gray"] = jpeg(picture("L", 640, 480, 12), quality=80)
files["large_prog"] = jpeg(picture("RGB", 640, 480, 13), quality=80, subsampling=2, progressive=True)
files["wide_rgb422"] = jpeg(picture("RGB", 2100, 24, 14), quality=60, subsampling=1)
files["tall_gray"] = jpeg(picture("L", 16, 1200, 15), quality=60)

# Broken files.
good = files["rgb420_100x60"]
gray = files["gray_100x60"]


def markers(data):
    """(offset, marker, segment length) of every segment before the scan data."""
    found, p = [], 2
    while p + 4 <= len(data) and data[p] == 0xFF:
        m = data[p + 1]
        length = struct.unpack(">H", data[p + 2 : p + 4])[0]
        found.append((p, m, length))
        if m == 0xDA:
            break
        p += 2 + length
    return found


def segment(data, marker):
    return next((p, length) for p, m, length in markers(data) if m == marker)


sof, _ = segment(good, 0xC0)
sos, sos_len = segment(good, 0xDA)
files["bad_soi"] = b"\xff\xd9" + good[2:]
files["empty"] = b""
files["soi_only"] = b"\xff\xd8"
files["headers_only"] = good[: sos + 2 + sos_len]
files["no_eoi"] = good[:-2]
files["junk_after_eoi"] = good + b"\x00" * 50 + b"junk"
files["zero_width"] = good[: sof + 7] + b"\x00\x00" + good[sof + 9 :]
files["zero_height"] = good[: sof + 5] + b"\x00\x00" + good[sof + 7 :]
files["huge_dimensions"] = good[: sof + 5] + b"\xff\xff\xff\xff" + good[sof + 9 :]
files["arithmetic_sof9"] = good[: sof + 1] + b"\xc9" + good[sof + 2 :]
files["lossless_sof3"] = good[: sof + 1] + b"\xc3" + good[sof + 2 :]
files["precision_12"] = good[: sof + 4] + b"\x0c" + good[sof + 5 :]
dqt, dqt_len = segment(good, 0xDB)
files["short_dqt"] = good[: dqt + 2] + struct.pack(">H", 20) + good[dqt + 4 :]
dht, dht_len = segment(good, 0xC4)
files["dht_counts_overflow"] = good[: dht + 5] + b"\xff" * 16 + good[dht + 21 :]
files["zero_length_app"] = good[:2] + b"\xff\xe5\x00\x00" + good[2:]
files["app_length_past_end"] = good[:2] + b"\xff\xe5\xff\xff" + good[2:]
files["scan_all_zero"] = good[: sos + 2 + sos_len] + bytes(len(good) - sos - 2 - sos_len - 2) + b"\xff\xd9"
files["scan_all_ff"] = good[: sos + 2 + sos_len] + b"\xff\x00" * 200 + b"\xff\xd9"
data_start = sos + 2 + sos_len
for frac, name in ((3, "third"), (2, "half")):
    cut = data_start + (len(good) - data_start) // frac
    files[f"scan_cut_{name}"] = good[:cut]
    files[f"scan_cut_{name}_eoi"] = good[:cut] + b"\xff\xd9"
gray_sos, gray_sos_len = segment(gray, 0xDA)
files["gray_scan_cut"] = gray[: gray_sos + 2 + gray_sos_len + 40]
prog = files["prog_rgb420_100x60"]
files["prog_cut_half"] = prog[: len(prog) // 2]
files["prog_cut_late"] = prog[: len(prog) - 40]

for name, data in files.items():
    (out / f"{name}.jpg").write_bytes(data)
print(len(files), "files,", sum(len(d) for d in files.values()), "bytes")
