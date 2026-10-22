"""Builds a large clipping store for the book from make_utf8_epub.py: 1000 clippings of 4096
bytes each (about 4 MiB) in spine item 99, which the book does not have, so none shows on a
page. Adding or removing a clipping rewrites the store and copies every text. Prints the
.crosspoint folder to put on the card:

  book="$(python3 emu/make_utf8_epub.py emu/books)"
  store="$(python3 emu/make_clipping_store.py emu/books)"
  emu/compare.sh emu/clipping-large.txt x4pro "$book" "$store"

The record layout is version 4 of rust/crates/bembeyaz-clippings (`save`); update both."""
import struct, sys, unicodedata
from pathlib import Path

COUNT = 1000
# The book's path as the firmware sees it on the card (NFC).
BOOK = "/" + unicodedata.normalize("NFC", "Çiçekli-Şiir") + ".epub"


def std_hash(data):
    """std::hash<std::string> of the firmware's 32-bit libstdc++: MurmurHash2, seed 0xc70f6907."""
    m, mask = 0x5BD1E995, 0xFFFFFFFF
    h = (0xC70F6907 ^ len(data)) & mask
    whole = len(data) // 4 * 4
    for i in range(0, whole, 4):
        k = int.from_bytes(data[i : i + 4], "little") * m & mask
        k = (k ^ k >> 24) * m & mask
        h = (h * m & mask) ^ k
    tail = data[whole:]
    if len(tail) == 3:
        h ^= tail[2] << 16
    if len(tail) >= 2:
        h ^= tail[1] << 8
    if tail:
        h = (h ^ tail[0]) * m & mask
    h = (h ^ h >> 13) * m & mask
    return h ^ h >> 15


# The store the firmware made for this book in the emulator.
assert std_hash(BOOK.encode()) == 3956371232


def string(s):
    b = s.encode()
    return struct.pack("<I", len(b)) + b


out = Path(sys.argv[1]) / "clippings" / ".crosspoint"
folder = out / "clippings"
folder.mkdir(parents=True, exist_ok=True)
data = bytearray(struct.pack("<BH", 4, COUNT))
data += string("") + string("") + string(BOOK)
for i in range(COUNT):
    # spine 99, pages 0-0 of 1, words 0-0 of 1, paragraph 0, no timestamp, layout 1, no source
    # offsets, revision 0, pending upload, no id.
    data += struct.pack("<8HIIIIQB", 99, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0xFFFFFFFF, 0xFFFFFFFF, 0, 1)
    data += bytes(65) + f"Filler {i}".encode().ljust(48, b"\0")
    text = (f"filler text {i} " * 400).encode()[:4096]
    data += struct.pack("<H", len(text)) + text
(folder / f"epub_{std_hash(BOOK.encode())}.bin").write_bytes(data)
print(out)
