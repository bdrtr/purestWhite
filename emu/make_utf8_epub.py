"""Builds an EPUB that exercises lib/Utf8: Turkish text, NFD Latin and
Vietnamese marks, NFD Hangul, and an NFD title and file name."""
import sys, unicodedata, zipfile
from pathlib import Path

out_dir = Path(sys.argv[1])
nfd = lambda s: unicodedata.normalize("NFD", s)
title = nfd("Çiçekli Şiir — Tiếng Việt")
paragraphs = [
    "Türkçe: Ağaç gölgesinde çalışan işçi, ılık öğle güneşinde şarkı söylüyordu. İğde, ıhlamur, üzüm.",
    nfd("NFD Türkçe: Ağaç gölgesinde çalışan işçi, ılık öğle güneşinde şarkı söylüyordu."),
    nfd("NFD Tiếng Việt: Tôi yêu tiếng nước tôi từ khi mới ra đời, người ơi."),
    nfd("NFD Français: Élève, café, naïve, garçon, où, déjà vu, crème brûlée."),
    nfd("NFD 한국어: 한글은 아름다운 글자입니다."),
    "Combining without composition: q́ ẍ ́ leading mark.",
] * 6
body = "".join(f"<p>{p}</p>\n" for p in paragraphs)
files = {
    "META-INF/container.xml": '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
    "OEBPS/content.opf": f'<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">bembeyaz-utf8</dc:identifier><dc:title>{title}</dc:title><dc:creator>{nfd("Yazar Öğüt")}</dc:creator><dc:language>tr</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>',
    "OEBPS/nav.xhtml": f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>nav</title></head><body><nav epub:type="toc"><ol><li><a href="c1.xhtml">{nfd("Bölüm Bir — Đầu")}</a></li></ol></nav></body></html>',
    "OEBPS/c1.xhtml": f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c1</title></head><body><h1>{nfd("Bölüm Bir — Đầu")}</h1>\n{body}</body></html>',
}
path = out_dir / (nfd("Çiçekli-Şiir") + ".epub")
with zipfile.ZipFile(path, "w") as z:
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    for name, data in files.items():
        z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)
print(path)
