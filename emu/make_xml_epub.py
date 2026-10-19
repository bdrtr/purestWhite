"""Builds an EPUB whose files exercise the XML parser (rust/crates/bembeyaz-xml) in every reader:
container.xml with a comment and CR LF line ends; a package document with entities, CDATA, a
comment and series metadata; an NCX behind an external DTD whose labels use &nbsp;; a nav
document; and chapters in XHTML 1.1 with HTML entities, CDATA, comments and a processing
instruction, a table (planned from the start tag's byte offset), UTF-16 with a byte order mark,
ISO-8859-1, an internal DTD subset with attribute defaults, and a paragraph longer than 32 KB.
Each chapter but the last is about a page, so the xml.txt scenario's turns reach all of them.

    python3 emu/make_xml_epub.py emu/books/xml
    emu/compare.sh emu/xml.txt x4pro emu/books/xml/xml-test.epub
    emu/compare.sh emu/open-book.txt x4pro emu/books/xml/broken/xml-broken.epub
    emu/compare.sh emu/open-book.txt x4pro emu/books/xml/entities/xml-entities.epub"""
import sys
import zipfile
from pathlib import Path

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)

XHTML11 = (
    '<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" '
    '"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">\n'
)


def page(title, body, doctype=XHTML11, decl='<?xml version="1.0" encoding="utf-8"?>\n'):
    return (
        f'{decl}{doctype}<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="tr"><head>'
        f"<title>{title}</title></head><body><h1>{title}</h1>\n{body}\n</body></html>\n"
    )


chapters = [
    (
        "Varlıklar",
        page(
            "Varl&#305;klar",
            "<!-- A comment the reader skips -->\r\n"
            '<style type="text/css"><![CDATA[ p { text-indent: 1em } ]]></style>\r\n'
            "<p>Fu&szlig;g&auml;nger&nbsp;yolu &mdash; &copy; 2024 &amp; &lt;etiket&gt; "
            "&quot;tırnak&quot; &#8220;akıllı&#8221; &#x2026; &bilinmeyen;</p>\r\n"
            "<?yer-imi ad=\"1\"?>\r\n"
            "<p>Satır bir<br/>\r\nSatır iki<br />\rSatır üç</p>\r\n"
            '<table border="1"><tr><th>Ad</th><td>Uzun bir açıklama hücresi</td></tr>'
            "<tr><td>7</td><td>kısa<br/>satırlar &amp; dahası</td></tr></table>"
            "<p>Tablodan sonra.</p>",
        ),
    ),
    (
        "UTF-16",
        page(
            "UTF-16 bölüm",
            "<p>Bu bölüm UTF-16 ile kodlandı: Şığçöü "
            "İı — \U0001F600 yok, ama Ç ve Ö var.</p>",
            decl='<?xml version="1.0" encoding="UTF-16"?>\n',
        ),
    ),
    (
        "Latin-1",
        page(
            "Latin-1 chapitre",
            "<p>Café, naïve, à la façon de Émile &ndash; « guillemets ».</p>",
            decl='<?xml version="1.0" encoding="ISO-8859-1"?>\n',
        ),
    ),
    (
        "DTD",
        page(
            "İç DTD",
            '<p class="x">Varsayılan öznitelikler ve &yazar; gibi bildirilmiş bir varlık.</p>',
            doctype=(
                '<!DOCTYPE html [<!ENTITY yazar "Yunus Emre">'
                '<!ATTLIST p dir CDATA "ltr" lang NMTOKEN " tr ">]>\n'
            ),
        ),
    ),
    (
        "Uzun",
        page(
            "Uzun paragraf",
            "<p>" + " ".join(f"kelime{i}" for i in range(4500)) + "</p>",
        ),
    ),
]

container = (
    '<?xml version="1.0"?>\r\n<!-- container -->\r\n'
    '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">\r\n'
    '  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>'
    "</rootfiles>\r\n</container>\r\n"
)

manifest = "".join(
    f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(len(chapters))
)
spine = "".join(f'<itemref idref="c{i}"/>' for i in range(len(chapters)))
opf = (
    '<?xml version="1.0" encoding="UTF-8"?>\n<!-- package -->\n'
    '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">'
    '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
    '<dc:identifier id="id">bembeyaz-xml</dc:identifier>'
    "<dc:title>XML &amp; <![CDATA[Ayrıştırıcı]]></dc:title>"
    "<dc:creator>Bembeyaz</dc:creator><dc:creator>&#304;kinci Yazar</dc:creator>"
    "<dc:language>tr</dc:language>"
    '<meta property="belongs-to-collection" id="s">Deneme Dizisi</meta>'
    '<meta refines="#s" property="collection-type">series</meta>'
    '<meta refines="#s" property="group-position">2</meta>'
    "</metadata>"
    '<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>'
    '<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>'
    f"{manifest}</manifest>"
    f'<spine toc="ncx">{spine}</spine></package>'
)
ncx_points = "".join(
    f'<navPoint id="n{i}" playOrder="{i + 1}"><navLabel><text>{i + 1}.&nbsp;{t}</text></navLabel>'
    f'<content src="c{i}.xhtml"/></navPoint>'
    for i, (t, _) in enumerate(chapters)
)
ncx = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd">\n'
    '<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head/>'
    f"<docTitle><text>XML</text></docTitle><navMap>{ncx_points}</navMap></ncx>"
)
nav_items = "".join(f'<li><a href="c{i}.xhtml">{t}</a></li>' for i, (t, _) in enumerate(chapters))
nav = (
    '<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE html>\n'
    '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>nav</title>'
    f'</head><body><nav epub:type="toc"><ol>{nav_items}</ol></nav></body></html>'
)

files = {
    "META-INF/container.xml": container.encode(),
    "OEBPS/content.opf": opf.encode(),
    "OEBPS/toc.ncx": ncx.encode(),
    "OEBPS/nav.xhtml": nav.encode(),
}
for i, (_, html) in enumerate(chapters):
    if "UTF-16" in html[:60]:
        data = b"\xff\xfe" + html.encode("utf-16-le")
    elif "ISO-8859-1" in html[:60]:
        data = html.encode("latin-1")
    else:
        data = html.encode()
    files[f"OEBPS/c{i}.xhtml"] = data



def write(name, files):
    path = out_dir / name
    with zipfile.ZipFile(path, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        for file, data in files.items():
            z.writestr(file, data, compress_type=zipfile.ZIP_DEFLATED)
    print(path)


write("xml-test.epub", files)

# A broken twin, for open-book.txt: its NCX ends early and its package document has junk after
# the root. (Its first chapter's HTML entity without a DTD no longer fails the chapter.)
broken = dict(files)
broken["OEBPS/c0.xhtml"] = page("Bozuk", "<p>a&nbsp;b</p>", doctype="<!DOCTYPE html>\n").encode()
broken["OEBPS/toc.ncx"] = ncx.encode()[: len(ncx) // 2]
broken["OEBPS/content.opf"] = (opf + "<junk/>").encode()
(out_dir / "broken").mkdir(exist_ok=True)
write("broken/xml-broken.epub", broken)

# An entities twin, for open-book.txt: its first chapter and its nav document use HTML entities
# without a DTD, which used to fail them.
entities = dict(files)
entities["OEBPS/c0.xhtml"] = page(
    "Varl&#305;klar",
    "<p>Fu&szlig;g&auml;nger&nbsp;yolu &mdash; &copy; 2024 &bogus; &amp; son.</p>",
    doctype="<!DOCTYPE html>\n",
).encode()
entities["OEBPS/nav.xhtml"] = nav.replace("</a>", "&nbsp;&ndash;&nbsp;son</a>").encode()
(out_dir / "entities").mkdir(exist_ok=True)
write("entities/xml-entities.epub", entities)
