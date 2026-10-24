"""Writes a book and two dictionary cards for emu/dictionary.txt (bembeyaz-dict, bembeyaz-html's
xhtml and plain modules, DictZip):

- dict-book.epub: one short chapter whose words need the dictionary's tricks: a plural, a past
  tense, a quoted word, a possessive with a curly apostrophe, a British spelling only the synonym
  file knows, a word whose HTML definition is too long to lay out, and one no dictionary has
- plain/card: /.dictionaries/plain/dict.{ifo,idx,dict,syn}, plain-text definitions, 2000 more
  entries so that the sidecars hold several samples, and the settings file that picks it
- html/card: the same words with HTML definitions (tag soup, entities, line breaks, one over
  16 KiB) in a dictzip .dict.dz, and its settings file

  python3 emu/make_dictionary_cards.py OUT_DIR
  emu/compare.sh emu/dictionary.txt x4pro OUT_DIR/dict-book.epub OUT_DIR/plain/card/.dictionaries=/ \\
      OUT_DIR/plain/card/.crosspoint=/
"""
import struct
import sys
import zipfile
import zlib
from pathlib import Path

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)

WORDS = "Stories walked “zebra” cat’s colour quixotic boxes running qwertyuiop"
book = {
    "META-INF/container.xml": '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
    "OEBPS/content.opf": '<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">bembeyaz-dict</dc:identifier><dc:title>Dictionary words</dc:title><dc:language>en</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>',
    "OEBPS/nav.xhtml": '<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>nav</title></head><body><nav epub:type="toc"><ol><li><a href="c1.xhtml">Words</a></li></ol></nav></body></html>',
    "OEBPS/c1.xhtml": f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c1</title></head><body><p>{" ".join(WORDS.split()[:5])}</p><p>{" ".join(WORDS.split()[5:])}</p></body></html>',
}
with zipfile.ZipFile(out_dir / "dict-book.epub", "w") as z:
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    for name, data in book.items():
        z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)

LONG = "A very long word. " * 1100  # over 16 KiB: shown as plain text
PLAIN = {
    "story": "story n. an account of events, real or imagined.",
    "walk": "walk v. to move along on foot.",
    "zebra": "zebra n. an African wild horse with black and white stripes.",
    "cat": "cat n. a small domesticated carnivorous mammal.",
    "color": "color n. the property of reflecting light of a particular wavelength.",
    "quixotic": "quixotic adj. exceedingly idealistic. " + LONG,
    "box": "box n. a container with a flat base and sides.",
    "run": "run v. to move at a speed faster than a walk.",
}
HTML = {
    "story": "<B>story</B> <i>n.</i><br>an account of events &amp; people<BR>(&#8220;tale&#8221;)",
    "walk": "<b>walk</b> <i>v.</i><ol><li>to move on foot</li><li>to accompany</li></ol>",
    "zebra": "<p><b>zebra</b> &mdash; striped <span style='color:x'>horse</span></p><hr><p>Tom & Jerry < x</p>",
    "cat": "<b>cat</b><!-- c --> <i>n.</i> a small carnivore<br/>see also: <a href=\"bword://lion\">lion</a>",
    "color": "<h3>color</h3>the property of reflecting light &#x2014; hue",
    "quixotic": "<b>quixotic</b> <i>adj.</i><br>" + LONG,
    "box": "<b>box</b> <i>n.</i> a container<br><img src='x.png'>",
    "run": "<b>run</b> <i>v.</i> to move fast<br>&eacute;&nbsp;&#233;",
}
SYNONYMS = {"colour": "color", "ran": "run"}


def ascii_case_key(word):
    return (word.lower(), word)


def stardict(defs):
    filler = {f"zz{i:05d}": f"filler entry {i}" for i in range(2000)}
    entries = sorted({**defs, **filler}.items(), key=lambda kv: ascii_case_key(kv[0]))
    data = bytearray()
    idx = bytearray()
    for word, definition in entries:
        body = definition.encode()
        idx += word.encode() + b"\0" + struct.pack(">II", len(data), len(body))
        data += body
    order = [w for w, _ in entries]
    syn = bytearray()
    for synonym, word in sorted(SYNONYMS.items(), key=lambda kv: ascii_case_key(kv[0])):
        syn += synonym.encode() + b"\0" + struct.pack(">I", order.index(word))
    return bytes(data), bytes(idx), bytes(syn), len(entries)


def dictzip(data, chunk_length=4000):
    deflate = zlib.compressobj(9, zlib.DEFLATED, -15)
    chunks = []
    for start in range(0, len(data), chunk_length):
        last = start + chunk_length >= len(data)
        body = deflate.compress(data[start:start + chunk_length])
        body += deflate.flush(zlib.Z_FINISH if last else zlib.Z_FULL_FLUSH)
        chunks.append(body)
    ra = struct.pack("<HHH", 1, chunk_length, len(chunks)) + b"".join(struct.pack("<H", len(c)) for c in chunks)
    extra = b"RA" + struct.pack("<H", len(ra)) + ra
    header = bytes([0x1F, 0x8B, 8, 0x04]) + bytes(4) + bytes([2, 3]) + struct.pack("<H", len(extra)) + extra
    return header + b"".join(chunks) + struct.pack("<II", zlib.crc32(data), len(data))


for name, defs, kind in (("plain", PLAIN, "m"), ("html", HTML, "h")):
    card = out_dir / name / "card"
    folder = card / ".dictionaries" / name
    folder.mkdir(parents=True, exist_ok=True)
    data, idx, syn, count = stardict(defs)
    (folder / "dict.idx").write_bytes(idx)
    (folder / "dict.syn").write_bytes(syn)
    if kind == "h":
        (folder / "dict.dict.dz").write_bytes(dictzip(data))
    else:
        (folder / "dict.dict").write_bytes(data)
    (folder / "dict.ifo").write_text(
        f"StarDict's dict ifo file\nversion=3.0.0\nbookname={name}\nwordcount={count}\n"
        f"idxfilesize={len(idx)}\nsametypesequence={kind}\nsynwordcount={len(SYNONYMS)}\n")
    (card / ".crosspoint").mkdir(parents=True, exist_ok=True)
    (card / ".crosspoint" / "settings.json").write_text(f'{{"dictionaryName": "{name}"}}\n')
    print(card)
print(out_dir / "dict-book.epub")
