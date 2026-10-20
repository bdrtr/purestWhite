#include <gtest/gtest.h>

#include <string>

#include "Epub/BookMetadataCache.h"
#include "TocNavParser.h"

namespace {

BookMetadataCache parse(const std::string& xml) {
  BookMetadataCache cache;
  const std::string base = "OEBPS/";
  TocNavParser parser(base, xml.size(), &cache);
  EXPECT_TRUE(parser.setup());
  EXPECT_EQ(parser.write(reinterpret_cast<const uint8_t*>(xml.data()), xml.size()), xml.size());
  return cache;
}

}  // namespace

TEST(TocNavParser, ReadsNestedEntries) {
  const auto cache = parse(R"(<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>)"
                           R"(<li><a href="c1.xhtml">One</a><ol><li><a href="c1.xhtml#s">One A</a></li></ol></li>)"
                           R"(<li><a href="c2.xhtml">Two</a></li></ol></nav></body></html>)");

  ASSERT_EQ(cache.toc.size(), 3u);
  EXPECT_EQ(cache.toc[0].title, "One");
  EXPECT_EQ(cache.toc[0].href, "OEBPS/c1.xhtml");
  EXPECT_EQ(cache.toc[0].level, 1);
  EXPECT_EQ(cache.toc[1].title, "One A");
  EXPECT_EQ(cache.toc[1].anchor, "s");
  EXPECT_EQ(cache.toc[1].level, 2);
  EXPECT_EQ(cache.toc[2].title, "Two");
}

// Nav documents use HTML entities without declaring them; the parser used to stop at the first.
TEST(TocNavParser, ExpandsHtmlEntitiesInLabels) {
  const auto cache = parse(
      "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n"
      R"(<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>)"
      R"(<li><a href="c1.xhtml">1.&nbsp;Bir &mdash; &bogus;</a></li>)"
      R"(<li><a href="c2.xhtml" title="&nbsp;">&copy;&amp;</a></li></ol></nav></body></html>)");

  ASSERT_EQ(cache.toc.size(), 2u);
  EXPECT_EQ(cache.toc[0].title,
            "1.\xC2\xA0"
            "Bir \xE2\x80\x94 &bogus;");
  EXPECT_EQ(cache.toc[1].title, "\xC2\xA9&");
}

// Behind an external DTD (which the parser does not read) they used to be dropped.
TEST(TocNavParser, ExpandsHtmlEntitiesBehindAnExternalDtd) {
  const auto cache = parse(
      "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n"
      R"(<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>)"
      R"(<li><a href="c1.xhtml">Chapter&nbsp;1</a></li></ol></nav></body></html>)");

  ASSERT_EQ(cache.toc.size(), 1u);
  EXPECT_EQ(cache.toc[0].title,
            "Chapter\xC2\xA0"
            "1");
}

namespace {

std::string nav(const std::string& body) {
  return R"(<html xmlns:epub="http://www.idpf.org/2007/ops"><body>)" + body + "</body></html>";
}

}  // namespace

TEST(TocNavParser, ResolvesPathsAndDecodesEscapes) {
  const auto cache = parse(nav(R"(<nav epub:type="toc"><ol><li><a href="../Text/ch%201.xhtml#sec%202">A</a></li>)"
                               R"(<li><a href="x.xhtml#">B</a></li></ol></nav>)"));
  ASSERT_EQ(cache.toc.size(), 2u);
  EXPECT_EQ(cache.toc[0].href, "Text/ch 1.xhtml");
  EXPECT_EQ(cache.toc[0].anchor, "sec 2");
  EXPECT_EQ(cache.toc[1].href, "OEBPS/x.xhtml");
  EXPECT_EQ(cache.toc[1].anchor, "");
}

TEST(TocNavParser, ReadsOnlyTheTocNav) {
  const auto cache = parse(nav(R"(<nav epub:type="landmarks"><ol><li><a href="l.xhtml">Landmark</a></li></ol></nav>)"
                               R"(<div><nav type="toc"><ol><li><a href="t.xhtml">Toc</a></li></ol></nav></div>)"
                               R"(<nav epub:type="page-list"><ol><li><a href="p.xhtml">Page</a></li></ol></nav>)"));
  ASSERT_EQ(cache.toc.size(), 1u);
  EXPECT_EQ(cache.toc[0].title, "Toc");
}

TEST(TocNavParser, CollectsTextInsideAnAnchor) {
  const auto cache = parse(nav(R"(<nav epub:type="toc"><ol><li><a href="a.xhtml"><span>1.</span> <b>Bir</b></a></li>)"
                               R"(<li><span>Heading only</span><ol><li><a href="b.xhtml">Child</a></li></ol></li>)"
                               R"(<li><a>No href</a></li></ol></nav>)"));
  ASSERT_EQ(cache.toc.size(), 2u);
  EXPECT_EQ(cache.toc[0].title, "1. Bir");
  EXPECT_EQ(cache.toc[1].title, "Child");
  EXPECT_EQ(cache.toc[1].level, 2);
}

TEST(TocNavParser, NeedsHtmlAndBody) {
  EXPECT_TRUE(parse(R"(<nav epub:type="toc"><ol><li><a href="a.xhtml">A</a></li></ol></nav>)").toc.empty());
  EXPECT_TRUE(
      parse(R"(<html><nav epub:type="toc"><ol><li><a href="a.xhtml">A</a></li></ol></nav></html>)").toc.empty());
}
