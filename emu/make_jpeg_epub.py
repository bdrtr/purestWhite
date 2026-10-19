"""Builds jpeg.epub, which exercises the JPEG decoder (bembeyaz-jpeg): a JPEG cover (decoded by
JpegToBmpConverter for the Home thumbnail) and one image per chapter: grayscale, color at 4:4:4,
4:2:2 and 4:2:0, progressive, restart intervals, CMYK, and images large enough that the chapter
converter decodes them at 1/2, 1/4 and 1/8 size (456 pixels wide on the X4 Pro with the default
margins).

  python3 emu/make_jpeg_epub.py OUT_DIR
"""
import io
import random
import sys
import zipfile
from pathlib import Path

from PIL import Image, ImageDraw

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)


def picture(mode, width, height, seed):
    """Gradients, circles and lines: smooth areas and sharp edges."""
    rng = random.Random(seed)
    im = Image.new("RGB", (width, height))
    draw = ImageDraw.Draw(im)
    for y in range(0, height, 2):
        shade = y * 255 // max(1, height - 1)
        draw.line([(0, y), (width - 1, y)], fill=(shade, 255 - shade, (shade * 3) % 256), width=2)
    for _ in range(40):
        x, y = rng.randrange(width), rng.randrange(height)
        r = rng.randrange(4, max(5, min(width, height) // 4))
        color = tuple(rng.randrange(256) for _ in range(3))
        if rng.random() < 0.6:
            draw.ellipse([x - r, y - r, x + r, y + r], fill=color)
        else:
            draw.line([x - r, y, x + r, y + r], fill=color, width=max(1, r // 6))
    return im.convert(mode)


def jpeg(im, **kw):
    b = io.BytesIO()
    im.save(b, "JPEG", quality=kw.pop("quality", 80), **kw)
    return b.getvalue()


images = {
    "Gray": jpeg(picture("L", 300, 200, 1)),
    "Color 4:4:4": jpeg(picture("RGB", 300, 200, 2), subsampling=0),
    "Color 4:2:2": jpeg(picture("RGB", 300, 200, 3), subsampling=1),
    "Color 4:2:0, odd size": jpeg(picture("RGB", 301, 199, 4), subsampling=2),
    "Progressive gray": jpeg(picture("L", 300, 200, 5), progressive=True),
    "Progressive color": jpeg(picture("RGB", 300, 200, 6), subsampling=2, progressive=True),
    "Restart intervals": jpeg(picture("RGB", 300, 200, 7), subsampling=2, restart_marker_blocks=3),
    "CMYK": jpeg(picture("CMYK", 300, 200, 8)),
    "Large, at 1/2": jpeg(picture("RGB", 1200, 800, 9), subsampling=2),
    "Larger, at 1/4": jpeg(picture("RGB", 2400, 1600, 10), subsampling=2),
    "Wide, at 1/8": jpeg(picture("RGB", 3700, 900, 11), subsampling=2),
}
cover = jpeg(picture("RGB", 600, 800, 12), subsampling=2)


def page(title, src):
    return (
        '<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml">'
        f'<head><title>{title}</title></head><body><h2>{title}</h2><p><img src="{src}" alt=""/></p></body></html>'
    )


manifest = '<item id="cover" href="cover.jpg" media-type="image/jpeg" properties="cover-image"/>'
spine = ""
files = {"OEBPS/cover.jpg": cover}
for n, (name, data) in enumerate(images.items(), 1):
    manifest += f'<item id="i{n}" href="i{n}.jpg" media-type="image/jpeg"/>'
    manifest += f'<item id="c{n}" href="c{n}.xhtml" media-type="application/xhtml+xml"/>'
    spine += f'<itemref idref="c{n}"/>'
    files[f"OEBPS/i{n}.jpg"] = data
    files[f"OEBPS/c{n}.xhtml"] = page(name, f"i{n}.jpg").encode()
files["META-INF/container.xml"] = (
    b'<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
    b'<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>'
    b"</rootfiles></container>"
)
files["OEBPS/content.opf"] = (
    '<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" '
    'unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
    '<dc:identifier id="id">bembeyaz-jpeg</dc:identifier><dc:title>JPEG images</dc:title>'
    '<dc:language>en</dc:language><meta name="cover" content="cover"/></metadata>'
    f"<manifest>{manifest}</manifest><spine>{spine}</spine></package>"
).encode()
path = out_dir / "jpeg.epub"
with zipfile.ZipFile(path, "w") as z:
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    for name, data in files.items():
        z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)
print(path)
