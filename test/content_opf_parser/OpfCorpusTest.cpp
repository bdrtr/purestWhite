// What ContentOpfParser reports for a generated corpus of package documents: metadata (titles,
// creators, languages, identifiers, cover and calibre metas, EPUB 3 collections), manifests
// (ids, hrefs, media types, properties), spines and guides, in order and out of it, nested,
// repeated and missing, then mutated and truncated. Each document is read in each of the
// parser's modes, whole and in pieces, the way ZipFile streams it (stopping at the first short
// write). The reports of each mode hash to a value recorded from the C++ parser before the Rust
// port. A changed hash names the mode; rerun with OPF_CORPUS_PRINT=<mode> to list its cases, and
// run the commit that added this test the same way to see the C++ parser's. The hashes freeze the
// corpus: add new cases as a new mode or test.

#include <gtest/gtest.h>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include "ContentOpfParser.h"
#include "Epub/BookMetadataCache.h"

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
  bool chance(const uint32_t percent) { return below(100) < percent; }
  template <size_t N>
  const char* pick(const char* const (&items)[N]) {
    return items[below(N)];
  }
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

std::string escaped(const std::string& s) {
  std::string out;
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
  return out;
}

// --- The generator ---

const char* const kPrefixes[] = {"", "", "", "dc:", "opf:", "x:", "a:b:"};
const char* const kWords[] = {"The",     "Left",   "Hand",      "of",        "Darkness", "\xC3\x89mile",
                              "Zola",    "Ka\xC4\xB1nat", "&#201;", "&amp;",     "&lt;",    "&#x130;",
                              "&#32;",   "x",      "<![CDATA[ c d ]]>", "<!--n-->", "<?p d?>", "<b>bold</b>",
                              "<i/>",    "-",      ",",         "'",         "\""};
const char* const kSpaces[] = {" ", " ", "  ", "\t", "\n", "\r\n", " \n\t ", "&#9;", "&#10;", "&#160;"};
const char* const kSchemes[] = {"ISBN", "isbn", "ASIN", "asin", "MOBI-ASIN", "amazon", "Amazon",
                                "uuid", "",     "xisbnx", "AMAZON"};
const char* const kIdentifiers[] = {"978-1-4028-9462-6", "isbn:9781402894626", "ISBN 978 1", "urn:isbn:978-1",
                                    "URN:ISBN::\t 12",   "asin:B0DTT5LV77",    "urn:asin:B0",  "ASIN: B0",
                                    "B0DTT5LV77",        "uuid:1234",          "isbn",         "urn:isbn",
                                    "isbnx",             ""};
const char* const kNumbers[] = {"2",   "1.5", " 3",   "3 ",   "-0",   "1e3", "0x1p3", "inf", "nan", "1e40",
                                "abc", "",    "2.50", "+4",   "007",  ".5",  "5.",    "1,5", "\xC2\xB2", "1e-50"};
const char* const kMetaNames[] = {"cover",          "COVER",          "calibre:series",       "Calibre:Series",
                                  "calibre:series", "calibre:series", "calibre:series_index", "CALIBRE:SERIES_INDEX",
                                  "calibre:series_index", "calibre:series_index", "generator", "covers", ""};
const char* const kProperties[] = {"belongs-to-collection", "collection-type", "group-position", "BELONGS-TO-COLLECTION",
                                   "Collection-Type", "group-position", "dcterms:modified", "title-type", ""};
const char* const kCollectionTypes[] = {"series", "SERIES", " series", "set", "Series"};
const char* const kIds[] = {"a", "b", "c", "cover", "nav", "ncx", "id1", "x", "", "dup", "s", "c1", "Cover"};
const char* const kHrefs[] = {"text/ch1.xhtml", "ch%201.xhtml", "../up.xhtml", "./dot.xhtml", "a//b.xhtml",
                              "/abs.xhtml",     "",             "%zz.xhtml",   "%41%2F.x",    "..",
                              "a/../../b.html", "caf\xC3\xA9.xhtml", "img/c.jpg",  "s.css",       "toc.ncx",
                              "nav.xhtml",      "a/./b/../c",   "%2e%2e/x",    "q?x=1#f",     "%"};
const char* const kMediaTypes[] = {"application/xhtml+xml", "application/x-dtbncx+xml", "APPLICATION/X-DTBNCX+XML",
                                   "text/css", "TEXT/CSS", "image/jpeg", "IMAGE/png", "image", "images/x", "image/",
                                   "", "application/x-dtbncx+xml "};
const char* const kItemProperties[] = {"nav", "cover-image", "nav cover-image", "cover-image nav", "scripted nav",
                                       "navigation", "nav2", "x cover-image", "cover-imagex", "svg", "", " nav",
                                       "NAV", "mathml cover-image svg"};
const char* const kGuideTypes[] = {"start", "text", "cover", "cover-page", "toc", "START", "", "Cover"};
const char* const kNoise[] = {"<!--c-->", "<?pi x?>", "<x/>", "<other a='1'>t</other>", "\n  ", "<y><z/></y>"};

class Generator {
 public:
  explicit Generator(Rng& rng) : rng(rng) {}

  std::string document() {
    std::string doc;
    if (rng.chance(40)) doc += "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
    if (rng.chance(5)) doc += "<!DOCTYPE package [<!ENTITY e 'v'><!ATTLIST item media-type CDATA 'text/css'>]>";
    const std::string pkg = std::string(rng.chance(85) ? "" : rng.pick(kPrefixes)) + (rng.chance(95) ? "package" : "pack");
    doc += "<" + pkg + " xmlns=\"http://www.idpf.org/2007/opf\" xmlns:dc=\"urn:dc\" version=\"3.0\">";
    const uint32_t sections = rng.chance(10) ? rng.below(7) : 4;
    const char* const order[] = {"metadata", "manifest", "spine", "guide"};
    for (uint32_t i = 0; i < sections; i++) {
      // Mostly in the usual order; sometimes shuffled, repeated or wrapped.
      const char* section = rng.chance(85) && i < 4 ? order[i] : order[rng.below(4)];
      if (i == 3 && rng.chance(40)) continue;  // most books have no guide
      const bool wrapped = rng.chance(4);
      if (wrapped) doc += "<wrap>";
      if (std::strcmp(section, "metadata") == 0) doc += metadata();
      if (std::strcmp(section, "manifest") == 0) doc += manifest();
      if (std::strcmp(section, "spine") == 0) doc += spine();
      if (std::strcmp(section, "guide") == 0) doc += guide();
      if (wrapped) doc += "</wrap>";
      if (rng.chance(10)) doc += rng.pick(kNoise);
    }
    doc += "</" + pkg + ">";
    if (rng.chance(3)) doc += "<junk/>";
    return doc;
  }

 private:
  Rng& rng;
  std::vector<std::string> ids;

  std::string name(const char* local) {
    return std::string(rng.chance(50) ? rng.pick(kPrefixes) : "") + local;
  }

  std::string text(const uint32_t maxWords = 6) {
    std::string out;
    if (rng.chance(30)) out += rng.pick(kSpaces);
    const uint32_t words = rng.chance(5) ? 0 : 1 + rng.below(maxWords);
    for (uint32_t w = 0; w < words; w++) {
      if (w > 0) out += rng.pick(kSpaces);
      out += rng.pick(kWords);
    }
    if (rng.chance(30)) out += rng.pick(kSpaces);
    if (rng.chance(2)) out += std::string(500 + rng.below(100), 'L') + "&#305;" + std::string(20, 'm');
    return out;
  }

  std::string element(const char* local, const std::string& attrs, const std::string& body) {
    const std::string n = name(local);
    if (body.empty() && rng.chance(30)) return "<" + n + attrs + "/>";
    return "<" + n + attrs + ">" + body + "</" + n + ">";
  }

  static std::string attr(const std::string& n, const std::string& value) { return " " + n + "=\"" + value + "\""; }

  std::string metadata() {
    std::string body;
    const uint32_t count = rng.below(12);
    for (uint32_t i = 0; i < count; i++) {
      switch (rng.below(8)) {
        case 0:
          body += element("title", "", rng.chance(10) ? "" : text());
          break;
        case 1:
          body += element("creator", rng.chance(20) ? attr("opf:role", "aut") : "", text(4));
          break;
        case 2:
          body += element("language", "", rng.chance(80) ? "en" : text(2));
          break;
        case 3: {
          std::string attrs;
          if (rng.chance(10)) {
            attrs += attr("scheme", rng.pick(kSchemes)) + attr("b:scheme", rng.pick(kSchemes));
          } else if (rng.chance(60)) {
            attrs += attr(rng.chance(50) ? "opf:scheme" : "scheme", rng.pick(kSchemes));
          }
          body += element("identifier", attrs, rng.pick(kIdentifiers) + std::string(rng.chance(20) ? " " : ""));
          break;
        }
        case 4: {
          std::string attrs = attr(rng.chance(80) ? "name" : "opf:name", rng.pick(kMetaNames));
          if (rng.chance(90)) {
            const bool number = rng.chance(50);
            attrs += attr("content", number ? rng.pick(kNumbers) : rng.chance(50) ? rng.pick(kIds) : "Earthsea");
          }
          body += "<" + name("meta") + attrs + "/>";
          break;
        }
        case 5:
        case 6: {
          const std::string id = rng.chance(70) ? std::string("s") + std::to_string(rng.below(3))
                                                : std::string("x") + std::to_string(rng.below(12));
          const char* property = rng.pick(kProperties);
          std::string attrs = attr("property", property);
          std::string value;
          if (std::strcmp(property, "belongs-to-collection") == 0 || rng.chance(20)) {
            attrs += attr("id", id);
            value = rng.chance(10) ? "" : text(3);
          } else {
            attrs += attr("refines", (rng.chance(85) ? "#" : "") + id);
            value = rng.chance(50) ? rng.pick(kCollectionTypes) : rng.pick(kNumbers);
          }
          if (rng.chance(5)) attrs += attr("name", rng.pick(kMetaNames)) + attr("content", rng.pick(kNumbers));
          body += element("meta", attrs, value);
          break;
        }
        default:
          body += rng.chance(50) ? rng.pick(kNoise) : element("subject", "", text(2));
          break;
      }
      if (rng.chance(20)) body += rng.pick(kSpaces);
    }
    if (rng.chance(5)) body += "<manifest/>";  // ends a metadata-only read early
    return element("metadata", rng.chance(20) ? attr("xmlns:opf", "urn:opf") : "", body);
  }

  std::string itemId() {
    if (!ids.empty() && rng.chance(40)) return ids[rng.below(static_cast<uint32_t>(ids.size()))];
    if (rng.chance(50)) return rng.pick(kIds);
    return "i" + std::to_string(rng.below(80));
  }

  std::string manifest() {
    std::string body;
    const uint32_t count = rng.chance(15) ? 18 + rng.below(60) : rng.below(10);
    for (uint32_t i = 0; i < count; i++) {
      std::string attrs;
      const std::string id = itemId();
      ids.push_back(id);
      if (rng.chance(95)) attrs += attr(rng.chance(95) ? "id" : rng.chance(50) ? "opf:id" : "xml:id", id);
      if (rng.chance(97)) attrs += attr(rng.chance(97) ? "href" : "opf:href", rng.pick(kHrefs));
      if (rng.chance(90)) attrs += attr("media-type", rng.pick(kMediaTypes));
      if (rng.chance(25)) attrs += attr("properties", rng.pick(kItemProperties));
      const std::string item = name("item");
      body += "<" + item + attrs + (rng.chance(10) ? "></" + item + ">" : "/>");
      if (rng.chance(5)) body += rng.pick(kNoise);
    }
    return element("manifest", "", body);
  }

  std::string spine() {
    std::string body;
    const uint32_t count = rng.below(12);
    for (uint32_t i = 0; i < count; i++) {
      std::string attrs;
      if (rng.chance(95)) attrs += attr(rng.chance(95) ? "idref" : "opf:idref", rng.chance(85) ? itemId() : "missing");
      if (rng.chance(20)) attrs += attr("linear", rng.chance(50) ? "no" : "yes");
      body += "<" + name("itemref") + attrs + "/>";
      if (rng.chance(5)) body += rng.pick(kNoise);
    }
    return element("spine", rng.chance(70) ? attr("toc", "ncx") : "", body);
  }

  std::string guide() {
    std::string body;
    const uint32_t count = rng.below(6);
    for (uint32_t i = 0; i < count; i++) {
      std::string attrs;
      if (rng.chance(95)) attrs += attr("type", rng.pick(kGuideTypes));
      if (rng.chance(30)) attrs += attr("title", "T");
      if (rng.chance(95)) attrs += attr(rng.chance(95) ? "href" : "opf:href", rng.pick(kHrefs));
      body += "<" + name("reference") + attrs + "/>";
    }
    return element("guide", "", body);
  }
};

const char* const kSnippets[] = {"&amp;", "&#0;",       "&nbsp;", "<!--c-->", "<x>",    "</x>",
                                 "\"",    "'",          "=",      "<",        "\xc3",   "\x00",
                                 "</metadata>", "<manifest>", "</manifest>", "<spine>", "</package>", "#"};

std::string mutate(std::string doc, Rng& rng) {
  const uint32_t edits = 1 + rng.below(3);
  for (uint32_t e = 0; e < edits; ++e) {
    const size_t at = doc.empty() ? 0 : rng.below(static_cast<uint32_t>(doc.size() + 1));
    switch (rng.below(4)) {
      case 0: {
        const char* snippet = rng.pick(kSnippets);
        doc.insert(at, snippet, *snippet ? std::strlen(snippet) : 1);
        break;
      }
      case 1:
        if (at < doc.size()) doc.erase(at, 1 + rng.below(12));
        break;
      case 2:
        if (at < doc.size()) doc.insert(at, doc.substr(at, 1 + rng.below(40)));
        break;
      default:
        if (at < doc.size()) doc[at] = "<>&;\"'=/ x:#%"[rng.below(13)];
        break;
    }
  }
  return doc;
}

std::vector<std::string> documents() {
  std::vector<std::string> docs;
  Rng rng{0x0F0F5EED1234ABCDULL};
  for (int i = 0; i < 6000; i++) {
    Generator generator(rng);
    const std::string doc = generator.document();
    docs.push_back(doc);
    for (int m = 0; m < 2; m++) docs.push_back(mutate(doc, rng));
    docs.push_back(doc.substr(0, rng.below(static_cast<uint32_t>(doc.size()) + 1)));
  }
  return docs;
}

// --- The parser under test ---

struct Mode {
  const char* name;
  bool spine;
  bool metadataOnly;
};

const Mode kModes[] = {
    {"metadata", false, true},
    {"full", false, false},
    {"spine", true, false},
    {"spine-metadata", true, true},
};

const char* const kBases[] = {"OPS/", "", "a/b/", "OEBPS/%41/", "../x/", "/"};

// Feeds `doc` in pieces of `chunk` bytes (0: whole) as ZipFile does, then lists the results.
std::string report(const std::string& doc, const Mode& mode, const std::string& base, const size_t chunk) {
  BookMetadataCache cache;
  Storage = {};
  std::string out;
  const std::string cachePath = "/cache";  // ContentOpfParser keeps a reference
  ContentOpfParser parser(cachePath, base, doc.size(), mode.spine ? &cache : nullptr, mode.metadataOnly);
  if (!parser.setup()) return "setup failed\n";
  const size_t step = chunk == 0 ? doc.size() : chunk;
  for (size_t at = 0; at < doc.size();) {
    const size_t n = std::min(step, doc.size() - at);
    const size_t written = parser.write(reinterpret_cast<const uint8_t*>(doc.data() + at), n);
    if (written != n) {
      out += "short " + std::to_string(at) + " " + std::to_string(written) + "\n";
      break;
    }
    at += n;
  }
  const auto field = [&out](const char* label, const std::string& value) {
    out += label;
    out += '=';
    out += escaped(value);
    out += '\n';
  };
  field("title", parser.title);
  field("author", parser.author);
  field("language", parser.language);
  field("isbn", parser.isbn);
  field("asin", parser.asin);
  field("series", parser.series);
  if (parser.seriesIndex.has_value()) {
    char index[32];
    std::snprintf(index, sizeof(index), "%a", static_cast<double>(*parser.seriesIndex));
    field("index", index);
  }
  field("ncx", parser.tocNcxPath);
  field("nav", parser.tocNavPath);
  field("cover", parser.coverItemHref);
  field("guideCover", parser.guideCoverPageHref);
  field("start", parser.textReferenceHref);
  for (const auto& css : parser.cssFiles) field("css", css);
  for (const auto& href : cache.spine) field("spine", href);
  return out;
}

// Recorded from the C++ ContentOpfParser as of 187753a.
const uint64_t kHashes[] = {
    0xFE6595CD523F0105ULL,  // metadata
    0xE3F33245572EDE6AULL,  // full
    0x4F30FA4BCBE2E285ULL,  // spine
    0xFE6595CD523F0105ULL,  // spine-metadata
};

}  // namespace

TEST(OpfCorpus, ReportsMatchTheRecordedParser) {
  const auto docs = documents();
  const char* print = std::getenv("OPF_CORPUS_PRINT");
  for (size_t m = 0; m < sizeof(kModes) / sizeof(kModes[0]); m++) {
    const Mode& mode = kModes[m];
    Hash hash;
    for (size_t d = 0; d < docs.size(); d++) {
      const std::string& doc = docs[d];
      const std::string base = kBases[d % (sizeof(kBases) / sizeof(kBases[0]))];
      const std::string whole = report(doc, mode, base, 0);
      hash.add(doc);
      hash.add(whole);
      hash.add(report(doc, mode, base, 7));
      hash.add(report(doc, mode, base, 1024));
      if (d % 16 == 0) hash.add(report(doc, mode, base, 1));
      if (print && std::strcmp(print, mode.name) == 0) {
        std::printf("--- %s [%s]\n%s", escaped(doc).c_str(), base.c_str(), whole.c_str());
      }
    }
    EXPECT_EQ(hash.value, kHashes[m]) << mode.name << ": 0x" << std::hex << std::uppercase << hash.value << "ULL";
  }
  EXPECT_GT(docs.size(), 20000u);
}
