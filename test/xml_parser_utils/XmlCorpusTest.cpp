// What the XML parser reports for a generated corpus: EPUB package files, NCX and XHTML, and
// documents probing the prolog, the internal DTD subset (entities, attribute defaults, element
// and notation declarations), content, attributes, character references, names, malformed UTF-8
// and the encodings the parser knows (UTF-8, UTF-16 either way, ISO-8859-1, US-ASCII), each
// mutated, truncated and re-encoded. Each document is parsed whole, then fed a byte at a time and
// seven bytes at a time with each text callback recorded: where expat's text splits depends on
// how its input is cut, and the chapter parser lays text out by callback. The reports of each
// group hash to a value recorded from expat 2.7.3 as the firmware built it (XML_GE=0, no XML_DTD)
// until the Rust parser replaced it; the error code is left out. A changed hash names the group;
// rerun with XML_CORPUS_PRINT=<group> to list its cases, and run the commit that added this test
// (still on expat) the same way to see expat's. The hashes freeze the corpus: add new cases as a
// new group, or to rust/crates/bembeyaz-xml/src/tests.rs.

#include <XmlParser.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

namespace {

struct Rng {
  uint64_t state;
  uint32_t next() {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    return static_cast<uint32_t>(state >> 32);
  }
  uint32_t below(const uint32_t n) { return next() % n; }
};

struct Hash {
  uint64_t value = 0xCBF29CE484222325ULL;  // FNV-1a
  void add(const std::string& s) {
    for (const char c : s) {
      value ^= static_cast<uint8_t>(c);
      value *= 0x100000001B3ULL;
    }
    value ^= 0xFF;
    value *= 0x100000001B3ULL;
  }
};

// One line per event, bytes outside printable ASCII escaped. Consecutive text is merged, or with
// `split` written as each callback brought it: the chapter parser lays text out by callback.
class Recorder {
 public:
  std::string out;
  bool split = false;

  void text(const char* s, const size_t len) {
    pending.append(s, len);
    if (split) flush();
  }
  void line(const char tag, const std::string& body) {
    flush();
    out += tag;
    escape(body);
    out += '\n';
  }
  void attribute(const char* name, const char* value) {
    out += 'A';
    escape(name);
    out += '=';
    escape(value);
    out += '\n';
  }
  void finish(const std::string& status) {
    flush();
    out += status;
    out += '\n';
  }

 private:
  std::string pending;

  void flush() {
    if (pending.empty()) return;
    out += 'T';
    escape(pending);
    out += '\n';
    pending.clear();
  }
  void escape(const std::string& s) {
    for (const char c : s) {
      const auto b = static_cast<uint8_t>(c);
      if (b < 0x20 || b >= 0x7F || c == '\\') {
        char hex[8];
        std::snprintf(hex, sizeof(hex), "\\%02X", b);
        out += hex;
      } else {
        out += c;
      }
    }
  }
};

// The parser under test: feeds `doc` in pieces of `chunk` bytes (0: whole) and records events.
std::string report(const std::string& doc, const size_t chunk, const bool split = false) {
  Recorder rec;
  rec.split = split;
  XmlParser parser;
  parser.create();
  parser.setUserData(&rec);
  parser.setElementHandler(
      [](void* r, const char* name, const char** atts) {
        auto* rec = static_cast<Recorder*>(r);
        rec->line('S', name);
        for (int i = 0; atts[i]; i += 2) rec->attribute(atts[i], atts[i + 1]);
      },
      [](void* r, const char* name) { static_cast<Recorder*>(r)->line('E', name); });
  parser.setCharacterDataHandler(
      [](void* r, const char* s, const int len) { static_cast<Recorder*>(r)->text(s, static_cast<size_t>(len)); });
  parser.setCommentHandler([](void* r, const char* data) { static_cast<Recorder*>(r)->line('C', data); });
  parser.setProcessingInstructionHandler([](void* r, const char* target, const char* data) {
    static_cast<Recorder*>(r)->line('P', std::string(target) + ' ' + data);
  });
  parser.setCdataSectionHandler([](void* r) { static_cast<Recorder*>(r)->line('[', ""); },
                                [](void* r) { static_cast<Recorder*>(r)->line(']', ""); });
  parser.setSkippedEntityHandler([](void* r, const char* name) { static_cast<Recorder*>(r)->line('K', name); });

  const size_t step = chunk == 0 ? doc.size() : chunk;
  size_t at = 0;
  bool ok = true;
  do {
    const size_t n = std::min(step, doc.size() - at);
    const bool last = at + n == doc.size();
    if (!parser.parse(doc.data() + at, n, last)) {
      rec.finish("error");
      ok = false;
      break;
    }
    at += n;
  } while (at < doc.size());
  if (ok) rec.finish("ok");
  return rec.out;
}

// UTF-16 code units as bytes, optionally after a byte order mark.
std::string utf16(const std::u16string& units, const bool bigEndian, const bool bom) {
  std::string out;
  if (bom) out = bigEndian ? "\xfe\xff" : "\xff\xfe";
  for (const char16_t u : units) {
    out += static_cast<char>(bigEndian ? u >> 8 : u & 0xFF);
    out += static_cast<char>(bigEndian ? u & 0xFF : u >> 8);
  }
  return out;
}

struct Group {
  const char* name;
  std::vector<std::string> seeds;
};

const std::vector<Group>& groups() {
  static const std::vector<Group> all = {
      {"documents",
       {
           R"(<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>)",
           R"(<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>Le Rouge et le Noir</dc:title>
    <dc:creator opf:role="aut" opf:file-as="Stendhal">Stendhal</dc:creator>
    <dc:language>fr</dc:language>
    <dc:identifier id="BookId" opf:scheme="ISBN">978-2-07-041237-8</dc:identifier>
    <meta name="cover" content="cover-image"/>
    <meta name="calibre:series" content="Classiques"/>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="cover-image" href="images/cover%20front.jpg" media-type="image/jpeg"/>
    <item id="css" href="../Styles/style.css" media-type="text/css"/>
    <item id="c1" href="Text/chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="c1"/><itemref idref="missing"/></spine>
  <guide><reference type="cover" title="Cover" href="Text/cover.xhtml"/></guide>
</package>)",
           R"(<?xml version='1.0' encoding='utf-8'?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" xml:lang="en" unique-identifier="uid" prefix="calibre: https://calibre-ebook.com">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:title id="t1">The Expanse &amp; Beyond</dc:title>
<meta refines="#t1" property="title-type">main</meta>
<meta property="belongs-to-collection" id="c01">The Expanse</meta>
<meta refines="#c01" property="collection-type">series</meta>
<meta refines="#c01" property="group-position">2.5</meta>
<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
</metadata>
<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav scripted"/>
<item id="img" href="cover.png" media-type="image/png" properties="cover-image"/></manifest>
<spine><itemref idref="nav" linear="no"/></spine>
</package>)",
           R"(<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd">
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
<head><meta name="dtb:uid" content="urn:uuid:1234"/><meta name="dtb:depth" content="2"/></head>
<docTitle><text>Kitap &#x2014; Başlık</text></docTitle>
<navMap>
<navPoint id="n1" playOrder="1"><navLabel><text>Bölüm 1</text></navLabel><content src="ch1.xhtml#s1"/>
<navPoint id="n2" playOrder="2"><navLabel><text>1.1&nbsp;Alt</text></navLabel><content src="ch1.xhtml#s2"/></navPoint>
</navPoint>
</navMap>
</ncx>)",
           R"(<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="tr">
<head><title>İçindekiler</title></head>
<body><nav epub:type="toc" id="toc"><h1>İçindekiler</h1><ol>
<li><a href="text/ch1.xhtml">Birinci</a><ol><li><a href="text/ch1.xhtml#a">Alt</a></li></ol></li>
<li><span>Başlıksız</span></li>
</ol></nav><nav epub:type="landmarks"><ol><li><a epub:type="bodymatter" href="text/ch1.xhtml">Start</a></li></ol></nav></body>
</html>)",
           R"(<?xml version="1.0" encoding="utf-8" standalone="no"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="de">
<head>
<title>Kapitel&nbsp;1</title>
<link rel="stylesheet" type="text/css" href="../Styles/style.css"/>
<style type="text/css"><![CDATA[ p > em { font-style: italic } ]]></style>
<script type="text/javascript">//<![CDATA[
if (a < b && c) {}
//]]></script>
</head>
<body class="c" id="top">
<!-- Kommentar -->
<h1 class="chapter" title="Erstes &quot;Kapitel&quot;">Erstes Kapitel</h1>
<p>Fu&szlig;g&auml;nger&#160;&#x2011;&#8212; &lt;tags&gt; &amp; &apos;quotes&apos; &quot;q&quot;.</p>
<p><img src="../Images/a.png" alt="A &amp; B"/><br/><sup><a href="#fn1" id="r1">1</a></sup></p>
<?page number="3"?>
<p>Zeile eins<br />
Zeile zwei</p>
</body>
</html>)",
           R"(<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Ohne&nbsp;Doctype &copy; 2024</p></body></html>)",
           R"(<?xml version="1.0" encoding="UTF-8"?>
<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
<enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/>
<enc:CipherData><enc:CipherReference URI="OEBPS/chapter1.xhtml"/></enc:CipherData></enc:EncryptedData>
</encryption>)",
           R"(<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:xlink="http://www.w3.org/1999/xlink">
<head><title>Cover</title></head>
<body><div><svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="100%" height="100%" viewBox="0 0 600 800" preserveAspectRatio="none">
<image width="600" height="800" xlink:href="../Images/cover.jpeg"/></svg></div></body></html>)",
           "<?xml version=\"1.0\"?>\r\n<package>\r\n\t<metadata>\r\n\t\t<title>CR\r\nLF</title>\r\n\t</metadata>"
           "\r\n</package>\r\n",
           "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html><body><table><tr><td>Hücre</td><td colspan=\"2\">"
           "\xe4\xb8\xad\xe6\x96\x87 \xf0\x9f\x98\x80</td></tr></table><ruby>\xe6\xbc\xa2<rt>kan</rt></ruby></body>"
           "</html>\n<!-- trailing -->\n",
       }},
      {"prolog",
       {
           "<a/>",
           "<?xml version=\"1.0\"?><a/>",
           "<?xml version='1.0' encoding='UTF-8' standalone='yes' ?><a/>",
           "<?xml version=\"1.0\" standalone=\"no\"?><a/>",
           "<?xml version=\"1.0\" standalone=\"maybe\"?><a/>",
           "<?xml encoding=\"UTF-8\" version=\"1.0\"?><a/>",
           "<?xml encoding=\"UTF-8\"?><a/>",
           "<?xml?><a/>",
           "<?xml version=\"1.0\"  encoding = \"UTF-8\" ?><a/>",
           "<?xml version=\"1.0\"encoding=\"UTF-8\"?><a/>",
           "<?xml version=\"1.0\" encoding=\"\"?><a/>",
           "<?xml version=\"1.0\" encoding=\"8bit\"?><a/>",
           "<?xml version=\"1.0\" encoding=\"UTF 8\"?><a/>",
           "<?xml version=\"1.0\" standalone=\"yes\" encoding=\"UTF-8\"?><a/>",
           "<?xml version=\"2.0\"?><a/>",
           "<?xml version=\"1.0\" foo=\"bar\"?><a/>",
           " <?xml version=\"1.0\"?><a/>",
           "\xef\xbb\xbf<?xml version=\"1.0\"?><a/>",
           "\xef\xbb\xbf\n<a/>",
           "\xef\xbb\xbf\xef\xbb\xbf<a/>",
           "\xef\xbb<a/>",
           "<?XML version=\"1.0\"?><a/>",
           "<?xml version=\"1.0\"?><?xml version=\"1.0\"?><a/>",
           "<?xml-stylesheet type=\"text/css\" href=\"a.css\"?><a/>",
           "<?xmlfoo?><a/><?xmlbar data?>",
           "<?pi?><?pi  data with ? and > ?><a/>",
           "<? pi?><a/>",
           "<?pi\x01?><a/>",
           "<!-- before --><a/><!-- after -->",
           "<!-- a -- b --><a/>",
           "<!--- x --><a/>",
           "<!-- x ---><a/>",
           "<!----><a/>",
           "text<a/>",
           "<!DOCTYPE a><a/>",
           "<!DOCTYPE a SYSTEM \"a.dtd\"><a/>",
           "<!DOCTYPE a SYSTEM 'a.dtd' ><a>&x;</a>",
           "<!DOCTYPE a PUBLIC \"-//X//DTD Y 1.0//EN\" \"a.dtd\"><a/>",
           "<!DOCTYPE a PUBLIC \"-//X//{bad}//EN\" \"a.dtd\"><a/>",
           "<!DOCTYPE a PUBLIC \"tab\there\" \"a.dtd\"><a/>",
           "<!DOCTYPE a PUBLIC \"x\"><a/>",
           "<!DOCTYPE a SYSTEM><a/>",
           "<!doctype a><a/>",
           "<!DOCTYPE><a/>",
           "<!DOCTYPE a><!DOCTYPE b><a/>",
           "<a/><!DOCTYPE a>",
           "<!DOCTYPE a\n  SYSTEM\n  \"x\"\n><a/>",
           "<!DOCTYPE a [ ]><a/>",
           "<!DOCTYPE a [ ] ><a/>",
           "<!DOCTYPE a []x><a/>",
           "",
           " \n\t ",
           "<!-- only -->",
           "<?xml version=\"1.0\"?>",
           "<a/>\r",
           "<a/>\r\n \n",
           "<a/><b/>",
           "<a/>text",
           "<a/>&amp;",
           "<a/><![CDATA[x]]>",
           "<a/>]]>",
           "<a></a><?pi after?> <!-- c --> ",
           "<![CDATA[x]]><a/>",
           "</a>",
           "<a/></a>",
       }},
      {"dtd",
       {
           "<!DOCTYPE a [<!ENTITY e \"value\">]><a>&e;|&e;</a>",
           "<!DOCTYPE a [<!ENTITY e 'v'><!ENTITY e 'w'>]><a x='&e;'>&e;</a>",
           "<!DOCTYPE a [<!ENTITY e SYSTEM \"e.xml\">]><a>&e;</a>",
           "<!DOCTYPE a [<!ENTITY e SYSTEM \"e.xml\">]><a x=\"&e;\"/>",
           "<!DOCTYPE a [<!ENTITY e PUBLIC \"-//P//EN\" \"e.xml\">]><a>&e;</a>",
           "<!DOCTYPE a [<!ENTITY e PUBLIC \"bad{\" \"e.xml\">]><a/>",
           "<!DOCTYPE a [<!NOTATION n SYSTEM \"n\"><!ENTITY e SYSTEM \"e\" NDATA n>]><a>&e;</a>",
           "<!DOCTYPE a [<!NOTATION n SYSTEM \"n\"><!ENTITY e SYSTEM \"e\" NDATA n>]><a x='&e;'/>",
           "<!DOCTYPE a [<!NOTATION n PUBLIC \"p\"><!NOTATION m PUBLIC \"p\" \"s\">]><a/>",
           "<!DOCTYPE a [<!ENTITY lt \"&#38;#60;\"><!ENTITY amp 'x'>]><a>&lt;&amp;</a>",
           "<!DOCTYPE a [<!ENTITY % p \"<!ENTITY e 'x'>\"> %p; ]><a>&e;</a>",
           "<!DOCTYPE a [ %undefined; ]><a>&e;</a>",
           "<!DOCTYPE a [<!ENTITY e %p;>]><a/>",
           "<!DOCTYPE a [<!ENTITY% p 'x'>]><a/>",
           "<!DOCTYPE a [<!ENTITY e \"&undefined; < & %pe;\">]><a>&e;</a>",
           "<!DOCTYPE a [<!ENTITY e \"\x01\">]><a/>",
           "<!DOCTYPE a [<!ENTITY e 'a\"b'>]><a>&e;</a>",
           "<!DOCTYPE a [<!ENTITY e 'v'x>]><a/>",
           "<!DOCTYPE a [<!ENTITY e>]><a/>",
           "<!DOCTYPE a [<!ATTLIST a x CDATA \"d\" y (p|q) \"p\" z ID #IMPLIED w NMTOKENS #REQUIRED v CDATA #FIXED "
           "\"f\">]><a w=\"  one   two  \" y=\" q \"/>",
           "<!DOCTYPE a [<!ATTLIST a x CDATA 'one'><!ATTLIST a x CDATA 'two' t NMTOKEN ' n  m '>]><a/><a x='3' t=' "
           "k '/>",
           "<!DOCTYPE a [<!ATTLIST a t NMTOKEN #IMPLIED><!ATTLIST a t CDATA 'c'>]><a t='  s  p  '/>",
           "<!DOCTYPE a [<!ATTLIST b x CDATA 'b'>]><a><b/><b x='own'/><c/></a>",
           "<!DOCTYPE a [<!ENTITY e 'v'><!ATTLIST a x CDATA \"&e;&#65;&lt;\">]><a/>",
           "<!DOCTYPE a [<!ATTLIST a x CDATA \"&undefined;\">]><a/>",
           "<!DOCTYPE a SYSTEM 'a.dtd' [<!ATTLIST a x CDATA \"&undefined;-\">]><a/>",
           "<!DOCTYPE a [<!ATTLIST a x CDATA \"a\tb\nc\r\nd\">]><a/>",
           "<!DOCTYPE a [<!ATTLIST a x ENTITY #IMPLIED y ENTITIES #IMPLIED z IDREF #IMPLIED zz IDREFS #REQUIRED>]><a "
           "x=' e ' zz=' i  j '/>",
           "<!DOCTYPE a [<!ATTLIST a x NOTATION (n|m) 'n'>]><a/>",
           "<!DOCTYPE a [<!ATTLIST a x (1|2|three) '1' y (a | b ) #IMPLIED>]><a y=' a '/>",
           "<!DOCTYPE a [<!ATTLIST a x BOGUS #IMPLIED>]><a/>",
           "<!DOCTYPE a [<!ATTLIST a x CDATA #DEFAULT 'd'>]><a/>",
           "<!DOCTYPE a [<!ATTLIST a x CDATA>]><a/>",
           "<!DOCTYPE a [<!ATTLIST a>]><a/>",
           "<!DOCTYPE a [<!ATTLIST a x ID 'i'>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a EMPTY><!ELEMENT b ANY><!ELEMENT c (#PCDATA)>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a (#PCDATA|b|c)*><!ELEMENT b (a,b?,(c|d)+)*><!ELEMENT c ((a|b),(c,d)*)?>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a (b,c|d)>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a (b|c,d)>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a (#PCDATA|b)>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a (#PCDATA)*>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a (b+|c*|d?)+>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a ((((b))))>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a (b)*x>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a (1b)>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a ()>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a (b) >]><a/>",
           "<!DOCTYPE a [<!ELEMENT a EMPTY x>]><a/>",
           "<!DOCTYPE a [<!ELEMENT a %p;>]><a/>",
           "<!DOCTYPE a [<!-- c --><?pi in subset?><!ELEMENT a ANY>]><a/>",
           "<!DOCTYPE a [<![INCLUDE[<!ELEMENT a ANY>]]>]><a/>",
           "<!DOCTYPE a [<!BOGUS a>]><a/>",
           "<!DOCTYPE a [<a/>]><a/>",
           "<!DOCTYPE a [<!ENTITY e 'x'>",
           "<!DOCTYPE a SYSTEM \"s\" [<!ENTITY e 'x'>]><a>&e;&f;</a>",
           "<?xml version='1.0' standalone='yes'?><!DOCTYPE a SYSTEM \"s\"><a>&f;</a>",
           "<?xml version='1.0' standalone='yes'?><!DOCTYPE a [<!ENTITY e 'x'>]><a>&e;</a>",
           "<?xml version='1.0' standalone='yes'?><!DOCTYPE a PUBLIC 'p' 's'><a x='&f;'/>",
           "<!DOCTYPE a PUBLIC 'p' 's'><a x='&f;' y='&lt;&f;&gt;'>&f;&g;</a>",
       }},
      {"content",
       {
           "<a><b><c/></b><b x='1' y=\"2\"></b></a>",
           "<a x = \"1\" \n\ty='2' ></a >",
           "<a></ a>",
           "<a><b></a></b>",
           "<a><b>",
           "<a>text",
           "<a></b>",
           "<a/><a/>",
           "<a>]]></a>",
           "<a>]]</a>",
           "<a>]</a>",
           "<a>x]",
           "<a>a\r\nb\rc\nd\r\r\ne</a>",
           "<a>\r",
           "<a>&#65;&#x42;&#X43;</a>",
           "<a>&#0;</a>",
           "<a>&#xD800;</a>",
           "<a>&#xFFFE;&#xFFFF;</a>",
           "<a>&#x110000;</a>",
           "<a>&#x10FFFF;&#1114111;&#9;&#10;&#13;&#x20;</a>",
           "<a>&#00000000000000000065;</a>",
           "<a>&#99999999999999999999;</a>",
           "<a>&#;&#x;</a>",
           "<a>&#12a;</a>",
           "<a>& ;</a>",
           "<a>&a b;</a>",
           "<a>&amp</a>",
           "<a>&lt;&gt;&amp;&apos;&quot;</a>",
           "<a>&LT;</a>",
           "<a>&nbsp;</a>",
           "<a><![CDATA[<&>]]></a>",
           "<a><![CDATA[a\r\nb\rc]]]]></a>",
           "<a><![CDATA[]]></a>",
           "<a><![CDATA[x",
           "<a><![cdata[x]]></a>",
           "<a><![CDATA[\x01]]></a>",
           "<a><!----><!-- - --></a>",
           "<a><!-- -- --></a>",
           "<a><!---></a>",
           "<a><!-- ---></a>",
           "<a><!-- x\r\ny --></a>",
           "<a><?xml version='1.0'?></a>",
           "<a><?Xml?></a>",
           "<a><?pi data?><?pi?><?pi  ?><?pi\r\nx?></a>",
           "<a><?pi x</a>",
           "<a x='1' x='2'/>",
           "<a x='1'y='2'/>",
           "<a x/>",
           "<a x=1/>",
           "<a x='<'/>",
           "<a x='&'/>",
           "<a x='a\tb\nc\r\nd\re'/>",
           "<a x='&#10;&#9;&#13;&#32;'/>",
           "<a x='a&#32;&#32;b' y=\"'\" z='\"'/>",
           "<a x='&lt;&amp;&gt;&quot;&apos;'/>",
           "<a x='>'/>",
           "<a x='&#0;'/>",
           "<a x='&#0;' x='2'/>",
           "<a x='&undefined;'/>",
           "<a x='1' / >",
           "<a x='1'/ >",
           "<a/ >",
           "<a\n/>",
           "<a:b c:d='1' xmlns:a='urn:a'><a:c/></a:b>",
           "<:a :b='1'/>",
           "<a.b-c_d e.f-g_h='1'/>",
           "<1a/>",
           "<-a/>",
           "<a 1b='1'/>",
           "<\xc3\xa9l\xc3\xa9ment \xc3\xa9t\xc3\xa9='1'>\xc3\xa9</\xc3\xa9l\xc3\xa9ment>",
           "<\xc2\xaa\xc2\xb5\xc2\xba/>",
           "<a\xc2\xb7\xcc\x81/>",
           "<\xc2\xb7/>",
           "<\xf0\x90\x80\x80/>",
           "<a\xf0\x90\x80\x80/>",
           "<\xe4\xb8\xad\xe6\x96\x87 \xe5\xb1\x9e='\xe5\x80\xbc'/>",
           "<a\xe2\x80\x8c/>",
           "<a><b/>text<c>more</c>tail</a>",
           "<a>   </a>",
           "<a>\t\n</a>",
       }},
      {"chars",
       {
           "<a>\xc0\x80</a>",
           "<a>\xc1\xbf</a>",
           "<a>\xc2\x80\xdf\xbf</a>",
           "<a>\xe0\x80\x80</a>",
           "<a>\xe0\xa0\x80\xef\xbf\xbd</a>",
           "<a>\xed\xa0\x80</a>",
           "<a>\xed\x9f\xbf</a>",
           "<a>\xef\xbf\xbe</a>",
           "<a>\xef\xbf\xbf</a>",
           "<a>\xf0\x80\x80\x80</a>",
           "<a>\xf0\x90\x80\x80\xf4\x8f\xbf\xbf</a>",
           "<a>\xf4\x90\x80\x80</a>",
           "<a>\xf5\x80\x80\x80</a>",
           "<a>\xf8\x88\x80\x80\x80</a>",
           "<a>\xfe\xff</a>",
           "<a>\x80</a>",
           "<a>\xbf</a>",
           "<a>x\xc3</a>",
           "<a>x\xe2\x82</a>",
           "<a>\xc3x</a>",
           "<a>\x01</a>",
           "<a>\x0b\x0c</a>",
           "<a>\x7f</a>",
           "<a>a\x00b</a>",
           "<a x='\x01'/>",
           "<a x='\xc3'/>",
           "<a x='\xef\xbf\xbe'/>",
           "<a\x01/>",
           "<a><!-- \xff --></a>",
           "<a><?pi \xc0\xaf?></a>",
           "<a/>\xc3\xa9",
           "\xc3\xa9<a/>",
           "<a>\xe2\x80\xa8\xc2\x85</a>",
       }},
      {"encodings",
       {
           utf16(u"<a>\xe9</a>", false, true),
           utf16(u"<a>\xe9</a>", true, true),
           utf16(u"<a/>", false, false),
           utf16(u"<a/>", true, false),
           utf16(u"<?xml version='1.0' encoding='UTF-16'?><a/>", false, true),
           utf16(u"<?xml version='1.0' encoding='UTF-16'?><a/>", true, false),
           utf16(u"<?xml version='1.0' encoding='UTF-8'?><a/>", false, true),
           utf16(u"<?xml version='1.0' encoding='UTF-16BE'?><a/>", false, true),
           utf16(u"<?xml version='1.0' encoding='UTF-16BE'?><a/>", true, true),
           utf16(u"<?xml version='1.0' encoding='ISO-8859-1'?><a/>", true, true),
           utf16(u"<a>\xd83d\xde00</a>", false, true),
           utf16(u"<a>\xd83dx</a>", false, true),
           utf16(u"<a>\xd83d<b/></a>", true, true),
           utf16(u"<a>\xdc00</a>", false, true),
           utf16(u"<a>\xfffe</a>", false, true),
           utf16(u"<a>\xfeff</a>", true, true),
           utf16(u"<\xaa\xb5\xba/>", false, true),
           utf16(u"<\xb7/>", false, true),
           utf16(u"<\xd800\xdc00/>", false, true),
           utf16(u"<\x4e2d \x5c5e='\x503c'>\x6587</\x4e2d>", true, true),
           utf16(u"<a>\r\n\r</a>\r", false, true),
           utf16(u"<a/>", false, true) + "x",
           utf16(u"<a>\xd83d", false, true),
           utf16(std::u16string(u"<a>\0</a>", 8), false, true),
           "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a x='\xe9'>caf\xe9 \x80\x9f\xff</a>",
           "<?xml version=\"1.0\" encoding=\"iso-8859-1\"?><\xe9\xaa\xb5\xba\xb7 \xc0='1'/>",
           "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><\xd7/>",
           "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>\xc3\xa9</a>",
           "<?xml version=\"1.0\" encoding=\"latin1\"?><a/>",
           "<?xml version=\"1.0\" encoding=\"US-ASCII\"?><a>plain</a>",
           "<?xml version=\"1.0\" encoding=\"US-ASCII\"?><a>caf\xe9</a>",
           "<?xml version=\"1.0\" encoding=\"windows-1252\"?><a/>",
           "<?xml version=\"1.0\" encoding=\"utf-8\"?><a>\xc3\xa9</a>",
           "<?xml version=\"1.0\" encoding=\"UTF-16\"?><a/>",
           "<?xml version=\"1.0\" encoding=\"UTF-16LE\"?><a/>",
           "\xef\xbb\xbf<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>\xe9</a>",
           "<?xml version=\"1.0\" encoding=\"a_b.c-d\"?><a/>",
           "<?xml version=\"1.0\" encoding=\"A0123456789012345678901234567890123456789012345678901234567890123456789012"
           "3456789012345678901234567890123456789012345678901234567890\"?><a/>",
           "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>\r\n\x85</a>",
       }},
  };
  return all;
}

// Re-encodes UTF-8 text as UTF-16; false when the text is not valid UTF-8.
bool toUtf16(const std::string& in, const bool bigEndian, std::string& out) {
  out.clear();
  for (size_t i = 0; i < in.size();) {
    const auto b = static_cast<uint8_t>(in[i]);
    uint32_t cp;
    size_t n;
    if (b < 0x80) {
      cp = b;
      n = 1;
    } else if (b >= 0xC2 && b < 0xE0) {
      cp = b & 0x1F;
      n = 2;
    } else if (b >= 0xE0 && b < 0xF0) {
      cp = b & 0x0F;
      n = 3;
    } else if (b >= 0xF0 && b < 0xF5) {
      cp = b & 0x07;
      n = 4;
    } else {
      return false;
    }
    if (i + n > in.size()) return false;
    for (size_t k = 1; k < n; ++k) {
      const auto c = static_cast<uint8_t>(in[i + k]);
      if ((c & 0xC0) != 0x80) return false;
      cp = (cp << 6) | (c & 0x3F);
    }
    if ((n == 3 && cp < 0x800) || (n == 4 && (cp < 0x10000 || cp > 0x10FFFF)) || (cp >= 0xD800 && cp < 0xE000)) {
      return false;
    }
    i += n;
    auto unit = [&](const uint32_t u) {
      out += static_cast<char>(bigEndian ? u >> 8 : u & 0xFF);
      out += static_cast<char>(bigEndian ? u & 0xFF : u >> 8);
    };
    if (cp >= 0x10000) {
      unit(0xD800 | ((cp - 0x10000) >> 10));
      unit(0xDC00 | ((cp - 0x10000) & 0x3FF));
    } else {
      unit(cp);
    }
  }
  return true;
}

// The seed as a UTF-16 document, declaring UTF-16 where it declared UTF-8.
std::vector<std::string> utf16Variants(const std::string& seed) {
  std::string text = seed;
  for (const char* utf8 : {"UTF-8", "utf-8"}) {
    const size_t at = text.find(utf8);
    if (at != std::string::npos && at < 64) text.replace(at, 5, "UTF-16");
  }
  std::vector<std::string> out;
  for (const bool big : {false, true}) {
    std::string body;
    if (!toUtf16(text, big, body)) return {};
    out.push_back((big ? std::string("\xfe\xff") : std::string("\xff\xfe")) + body);
    out.push_back(body);
  }
  return out;
}

const char* const kSnippets[] = {
    "&amp;",
    "&#x41;",
    "&#0;",
    "&nbsp;",
    "<!--c-->",
    "<?pi x?>",
    "<![CDATA[x]]>",
    "]]>",
    "\r\n",
    "\r",
    "<a>",
    "</a>",
    "<b x='1'/>",
    "&#xD800;",
    "\xc2\xa0",
    "&e;",
    "%p;",
    "<!ENTITY e 'v'>",
    "<!ATTLIST a x CDATA 'd'>",
    "\xef\xbb\xbf",
    "\xc3",
    "\xe2\x82",
    "\xf0\x9f\x98\x80",
    "'",
    "\"",
    "=",
    " x='y'",
    "<!DOCTYPE a>",
    "<?xml version='1.0'?>",
    "&#x10FFFF;",
    "&lt;",
    "\t",
    "\x00",
    "\xff",
    "]",
};

const char kBytes[] = {'<',    '>',    '&',    ';',    '"',    '\'',   '=',    '/',    '!',    '?',    '-',   ']',
                       '[',    '%',    '#',    ' ',    '\t',   '\r',   '\n',   '\0',   'x',    'A',    ':',   '.',
                       '\x80', '\xbf', '\xc0', '\xc3', '\xe2', '\xed', '\xef', '\xf0', '\xf4', '\xf5', '\xff'};

std::string mutate(std::string doc, Rng& rng) {
  const uint32_t edits = 1 + rng.below(3);
  for (uint32_t e = 0; e < edits; ++e) {
    const size_t at = doc.empty() ? 0 : rng.below(static_cast<uint32_t>(doc.size() + 1));
    switch (rng.below(5)) {
      case 0:  // replace a byte
        if (at < doc.size()) doc[at] = kBytes[rng.below(sizeof(kBytes))];
        break;
      case 1: {  // insert a snippet
        const char* snippet = kSnippets[rng.below(sizeof(kSnippets) / sizeof(kSnippets[0]))];
        doc.insert(at, snippet, *snippet ? std::strlen(snippet) : 1);
        break;
      }
      case 2:  // delete a run
        if (at < doc.size()) doc.erase(at, 1 + rng.below(8));
        break;
      case 3:  // duplicate a run
        if (at < doc.size()) doc.insert(at, doc.substr(at, 1 + rng.below(16)));
        break;
      default:  // insert a byte
        doc.insert(doc.begin() + static_cast<std::ptrdiff_t>(at), kBytes[rng.below(sizeof(kBytes))]);
        break;
    }
  }
  return doc;
}

// Every document of a group: each seed and its UTF-16 forms, mutated and truncated.
std::vector<std::string> documents(const Group& group, const size_t groupIndex) {
  std::vector<std::string> docs;
  for (size_t s = 0; s < group.seeds.size(); ++s) {
    Rng rng{0x9E3779B97F4A7C15ULL ^ (groupIndex << 32) ^ (s * 0x2545F4914F6CDD1DULL)};
    std::vector<std::pair<std::string, int>> variants{{group.seeds[s], 24}};
    for (const auto& v : utf16Variants(group.seeds[s])) variants.emplace_back(v, 4);
    for (const auto& [doc, mutations] : variants) {
      docs.push_back(doc);
      for (int m = 0; m < mutations; ++m) docs.push_back(mutate(doc, rng));
      for (size_t cut = 1; cut < 8 && doc.size() > 1; ++cut) docs.push_back(doc.substr(0, doc.size() * cut / 8));
    }
  }
  return docs;
}

std::string show(const std::string& doc) {
  std::string shown;
  for (const char c : doc) {
    const auto b = static_cast<uint8_t>(c);
    if (b < 0x20 || b >= 0x7F || c == '\\') {
      char hex[8];
      std::snprintf(hex, sizeof(hex), "\\x%02X", b);
      shown += hex;
    } else {
      shown += c;
    }
  }
  return shown;
}

// Recorded from expat 2.7.3 with XML_GE=0 and XML_CONTEXT_BYTES=1024, as lib/expat built.
const uint64_t kHashes[] = {
    0xA92741C56582CF64ULL,  // documents
    0xD39DCDFFE418AFAFULL,  // prolog
    0xCA85755295B8893AULL,  // dtd
    0x247C181CB5FC8752ULL,  // content
    0xEB7AC235B5E39A68ULL,  // chars
    0x63A237FB34CB6D5BULL,  // encodings
};

// The same, fed a byte at a time and seven bytes at a time, each text callback on its own.
const uint64_t kSplitHashes[] = {
    0x8023E9B76ED494C1ULL,  // documents
    0x02F3A330958510ADULL,  // prolog
    0x29E7B408AF3AB0B5ULL,  // dtd
    0x0E7883D4F4AC2CF6ULL,  // content
    0x1915B4B50550B321ULL,  // chars
    0x18345F8F926BB8ADULL,  // encodings
};

}  // namespace

TEST(XmlCorpus, ReportsMatchTheRecordedParser) {
  const auto& all = groups();
  ASSERT_EQ(all.size(), sizeof(kHashes) / sizeof(kHashes[0]));
  const char* print = std::getenv("XML_CORPUS_PRINT");
  size_t total = 0;
  for (size_t g = 0; g < all.size(); ++g) {
    Hash hash;
    Hash splitHash;
    const auto docs = documents(all[g], g);
    for (const auto& doc : docs) {
      const std::string whole = report(doc, 0);
      hash.add(doc);
      hash.add(whole);
      splitHash.add(report(doc, 1, true));
      splitHash.add(report(doc, 7, true));
      if (print && std::strcmp(print, all[g].name) == 0) std::printf("--- %s\n%s", show(doc).c_str(), whole.c_str());
    }
    total += docs.size();
    EXPECT_EQ(hash.value, kHashes[g]) << all[g].name << ": 0x" << std::hex << std::uppercase << hash.value << "ULL";
    EXPECT_EQ(splitHash.value, kSplitHashes[g])
        << all[g].name << " in pieces: 0x" << std::hex << std::uppercase << splitHash.value << "ULL";
  }
  EXPECT_GT(total, 10000u);
}
