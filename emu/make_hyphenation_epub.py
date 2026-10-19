"""Builds a German EPUB whose justified lines end in hyphenated words (rust/crates/bembeyaz-hyphenation):
long compounds broken by the patterns (and one of 69 letters, past the patterns' 68, that only the
fallback breaks), words joined by hyphens, soft hyphens and dashes, apostrophe contractions, a
footnote mark ([12], trimmed) and a one-digit one ([3], kept), and a long URL-like token that only
the fallback breaks. Also writes the settings file that turns hyphenation on, for the card.

    python3 emu/make_hyphenation_epub.py emu/books/hyphenation
    emu/compare.sh emu/reading.txt x4pro emu/books/hyphenation/hyphenation-test.epub \\
        emu/books/hyphenation/card/.crosspoint=/"""
import sys
import zipfile
from pathlib import Path

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)

css = "p { text-align: justify; text-indent: 1em; margin: 0 }\n"

chapters = [
    (
        "Komposita",
        """<p>Die Donaudampfschifffahrtsgesellschaft beschäftigte Kapitänswitwen, Rechtsschutzversicherungsgesellschaften
und Grundstücksverkehrsgenehmigungszuständigkeitsübertragungsverordnungen gleichermaßen. Der Quadratkilometer
Nahrungsmittelunverträglichkeit wurde vom Bundesverfassungsgericht als Arbeiterunfallversicherungsgesetz
anerkannt; Fußgängerüberwege und Straßenbahnhaltestellen blieben Verkehrsinfrastrukturfinanzierungsfragen.</p>
<p>Unabhängigkeitserklärungen, Geschwindigkeitsbegrenzungen und Lebensversicherungsunternehmen begegnen dem
Leser auf jeder Seite, damit die Zeilen an ihrem Ende Silbentrennung brauchen: Ver­ant­wor­tungs­be­wusst­sein
(mit weichen Trennzeichen), Haftpflichtversicherung, Schifffahrtsstraßen, Wahrscheinlichkeitsrechnung.</p>""",
    ),
    (
        "Bindestriche",
        """<p>Das US-Satellitensystem, die Max-Planck-Gesellschaft und der Baden-Württemberg-Tarif zeigen Wörter mit
Bindestrich; ein Gedankenstrich—ohne Leerzeichen—verbindet Satzteile. Geht's weiter? Wie geht's dem
Rhein-Main-Donau-Kanal-Betreiber? Fußnoten folgen Wörtern wie Bundesautobahnverwaltung[12] und
Selbstverständlichkeit[3].</p>
<p>Eine sehr lange Adresse ohne Trennstellen: https://example.org/verzeichnis/unterverzeichnis/dateimitlangemnamen.html
und die Zahl 1234567890123456789012345 enden hier.</p>""",
    ),
]


def xhtml(title, body):
    return f"""<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="de"><head><title>{title}</title>
<link rel="stylesheet" type="text/css" href="main.css"/></head><body><h1>{title}</h1>
{body}
</body></html>"""


manifest = "".join(f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(len(chapters)))
spine = "".join(f'<itemref idref="c{i}"/>' for i in range(len(chapters)))
toc = "".join(f'<li><a href="c{i}.xhtml">{t}</a></li>' for i, (t, _) in enumerate(chapters))
files = {
    "META-INF/container.xml": '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
    "OEBPS/content.opf": f'<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">bembeyaz-hyphenation</dc:identifier><dc:title>Silbentrennung</dc:title><dc:creator>Bembeyaz</dc:creator><dc:language>de</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="css" href="main.css" media-type="text/css"/>{manifest}</manifest><spine>{spine}</spine></package>',
    "OEBPS/nav.xhtml": f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>nav</title></head><body><nav epub:type="toc"><ol>{toc}</ol></nav></body></html>',
    "OEBPS/main.css": css,
}
for i, (title, body) in enumerate(chapters):
    files[f"OEBPS/c{i}.xhtml"] = xhtml(title, body)

path = out_dir / "hyphenation-test.epub"
with zipfile.ZipFile(path, "w") as z:
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    for name, data in files.items():
        z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)

settings = out_dir / "card" / ".crosspoint" / "settings.json"
settings.parent.mkdir(parents=True, exist_ok=True)
settings.write_text('{"hyphenationEnabled": 1}\n')
print(path)
