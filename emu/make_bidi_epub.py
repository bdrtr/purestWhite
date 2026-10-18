"""Builds an EPUB that exercises bidirectional text (rust/crates/bembeyaz-bidi): Arabic with
harakat and Lam-Alef, Farsi and Urdu letters, pointed Hebrew and the Tetragrammaton, numbers,
brackets and Latin words inside right-to-left paragraphs, and the reverse.

    python3 emu/make_bidi_epub.py emu/books/bidi
    emu/compare.sh emu/reading.txt x4pro emu/books/bidi/bidi-test.epub

The built-in reader fonts have no Arabic or Hebrew glyphs, and the card holds no SD font: the
body draws those letters as replacement boxes. A comparison then checks the line layout (word
order, numbers, brackets, Latin) and the Arabic chapter title in the UI font, not the shapes."""
import sys
import zipfile
from pathlib import Path

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)
title = "كتاب الاختبار — ספר"
rtl = [
    "السلام عليكم ورحمة الله وبركاته. قال الأستاذ: «لا تؤجل عمل اليوم إلى الغد».",
    "بِسْمِ اللَّهِ الرَّحْمَٰنِ الرَّحِيمِ — الْحَمْدُ لِلَّهِ رَبِّ الْعَالَمِينَ.",
    "في عام 2024 قرأت ١٢٣ كتابًا (منها 15 رواية) بسعر $12.50 للكتاب.",
    "برنامج CrossPoint Reader يعمل على جهاز Xteink X4 Pro بسرعة.",
    "فارسی: کتاب‌های پیچیده و گچ‌بری‌های زیبا در کوچه‌ی باغ.",
    "اردو: یہ کتاب بہت اچھی ہے، ٹھیک ہے اور ڈاکٹر نے کہا۔",
    "שָׁלוֹם עֲלֵיכֶם! בְּרֵאשִׁית בָּרָא אֱלֹהִים אֵת הַשָּׁמַיִם וְאֵת הָאָרֶץ.",
    "ברוך יהוה לעולם, אמן ואמן. קראתי 3 ספרים בשנת 2023 (ספר אחד בעברית).",
    "הוא אמר: \"Hello, world\" ואז הלך הביתה [בשעה 10:30].",
]
ltr = [
    "Mixed in a left-to-right paragraph: the word שלום means peace, and سلام too.",
    "Numbers next to Arabic: 2024 سنة and (القاهرة) in brackets; Hebrew ספר 12 pages.",
]
body = "".join(f'<p dir="rtl">{p}</p>\n' for p in rtl) + "".join(f"<p>{p}</p>\n" for p in ltr)
body = body * 4
files = {
    "META-INF/container.xml": '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
    "OEBPS/content.opf": f'<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">bembeyaz-bidi</dc:identifier><dc:title>{title}</dc:title><dc:creator>مؤلف</dc:creator><dc:language>ar</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine page-progression-direction="rtl"><itemref idref="c1"/></spine></package>',
    "OEBPS/nav.xhtml": '<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>nav</title></head><body><nav epub:type="toc"><ol><li><a href="c1.xhtml">الفصل الأول</a></li></ol></nav></body></html>',
    "OEBPS/c1.xhtml": f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" dir="rtl"><head><title>c1</title></head><body><h1>الفصل الأول — פרק</h1>\n{body}</body></html>',
}
path = out_dir / "bidi-test.epub"
with zipfile.ZipFile(path, "w") as z:
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    for name, data in files.items():
        z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)
print(path)
