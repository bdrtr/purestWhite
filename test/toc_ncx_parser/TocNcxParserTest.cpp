#include <gtest/gtest.h>

#include <string>

#include "Epub/BookMetadataCache.h"
#include "TocNcxParser.h"

namespace {

struct Result {
  BookMetadataCache cache;
  bool complete = true;  // every write took its bytes
};

// Feeds the document in pieces of `chunk` bytes, as the reader streams it out of the book.
Result parse(const std::string& xml, const size_t chunk = 0, const std::string& base = "OEBPS/") {
  Result result;
  TocNcxParser parser(base, xml.size(), &result.cache);
  EXPECT_TRUE(parser.setup());
  const size_t step = chunk ? chunk : xml.size();
  for (size_t pos = 0; pos < xml.size(); pos += step) {
    const size_t n = std::min(step, xml.size() - pos);
    if (parser.write(reinterpret_cast<const uint8_t*>(xml.data() + pos), n) != n) result.complete = false;
  }
  return result;
}

std::string ncx(const std::string& navMap) {
  return R"(<?xml version="1.0" encoding="UTF-8"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">)"
         R"(<head/><docTitle><text>Book</text></docTitle><navMap>)" +
         navMap + "</navMap></ncx>";
}

std::string point(const std::string& label, const std::string& src, const std::string& children = "") {
  return "<navPoint><navLabel><text>" + label + "</text></navLabel><content src=\"" + src + "\"/>" + children +
         "</navPoint>";
}

}  // namespace

TEST(TocNcxParser, ReadsNestedEntriesWithTheirDepth) {
  const auto r = parse(ncx(point("One", "c1.xhtml", point("One A", "c1.xhtml#a", point("Deep", "c1.xhtml#d"))) +
                           point("Two", "c2.xhtml")));
  ASSERT_TRUE(r.complete);
  ASSERT_EQ(r.cache.toc.size(), 4u);
  EXPECT_EQ(r.cache.toc[0].title, "One");
  EXPECT_EQ(r.cache.toc[0].href, "OEBPS/c1.xhtml");
  EXPECT_EQ(r.cache.toc[0].anchor, "");
  EXPECT_EQ(r.cache.toc[0].level, 1);
  EXPECT_EQ(r.cache.toc[1].title, "One A");
  EXPECT_EQ(r.cache.toc[1].anchor, "a");
  EXPECT_EQ(r.cache.toc[1].level, 2);
  EXPECT_EQ(r.cache.toc[2].level, 3);
  EXPECT_EQ(r.cache.toc[3].title, "Two");
  EXPECT_EQ(r.cache.toc[3].level, 1);
}

TEST(TocNcxParser, ResolvesPathsAndDecodesEscapes) {
  const auto r = parse(ncx(point("A", "../Text/ch%201.xhtml#sec%202") + point("B", "sub/./x.xhtml")));
  ASSERT_EQ(r.cache.toc.size(), 2u);
  EXPECT_EQ(r.cache.toc[0].href, "Text/ch 1.xhtml");
  EXPECT_EQ(r.cache.toc[0].anchor, "sec 2");
  EXPECT_EQ(r.cache.toc[1].href, "OEBPS/sub/./x.xhtml");
}

TEST(TocNcxParser, KeepsTheLabelTextAsWritten) {
  const auto r = parse(ncx(point("  Bölüm &amp; <![CDATA[<1>]]>  ", "c.xhtml")));
  ASSERT_EQ(r.cache.toc.size(), 1u);
  EXPECT_EQ(r.cache.toc[0].title, "  Bölüm & <1>  ");
}

TEST(TocNcxParser, SkipsAPointWithoutLabelOrSource) {
  const auto r =
      parse(ncx("<navPoint><content src=\"a.xhtml\"/></navPoint>"
                "<navPoint><navLabel><text>No source</text></navLabel><content/></navPoint>"
                "<navPoint><navLabel><text></text></navLabel><content src=\"b.xhtml\"/></navPoint>" +
                point("Kept", "c.xhtml")));
  ASSERT_EQ(r.cache.toc.size(), 1u);
  EXPECT_EQ(r.cache.toc[0].title, "Kept");
}

// The parser expects a point's label and content before its nested points (as the NCX spec orders
// them); a parent whose label and content follow its children is listed after them.
TEST(TocNcxParser, AParentWrittenAfterItsChildrenComesAfterThem) {
  const auto r = parse(ncx("<navPoint>" + point("Child", "child.xhtml") +
                           "<navLabel><text>Parent</text></navLabel><content src=\"parent.xhtml\"/></navPoint>"));
  ASSERT_EQ(r.cache.toc.size(), 2u);
  EXPECT_EQ(r.cache.toc[0].title, "Child");
  EXPECT_EQ(r.cache.toc[0].level, 2);
  EXPECT_EQ(r.cache.toc[1].title, "Parent");
  EXPECT_EQ(r.cache.toc[1].href, "OEBPS/parent.xhtml");
  EXPECT_EQ(r.cache.toc[1].level, 1);
}

TEST(TocNcxParser, ASecondContentInAPointIsNotAnEntry) {
  const auto r =
      parse(ncx("<navPoint><navLabel><text>Once</text></navLabel><content src=\"a.xhtml\"/>"
                "<content src=\"b.xhtml\"/></navPoint>"));
  ASSERT_EQ(r.cache.toc.size(), 1u);
  EXPECT_EQ(r.cache.toc[0].href, "OEBPS/a.xhtml");
}

TEST(TocNcxParser, IgnoresPointsOutsideTheNavMapAndPrefixedNames) {
  const auto r = parse(R"(<ncx><navList>)" + point("List", "l.xhtml") + R"(</navList><navMap>)" +
                       "<ncx:navPoint xmlns:ncx=\"x\"><navLabel><text>Prefixed</text></navLabel>"
                       "<content src=\"p.xhtml\"/></ncx:navPoint>" +
                       point("Map", "m.xhtml") + "</navMap></ncx>");
  ASSERT_EQ(r.cache.toc.size(), 1u);
  EXPECT_EQ(r.cache.toc[0].title, "Map");
}

TEST(TocNcxParser, ADocumentWithoutNcxRootHasNoEntries) {
  const auto r = parse("<navMap>" + point("A", "a.xhtml") + "</navMap>");
  EXPECT_TRUE(r.cache.toc.empty());
}

TEST(TocNcxParser, ReadsTheSameInAnyPieces) {
  const std::string doc =
      ncx(point("Ünïcödé — one", "a%20b.xhtml#x", point("Two", "../c.xhtml")) + point("Three", "d.xhtml"));
  const auto whole = parse(doc);
  for (const size_t chunk : {1u, 2u, 3u, 7u, 64u}) {
    const auto pieces = parse(doc, chunk);
    ASSERT_TRUE(pieces.complete) << chunk;
    ASSERT_EQ(pieces.cache.toc.size(), whole.cache.toc.size()) << chunk;
    for (size_t i = 0; i < whole.cache.toc.size(); ++i) {
      EXPECT_EQ(pieces.cache.toc[i].title, whole.cache.toc[i].title) << chunk;
      EXPECT_EQ(pieces.cache.toc[i].href, whole.cache.toc[i].href) << chunk;
      EXPECT_EQ(pieces.cache.toc[i].anchor, whole.cache.toc[i].anchor) << chunk;
      EXPECT_EQ(pieces.cache.toc[i].level, whole.cache.toc[i].level) << chunk;
    }
  }
}

// NCX documents declare no HTML entities: an undeclared one is an error, and the entries read
// before it stay.
TEST(TocNcxParser, AnUndeclaredEntityStopsTheDocument) {
  const auto r = parse(ncx(point("Before", "a.xhtml") + point("A&nbsp;B", "b.xhtml") + point("After", "c.xhtml")));
  EXPECT_FALSE(r.complete);
  ASSERT_EQ(r.cache.toc.size(), 1u);
  EXPECT_EQ(r.cache.toc[0].title, "Before");
}

TEST(TocNcxParser, MalformedXmlStopsTheDocument) {
  const auto r = parse(ncx(point("Before", "a.xhtml") + "<navPoint><navLabel></navPoint>" + point("After", "b.xhtml")));
  EXPECT_FALSE(r.complete);
  ASSERT_EQ(r.cache.toc.size(), 1u);
}

TEST(TocNcxParser, ABaseWithoutSlashIsJoinedAsWritten) {
  const auto r = parse(ncx(point("A", "c.xhtml")), 0, "");
  ASSERT_EQ(r.cache.toc.size(), 1u);
  EXPECT_EQ(r.cache.toc[0].href, "c.xhtml");
}

// A label keeps at most 1024 bytes (bembeyaz-toc's MAX_LABEL), ending on a whole character.
TEST(TocNcxParser, ALongLabelIsCut) {
  std::string label = "a";
  for (int i = 0; i < 2000; ++i) label += "\xC3\xA9";  // é
  const auto r = parse(ncx(point(label, "a.xhtml")), 101);
  ASSERT_TRUE(r.complete);
  ASSERT_EQ(r.cache.toc.size(), 1u);
  EXPECT_EQ(r.cache.toc[0].title.size(), 1023u);
  EXPECT_EQ(r.cache.toc[0].title, label.substr(0, 1023));
}
