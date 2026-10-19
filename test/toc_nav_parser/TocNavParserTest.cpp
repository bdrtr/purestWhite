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
