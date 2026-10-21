"""Writes XTC books that exercise the XTC reader (bembeyaz-xtc):

- book.xtc: 1-bit pages of stripes, with a title, an author and a chapter table
- book.xtch: the same as 2-bit pages (two planes of columns)

Each page shows its number as that many bars, so a wrong page is easy to spot.

  python3 emu/make_xtc_files.py OUT_DIR
"""
import struct
import sys
from pathlib import Path

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)
W, H, PAGES = 480, 800, 6
CHAPTERS = [("Bars one and two", 1, 2), ("Bars three to six", 3, 6)]


def pixel(page, x, y):
    """Gray level 0 (white) to 3 (black): page + 1 black bars, gray stripes elsewhere."""
    if 100 <= y < 700 and x // 40 < page + 1 and x % 40 < 24:
        return 3
    return (x // 8 + y // 8) % 3 if (y // 50) % 2 else 0


def page_1bit(page):
    rows = bytearray()
    for y in range(H):
        for xb in range(0, W, 8):
            byte = 0
            for bit in range(8):
                # 0 = black, 1 = white
                byte |= (0 if pixel(page, xb + bit, y) >= 2 else 1) << (7 - bit)
            rows.append(byte)
    return bytes(rows)


def page_2bit(page):
    planes = [bytearray(), bytearray()]
    for x in range(W - 1, -1, -1):  # columns right to left
        for yb in range(0, H, 8):
            hi = lo = 0
            for bit in range(8):
                value = pixel(page, x, yb + bit)
                hi |= (value >> 1) << (7 - bit)
                lo |= (value & 1) << (7 - bit)
            planes[0].append(hi)
            planes[1].append(lo)
    return bytes(planes[0] + planes[1])


def book(two_bit):
    header = bytearray(56)
    header[0:4] = b"XTCH" if two_bit else b"XTC\0"
    header[4] = 1
    struct.pack_into("<H", header, 6, PAGES)
    header[9] = 1
    header[11] = 1
    meta = bytearray(0xF8 - 56)
    title = b"XTC test (2-bit)" if two_bit else b"XTC test (1-bit)"
    meta[0x38 - 56:0x38 - 56 + len(title)] = title
    author = b"purestWhite"
    meta[0xB8 - 56:0xB8 - 56 + len(author)] = author
    chapters = bytearray()
    for name, first, last in CHAPTERS:
        record = bytearray(96)
        record[: len(name)] = name.encode()
        struct.pack_into("<HH", record, 0x50, first, last)
        chapters += record
    chapter_offset = 0xF8
    table_offset = chapter_offset + len(chapters)
    data_offset = table_offset + 16 * PAGES
    struct.pack_into("<I", header, 0x30, chapter_offset)
    struct.pack_into("<QQ", header, 0x18, table_offset, data_offset)
    table = bytearray()
    pages = bytearray()
    for p in range(PAGES):
        bitmap = page_2bit(p) if two_bit else page_1bit(p)
        page_header = (b"XTH\0" if two_bit else b"XTG\0") + struct.pack("<HHBBIQ", W, H, 0, 0, len(bitmap), 0)
        table += struct.pack("<QIHH", data_offset + len(pages), len(page_header) + len(bitmap), W, H)
        pages += page_header + bitmap
    return bytes(header + meta + chapters + table + pages)


for name, two_bit in (("book.xtc", False), ("book.xtch", True)):
    (out_dir / name).write_bytes(book(two_bit))
    print(out_dir / name)
