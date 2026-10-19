"""Builds EPUBs that exercise the PNG decoders (bembeyaz-png):

- png.epub, one image per chapter: every color type and bit depth the converter decodes, palette
  and color-key transparency, alpha, a large image scaled down, and last an image whose rows end
  in runs at a width where PNGdec built for a 64-bit host (the one test/png_decoder recorded)
  wrote past a row. The firmware's 32-bit build did not, so the screens should all match.
  png.epub's 600x800 palette cover goes through PngToBmpConverter (bembeyaz-png's cover reader)
  for the Home thumbnail.
- png-broken.epub: an image cut near the end of its data, on which PNGdec waited forever.

  python3 emu/make_png_epub.py OUT_DIR
"""
import struct
import sys
import zipfile
import zlib
from pathlib import Path

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)

CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def paeth(a, b, c):
    pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
    return a if pa <= pb and pa <= pc else (b if pb <= pc else c)


def png(width, height, depth, color, sample, filters=(0, 1, 2, 3, 4), palette=None, trns=None):
    """sample(x, y, c) gives each sample, below 2**depth."""
    bpp = max(1, CHANNELS[color] * depth // 8)
    raw, prev = bytearray(), None
    for y in range(height):
        acc = nbits = 0
        line = bytearray()
        for x in range(width):
            for c in range(CHANNELS[color]):
                acc = (acc << depth) | sample(x, y, c)
                nbits += depth
                if nbits == 8:
                    line.append(acc)
                    acc = nbits = 0
        if nbits:
            line.append(acc << (8 - nbits))
        prev = prev or bytes(len(line))
        f = filters[y % len(filters)]
        raw.append(f)
        for i, v in enumerate(line):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            p = [0, a, b, (a + b) // 2, paeth(a, b, c)][f]
            raw.append((v - p) & 0xFF)
        prev = bytes(line)
    parts = [b"\x89PNG\r\n\x1a\n", chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, depth, color, 0, 0, 0))]
    if palette is not None:
        parts.append(chunk(b"PLTE", palette))
    if trns is not None:
        parts.append(chunk(b"tRNS", trns))
    parts += [chunk(b"IDAT", zlib.compress(bytes(raw), 9)), chunk(b"IEND", b"")]
    return b"".join(parts)


def rings(x, y, levels, cx=150, cy=100):
    return ((x - cx) ** 2 + (y - cy) ** 2) // 300 % levels


images = {
    "Gray 8-bit": png(300, 200, 8, 0, lambda x, y, c: (x + y) * 255 // 500),
    "Gray 1-bit": png(300, 200, 1, 0, lambda x, y, c: (x // 10 + y // 10) % 2),
    "Gray 2-bit": png(300, 200, 2, 0, lambda x, y, c: rings(x, y, 4)),
    "Gray 4-bit": png(300, 200, 4, 0, lambda x, y, c: x * 16 // 300),
    "Palette 1-bit": png(
        300, 200, 1, 3, lambda x, y, c: (x // 15 + y // 15) % 2, palette=bytes([200, 40, 40, 30, 30, 120])
    ),
    "Palette 2-bit": png(
        300, 200, 2, 3, lambda x, y, c: rings(x, y, 4), palette=bytes((i * 70 + c * 20) % 256 for i in range(4) for c in range(3))
    ),
    "Palette 4-bit, partly transparent": png(
        300, 200, 4, 3, lambda x, y, c: rings(x, y, 16),
        palette=bytes((i * 16 + c * 40) % 256 for i in range(16) for c in range(3)),
        trns=bytes(range(0, 256, 32)),
    ),
    "Palette 8-bit": png(
        300, 200, 8, 3, lambda x, y, c: (x * 3 + y) % 256,
        palette=bytes(((i * 7) + c * 85) % 256 for i in range(256) for c in range(3)),
    ),
    "RGB with a color key": png(
        300, 200, 8, 2, lambda x, y, c: 200 if rings(x, y, 3) == 0 else [x * 255 // 300, y * 255 // 200, 90][c],
        trns=struct.pack(">HHH", 200, 200, 200),
    ),
    "RGBA": png(300, 200, 8, 6, lambda x, y, c: [40, 40, 40, x * 255 // 300][c]),
    "Gray and alpha": png(300, 200, 8, 4, lambda x, y, c: [0, 255 - y * 255 // 200][c]),
    "Large RGB, scaled down": png(
        1600, 1000, 8, 2, lambda x, y, c: [(x // 8) % 256, (y // 4) % 256, ((x + y) // 16) % 256][c]
    ),
    # 255 bytes a row: PNGdec's rows lay end to end in its buffer, and on a 64-bit host its zlib
    # wrote a row ending in a run (here of a delta no other row has, so zlib codes it as a repeat
    # of the byte before) 4 bytes at a time, over the start of the previous row.
    "Rows ending in runs": png(
        255, 255, 8, 0, lambda x, y, c: (x * 37 + y * 101) * 2654435761 % 256 if x < 60 else y * (y + 1) // 2 % 256,
        filters=(2,),
    ),
}


def page(title, src):
    return (
        '<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml">'
        f'<head><title>{title}</title></head><body><h2>{title}</h2><p><img src="{src}" alt=""/></p></body></html>'
    )


def epub(path, book_id, title, pictures, cover=None):
    manifest = spine = meta = ""
    files = {}
    if cover:
        manifest += '<item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/>'
        meta = '<meta name="cover" content="cover"/>'
        files["OEBPS/cover.png"] = cover
    for n, (name, data) in enumerate(pictures.items(), 1):
        manifest += f'<item id="i{n}" href="i{n}.png" media-type="image/png"/>'
        manifest += f'<item id="c{n}" href="c{n}.xhtml" media-type="application/xhtml+xml"/>'
        spine += f'<itemref idref="c{n}"/>'
        files[f"OEBPS/i{n}.png"] = data
        files[f"OEBPS/c{n}.xhtml"] = page(name, f"i{n}.png").encode()
    files["META-INF/container.xml"] = (
        b'<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
        b'<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>'
        b"</rootfiles></container>"
    )
    files["OEBPS/content.opf"] = (
        '<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" '
        'unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
        f'<dc:identifier id="id">{book_id}</dc:identifier><dc:title>{title}</dc:title><dc:language>en</dc:language>'
        f"{meta}</metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"
    ).encode()
    with zipfile.ZipFile(path, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        for name, data in files.items():
            z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)
    print(path)


# A palette cover (inflate.epub has an RGB one): rings over a gradient in indices 0-62, of which
# 60-62 are past its 60-entry palette (the converter reads them as entry 0), and a tRNS chunk it
# ignores.
cover = png(
    600,
    800,
    8,
    3,
    lambda x, y, c: (y * 48 // 800 + rings(x, y, 16, 300, 400)) % 64,
    palette=bytes(v for i in range(60) for v in (i * 4, 255 - i * 4, i * 37 % 256)),
    trns=bytes(range(0, 240, 4)),
)
epub(out_dir / "png.epub", "bembeyaz-png", "PNG images", images, cover)
# Cut 10 bytes before the end of the image data: PNGdec checked a chunk's length against the file
# size less where its 2048-byte read buffer started, not where the chunk started, so a cut this
# close to the end passed and it waited forever.
whole = images["Gray 8-bit"]
idat = whole.index(b"IDAT")
data_end = idat + 4 + struct.unpack(">I", whole[idat - 4 : idat])[0]
epub(out_dir / "png-broken.epub", "bembeyaz-png-broken", "Broken PNG", {"Cut inside its data": whole[: data_end - 10]})
