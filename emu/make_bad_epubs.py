"""Builds malformed EPUBs for the emulator, one directory per book:

  max-size-cover/  the guide cover page claims an uncompressed size of
                   0xFFFFFFFF; CrossPoint's size + 1 for the terminator wrapped
                   to 0 and the inflate wrote past a 0-byte buffer.
  long-comment/    a valid book behind a 4000-byte archive comment; CrossPoint
                   only searched the last 1 KiB for the end record.

  python3 emu/make_bad_epubs.py OUT_DIR
"""
import struct
import sys
import zipfile
from pathlib import Path

CONTAINER = (
    '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
    '<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>'
    "</rootfiles></container>"
)


def opf(title, guide=""):
    return (
        '<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" '
        'unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
        f'<dc:identifier id="id">{title}</dc:identifier><dc:title>{title}</dc:title><dc:language>en</dc:language>'
        '</metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>'
        '<item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/></manifest>'
        f'<spine><itemref idref="c1"/></spine>{guide}</package>'
    )


def xhtml(body):
    return f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><body>{body}</body></html>'


def write_epub(path, files, comment=b""):
    with zipfile.ZipFile(path, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        for name, data in files.items():
            z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)
        z.comment = comment


def patch_uncompressed_size(path, name, value):
    data = bytearray(path.read_bytes())
    needle = b"PK\x01\x02"
    at = data.find(needle)
    while at != -1:
        name_len = struct.unpack_from("<H", data, at + 28)[0]
        if data[at + 46 : at + 46 + name_len] == name.encode():
            struct.pack_into("<I", data, at + 24, value)
            path.write_bytes(data)
            return
        at = data.find(needle, at + 4)
    raise SystemExit(f"{name} not found in {path}")


def main():
    out = Path(sys.argv[1])
    chapter = xhtml("".join(f"<p>Paragraph {i}: the quick brown fox jumps over the lazy dog.</p>" for i in range(40)))

    book = out / "max-size-cover"
    book.mkdir(parents=True, exist_ok=True)
    path = book / "max-size-cover.epub"
    write_epub(
        path,
        {
            "META-INF/container.xml": CONTAINER,
            "OEBPS/content.opf": opf(
                "Max size cover", '<guide><reference type="cover" href="cover.xhtml" title="Cover"/></guide>'
            ),
            "OEBPS/c1.xhtml": chapter,
            # Large enough that the out-of-bounds inflate tramples real heap blocks.
            "OEBPS/cover.xhtml": xhtml("<p>" + "cover " * 3000 + "</p>"),
        },
    )
    patch_uncompressed_size(path, "OEBPS/cover.xhtml", 0xFFFFFFFF)

    book = out / "long-comment"
    book.mkdir(parents=True, exist_ok=True)
    write_epub(
        book / "long-comment.epub",
        {"META-INF/container.xml": CONTAINER, "OEBPS/content.opf": opf("Long comment"), "OEBPS/c1.xhtml": chapter,
         "OEBPS/cover.xhtml": xhtml("<p>cover</p>")},
        comment=b"c" * 4000,
    )


main()
