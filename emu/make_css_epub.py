"""Builds an EPUB whose look comes from its stylesheets (rust/crates/bembeyaz-css): alignment,
indents, margins and padding, bold and italic, small caps, underline, font sizes, borders and
shaded boxes, drop caps through ::first-letter and initial-letter, descendant and child
selectors, ids, compound classes, inline styles, hidden elements, lists and preformatted text.
Each chapter is about a page, so turning pages builds new sections from the CSS cache.

    python3 emu/make_css_epub.py emu/books/css
    emu/compare.sh emu/reading.txt x4pro emu/books/css/css-test.epub"""
import sys
import zipfile
from pathlib import Path

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)

main_css = """@charset "utf-8";
/* Body text */
p { text-indent: 1.5em; margin: 0 0 .4em 0; text-align: justify }
P.noindent, .first { text-indent: 0 }
h1 { font-size: 1.6em; text-align: center; margin-top: 2em; font-weight: bold }
h2.sub { font-variant: small-caps; text-align: right; font-size: 120% }
.note { font-style: italic; margin-left: 2em; margin-right: 2em }
.note.warn { font-weight: 700 !important; border: 2px solid #000; padding: .3em }
#motto { text-align: center; text-decoration: underline line-through }
div.poem > p { text-indent: 0; margin-left: 3em; text-align: left }
div.poem p.last { margin-bottom: 1.2em }
blockquote p { font-size: smaller; margin: 0 1em }
.box { background-color: #ddd; border-top: thick double; border-bottom-width: 3pt; border-bottom-style: dashed }
p.drop::first-letter { float: left; initial-letter: 3; font-weight: bold }
.hidden { display: none }
ul.plain { list-style-type: none }
pre { white-space: pre; margin-left: 1em }
sup.ref { vertical-align: super; font-size: 70% }
@media print { p { color: red } }
.unsupported + p, a[href], p:hover { font-weight: bold }
"""

extra_css = """
/* A second stylesheet: later rules of equal specificity win */
.note { margin-left: 1em }
section > .chapter-end { text-align: right; font-style: italic; page-break-after: always }
"""

chapters = [
    (
        "Bir — Hizalama",
        """<h2 class="sub">Stillerle düzen</h2>
<p class="drop first">Bu bölümün ilk harfi büyük ve üç satıra yayılıyor. Paragraf iki yana yaslı, girinti yok; ardından
gelenlerde 1.5em girinti var. Kitap yalnızca stil sayfasıyla biçimleniyor.</p>
<p>Sıradan bir paragraf: iki yana yaslanmış, ilk satırı girintili. Yazı tipi boyutu varsayılan, kenar boşlukları
sıfır; alt boşluk dörtte bir satırdan biraz fazla.</p>
<p class="note">Bir not: italik, iki yandan girintili. İkinci stil sayfası sol boşluğu 1em'e indiriyor.</p>
<p class="note warn">Uyarı notu: kalın (önemli), çerçeveli ve iç boşluklu.</p>
<p id="motto">Altı çizili ve üstü çizili ortalanmış söz.</p>
<p class="hidden">Bu satır görünmemeli.</p>
<p style="text-align: right; font-weight: bold; margin-top: 1em">Satır içi stil: sağa yaslı ve kalın.</p>""",
    ),
    (
        "İki — Şiir",
        """<div class="poem">
<p>Rüzgâr eser, dallar eğilir,</p>
<p>Gölgeler uzar, gün eğilir;</p>
<p class="last">Kim bilir yol nereye gider.<sup class="ref">1</sup></p>
</div>
<blockquote><p>Alıntı: biraz daha küçük, iki yandan girintili bir paragraf. Torun seçici blockquote p ile
eşleşiyor.</p></blockquote>
<div class="box"><p class="noindent">Gri zeminli kutu: üstte kalın çift çizgi, altta kesik çizgi.</p></div>
<ul class="plain"><li>İşaretsiz liste öğesi</li><li>Bir tane daha</li></ul>
<ul><li>İşaretli liste öğesi</li></ul>
<pre>önceden    biçimli
    metin   korunur</pre>""",
    ),
    (
        "Üç — Son",
        """<section><p class="noindent">Son bölüm kısa. Altında bir bitiş satırı var.</p>
<p class="chapter-end">— son —</p></section>
<p>Bu paragraf sayfa sonundan sonra gelir.</p>""",
    ),
]


def xhtml(title, body):
    return f"""<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title>
<link rel="stylesheet" type="text/css" href="main.css"/><link rel="stylesheet" type="text/css" href="extra.css"/>
</head><body><h1>{title}</h1>
{body}
</body></html>"""


manifest = "".join(f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(len(chapters)))
spine = "".join(f'<itemref idref="c{i}"/>' for i in range(len(chapters)))
toc = "".join(f'<li><a href="c{i}.xhtml">{t}</a></li>' for i, (t, _) in enumerate(chapters))
files = {
    "META-INF/container.xml": '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
    "OEBPS/content.opf": f'<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">bembeyaz-css</dc:identifier><dc:title>CSS Deneme</dc:title><dc:creator>Bembeyaz</dc:creator><dc:language>tr</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="main" href="main.css" media-type="text/css"/><item id="extra" href="extra.css" media-type="text/css"/>{manifest}</manifest><spine>{spine}</spine></package>',
    "OEBPS/nav.xhtml": f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>nav</title></head><body><nav epub:type="toc"><ol>{toc}</ol></nav></body></html>',
    "OEBPS/main.css": main_css,
    "OEBPS/extra.css": extra_css,
}
for i, (title, body) in enumerate(chapters):
    files[f"OEBPS/c{i}.xhtml"] = xhtml(title, body)

path = out_dir / "css-test.epub"
with zipfile.ZipFile(path, "w") as z:
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    for name, data in files.items():
        z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)
print(path)
