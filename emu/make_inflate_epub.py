"""Builds an EPUB that exercises lib/InflateStream: a large RGB PNG cover
(zlib stream, adaptive row filters; decoded for the cover and the Home
thumbnail), a one-page chapter of stored deflate blocks (level 0) and a
~150 KB chapter at level 9, whose cold layout emu/inflate.txt measures.

  python3 emu/make_inflate_epub.py OUT_DIR
"""
import io
import random
import sys
import zipfile
from pathlib import Path

from PIL import Image, ImageDraw

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)
rng = random.Random(4)

cover = Image.new("RGB", (600, 800))
draw = ImageDraw.Draw(cover)
for y in range(800):
    draw.line([(0, y), (599, y)], fill=(y * 255 // 800, 120, 255 - y * 255 // 800))
for _ in range(400):
    x, y, r = rng.randrange(600), rng.randrange(800), rng.randrange(4, 40)
    draw.ellipse([x - r, y - r, x + r, y + r], fill=tuple(rng.randrange(256) for _ in range(3)))
png = io.BytesIO()
cover.save(png, "PNG", optimize=True)

words = "deflate window inflate stream archive chapter reader paper ink light page margin".split()


def chapter(n, count):
    paragraphs = []
    for _ in range(count):
        paragraphs.append("<p>" + " ".join(rng.choice(words) for _ in range(70)) + ".</p>")
    return (
        '<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml">'
        f"<head><title>c{n}</title></head><body><h1>Chapter {n}</h1>\n" + "\n".join(paragraphs) + "</body></html>"
    )


manifest = '<item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/>'
spine = ""
for n in (1, 2):
    manifest += f'<item id="c{n}" href="c{n}.xhtml" media-type="application/xhtml+xml"/>'
    spine += f'<itemref idref="c{n}"/>'
files = {
    "META-INF/container.xml": '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
    "OEBPS/content.opf": '<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">bembeyaz-inflate</dc:identifier><dc:title>Inflate</dc:title><dc:language>en</dc:language><meta name="cover" content="cover"/></metadata>'
    f"<manifest>{manifest}</manifest><spine>{spine}</spine></package>",
}
path = out_dir / "inflate.epub"
with zipfile.ZipFile(path, "w") as z:
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    for name, data in files.items():
        z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)
    z.writestr("OEBPS/cover.png", png.getvalue(), compress_type=zipfile.ZIP_DEFLATED)
    z.writestr("OEBPS/c1.xhtml", chapter(1, 2), compress_type=zipfile.ZIP_DEFLATED, compresslevel=0)
    z.writestr("OEBPS/c2.xhtml", chapter(2, 330), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
print(path)
