// The FreeInk SDK's XML readers, which use the firmware's XML parser through expat's API: OPDS 1.x
// feeds (OpdsParser), OpenSearch descriptions (OpenSearchDescParser) and WebDAV listings
// (XmlListParser). Each reads a set of documents and mutations of them; what it extracts hashes
// to a value recorded with expat 2.7.3 as the firmware built it. A changed hash names the reader;
// rerun with SDK_XML_PRINT=<reader> to list its cases.

#include <OpdsParser.h>
#include <OpenSearchDescParser.h>
#include <XmlListParser.h>
#include <gtest/gtest.h>

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

const char kBytes[] = {'<', '>', '&', ';', '"', '\'', '=', '/', '!', '?', '-', ']', ' ', '\n', '\0', 'x', ':', '\xc3'};

// Mutations stay below 1024 bytes, which the readers hand to the parser in one piece.
std::string mutate(std::string doc, Rng& rng) {
  const uint32_t edits = 1 + rng.below(3);
  for (uint32_t e = 0; e < edits; ++e) {
    const size_t at = doc.empty() ? 0 : rng.below(static_cast<uint32_t>(doc.size()));
    switch (rng.below(4)) {
      case 0:
        if (at < doc.size()) doc[at] = kBytes[rng.below(sizeof(kBytes))];
        break;
      case 1:
        if (at < doc.size()) doc.erase(at, 1 + rng.below(6));
        break;
      case 2:
        doc.insert(at, rng.below(2) ? "&amp;" : "<![CDATA[<b>x</b>]]>");
        break;
      default:
        doc = doc.substr(0, at);
        break;
    }
  }
  return doc;
}

std::string show(const std::string& doc) {
  std::string shown;
  for (const char c : doc) {
    const auto b = static_cast<uint8_t>(c);
    if (b < 0x20 || b >= 0x7F) {
      char hex[8];
      std::snprintf(hex, sizeof(hex), "\\x%02X", b);
      shown += hex;
    } else {
      shown += c;
    }
  }
  return shown;
}

void field(std::string& out, const char* name, const std::string& value) {
  out += name;
  out += '=';
  out += value;
  out += '\n';
}

std::string readOpds(const std::string& doc) {
  OpdsParser parser;
  parser.write(reinterpret_cast<const uint8_t*>(doc.data()), doc.size());
  parser.flush();
  std::string out;
  field(out, "error", parser.error() ? "1" : "0");
  field(out, "truncated", parser.truncated() ? "1" : "0");
  field(out, "title", parser.getFeedTitle());
  field(out, "search", parser.getSearchTemplate());
  field(out, "searchDescription", parser.getSearchDescriptionUrl());
  field(out, "next", parser.getNextPageUrl());
  field(out, "prev", parser.getPrevPageUrl());
  field(out, "first", parser.getFirstPageUrl());
  field(out, "last", parser.getLastPageUrl());
  field(out, "shelf", parser.getShelfUrl());
  field(out, "wishlist", parser.getWishlistUrl());
  field(out, "history", parser.getHistoryUrl());
  field(out, "page", std::to_string(parser.currentPage()) + "/" + std::to_string(parser.pageCount()));
  auto entries = [&](const std::vector<OpdsEntry>& list, const char* kind) {
    for (const auto& e : list) {
      out += kind;
      out += '\n';
      field(out, " type", e.type == OpdsEntryType::BOOK ? "book" : "navigation");
      field(out, " title", e.title);
      field(out, " author", e.author);
      field(out, " href", e.href);
      field(out, " id", e.id);
      field(out, " heading", e.heading);
      field(out, " detail", e.detail);
      field(out, " flags", std::string(e.purchase ? "p" : "") + (e.indirect ? "i" : "") + (e.lcpLicense ? "l" : ""));
      field(out, " self", e.selfHref);
      field(out, " description", e.description);
      field(out, " cover", e.coverHref);
    }
  };
  entries(parser.getEntries(), "entry");
  entries(parser.takeFacetEntries(), "facet");
  return out;
}

std::string readOpenSearch(const std::string& doc) {
  OpenSearchDescParser parser;
  parser.write(reinterpret_cast<const uint8_t*>(doc.data()), doc.size());
  parser.flush();
  return std::string(parser.error() ? "error\n" : "ok\n") + parser.getTemplate() + "\n";
}

std::string readWebDav(const std::string& doc) {
  struct Source {
    const std::string* doc;
    size_t at;
  } source{&doc, 0};
  std::string url = "href", title = "displayname", none;
  const std::string* const selectors[] = {&url, &title, &none, &none};
  std::string out;
  XmlListParser parser(
      "response", "collection", selectors,
      [](void* context, XmlListParser::RawItem& row) {
        auto* out = static_cast<std::string*>(context);
        *out += row.isDir ? "dir " : "file ";
        *out += row.field[XmlListParser::F_URL] + " | " + row.field[XmlListParser::F_TITLE] + "\n";
      },
      &out);
  XmlListParser::UrlOptions urls;
  urls.requestUrl = "https://dav.example/books/";
  urls.skipSelf = true;
  urls.resolveUrls = true;
  parser.setUrlOptions(urls);
  const bool ok = parser.parse(
      [](void* context, char* buf, const size_t n) {
        auto* s = static_cast<Source*>(context);
        const size_t k = std::min(n, s->doc->size() - s->at);
        std::memcpy(buf, s->doc->data() + s->at, k);
        s->at += k;
        return static_cast<int>(k);
      },
      &source);
  return out + (ok ? "ok\n" : "error\n");
}

const std::vector<std::string> kOpds = {
    R"(<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog" xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/">
<id>urn:uuid:root</id><title>Kitapl&#305;k &amp; Ar&#351;iv</title>
<link rel="search" href="/opds/search/{searchTerms}" type="application/atom+xml"/>
<link rel="search" href="/opds/osd" type="application/opensearchdescription+xml"/>
<link rel="next" href="/opds/new?page=3"/><link rel="previous" href="/opds/new?page=1"/>
<link rel="first" href="/opds/new"/><link rel="last" href="/opds/new?page=5"/>
<opensearch:totalResults>95</opensearch:totalResults><opensearch:itemsPerPage>20</opensearch:itemsPerPage>
<opensearch:startIndex>21</opensearch:startIndex>
<entry><title>En yeniler</title><id>new</id>
<link rel="subsection" href="/opds/new" type="application/atom+xml;profile=opds-catalog;kind=acquisition"/></entry>
<entry><title>Yazarlar</title><id>authors</id><content type="text">T&#252;m yazarlar</content>
<link href="/opds/authors" type="application/atom+xml;profile=opds-catalog;kind=navigation"/></entry>
</feed>)",
    R"(<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:opds="http://opds-spec.org/2010/catalog" xmlns:thr="http://purl.org/syndication/thread/1.0">
<title>Kitaplar</title>
<link rel="http://opds-spec.org/facet" href="/opds/books?sort=new" title="Yeni" opds:facetGroup="S&#305;ra" thr:count="12" opds:activeFacet="true"/>
<link rel="http://opds-spec.org/facet" href="/opds/books?sort=title" title="Ba&#351;l&#305;k" opds:facetGroup="S&#305;ra"/>
<link rel="http://opds-spec.org/shelf" href="/opds/shelf"/>
<entry>
  <title>Su&#231; ve Ceza</title>
  <author><name>Fyodor Dostoyevski</name></author><author><name>&#199;evirmen</name></author>
  <id>urn:isbn:9789750719387</id>
  <summary type="html">&lt;p&gt;Raskolnikov&amp;apos;un &lt;b&gt;hik&#226;yesi&lt;/b&gt;.&lt;/p&gt;</summary>
  <link rel="http://opds-spec.org/image" href="/covers/1.jpg" type="image/jpeg"/>
  <link rel="http://opds-spec.org/image/thumbnail" href="/covers/1-t.jpg" type="image/jpeg"/>
  <link rel="http://opds-spec.org/acquisition/sample" href="/sample/1.epub" type="application/epub+zip"/>
  <link rel="http://opds-spec.org/acquisition" href="/get/1.epub" type="application/epub+zip"/>
  <link rel="http://opds-spec.org/acquisition/open-access" href="/get/1.kepub" type="application/kepub+zip"/>
</entry>
<entry>
  <title>Sat&#305;l&#305;k</title><id>b2</id>
  <content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml"><p>Bir <em>roman</em></p></div></content>
  <link rel="http://opds-spec.org/acquisition/buy" href="/buy/2" type="application/epub+zip"><opds:price currencycode="TRY">49.90</opds:price></link>
</entry>
<entry>
  <title><![CDATA[CDATA & <ba&#351;l&#305;k>]]></title><id>b3</id>
  <link rel="http://opds-spec.org/acquisition/borrow" href="/lcp/3.lcpl" type="application/vnd.readium.lcp.license.v1.0+json"/>
  <link rel="alternate" href="/pub/3" type="application/opds-publication+json"/>
</entry>
</feed>)",
    R"(<feed xmlns="http://www.w3.org/2005/Atom"><title>Bo&#351;</title></feed>)",
    R"(<feed xmlns="http://www.w3.org/2005/Atom"><title>Hatal&#305;</title><entry><title>A</title>&nbsp;</entry></feed>)",
    "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><feed xmlns=\"http://www.w3.org/2005/Atom\"><title>Caf\xe9</title>"
    "<entry><title>\xc0 la</title><id>1</id><link rel=\"http://opds-spec.org/acquisition\" href=\"/a.epub\" "
    "type=\"application/epub+zip\"/></entry></feed>",
    "<a:feed xmlns:a=\"http://www.w3.org/2005/Atom\"><a:title>Prefixed</a:title><a:entry><a:title>P</a:title>"
    "<a:id>p</a:id><a:link href=\"/p\" type=\"application/atom+xml;profile=opds-catalog\"/></a:entry></a:feed>",
};

const std::vector<std::string> kOpenSearch = {
    R"(<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
<ShortName>Ara</ShortName>
<Url type="text/html" template="/search?q={searchTerms}"/>
<Url type="application/atom+xml;profile=opds-catalog" template="/opds/search?q={searchTerms}&amp;p={startPage?}"/>
</OpenSearchDescription>)",
    R"(<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/"><Url template="/s/{searchTerms}"/></OpenSearchDescription>)",
    R"(<os:OpenSearchDescription xmlns:os="http://a9.com/-/spec/opensearch/1.1/"><os:Url type="application/atom+xml" template="/x?{searchTerms}"/></os:OpenSearchDescription>)",
};

const std::vector<std::string> kWebDav = {
    "<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\">"
    "<d:response><d:href>/books/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype>"
    "</d:prop></d:propstat></d:response>"
    "<d:response><d:href> /books/a%20b.epub </d:href><d:propstat><d:prop><d:displayname>A &amp; B</d:displayname>"
    "<d:resourcetype/></d:prop></d:propstat></d:response>"
    "<d:response><d:href>/books/sub/</d:href><d:resourcetype><d:collection/></d:resourcetype></d:response>"
    "<d:response><d:href>/books/Caf%C3%A9.EPUB</d:href></d:response>"
    "</d:multistatus>",
    "<D:multistatus xmlns:D=\"DAV:\"><D:response><D:href>relative.epub</D:href></D:response>"
    "<D:response><D:href>https://cdn.example/x/C.epub</D:href><D:displayname><![CDATA[C & D]]></D:displayname>"
    "</D:response></D:multistatus>",
    "<multistatus xmlns=\"DAV:\"><response><href>/books/x.epub</href></response></multistatus>",
};

struct Reader {
  const char* name;
  const std::vector<std::string>* seeds;
  std::string (*read)(const std::string&);
};

// Recorded with expat 2.7.3 (XML_GE=0, XML_CONTEXT_BYTES=1024), as lib/expat built.
const uint64_t kHashes[] = {
    0xBAF6E158F581C0C1ULL,  // opds
    0x80EDF9E70CB876AFULL,  // opensearch
    0x4FAE041D518E3BD6ULL,  // webdav
};

}  // namespace

TEST(SdkXmlParsers, ReadersExtractWhatTheyDidWithExpat) {
  const Reader readers[] = {
      {"opds", &kOpds, readOpds},
      {"opensearch", &kOpenSearch, readOpenSearch},
      {"webdav", &kWebDav, readWebDav},
  };
  ASSERT_EQ(sizeof(readers) / sizeof(readers[0]), sizeof(kHashes) / sizeof(kHashes[0]));
  const char* print = std::getenv("SDK_XML_PRINT");
  for (size_t r = 0; r < sizeof(readers) / sizeof(readers[0]); ++r) {
    Hash hash;
    for (size_t s = 0; s < readers[r].seeds->size(); ++s) {
      const std::string& seed = (*readers[r].seeds)[s];
      Rng rng{0x9E3779B97F4A7C15ULL ^ (r << 40) ^ (s * 0x2545F4914F6CDD1DULL)};
      std::vector<std::string> docs{seed};
      for (int m = 0; m < 200; ++m) {
        std::string doc = mutate(seed.substr(0, 1000), rng);
        docs.push_back(doc);
      }
      for (const auto& doc : docs) {
        const std::string result = readers[r].read(doc);
        hash.add(doc);
        hash.add(result);
        if (print && std::strcmp(print, readers[r].name) == 0) {
          std::printf("--- %s\n%s", show(doc).c_str(), result.c_str());
        }
      }
    }
    EXPECT_EQ(hash.value, kHashes[r]) << readers[r].name << ": 0x" << std::hex << std::uppercase << hash.value << "ULL";
  }
}
