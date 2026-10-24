// Host tests for htmlToPlainText(), used to render dictionary definitions that
// cannot be laid out as styled pages.

#include <gtest/gtest.h>

#include <string>

#include "HtmlToPlainText.h"
#include "OldHtmlToPlainText.h"

namespace {

TEST(HtmlToPlainText, StripsTagsAndKeepsText) {
  EXPECT_EQ(htmlToPlainText("<b>quixotic</b> <i>adj.</i>"), "quixotic adj.");
  EXPECT_EQ(htmlToPlainText("<span class=\"x\">in span</span>"), "in span");
  EXPECT_EQ(htmlToPlainText("plain"), "plain");
  EXPECT_EQ(htmlToPlainText(""), "");
}

TEST(HtmlToPlainText, BlockElementsBecomeBreaks) {
  EXPECT_EQ(htmlToPlainText("a<br>b"), "a\nb");
  EXPECT_EQ(htmlToPlainText("<div>a</div><div>b</div>"), "a\nb");
  EXPECT_EQ(htmlToPlainText("<p>a</p><p>b</p>"), "a\n\nb");
  // Consecutive breaks do not stack up.
  EXPECT_EQ(htmlToPlainText("a<br><br><br>b"), "a\nb");
}

TEST(HtmlToPlainText, HeadingsBreakLikeParagraphs) {
  // tagBreak() lists h1-h6 among the paragraph-breaking names, so they must
  // actually reach that comparison -- a name scan that stops at the first digit
  // reads "h1" as "h" and the heading runs into the text after it.
  for (const char* tag : {"h1", "h2", "h3", "h4", "h5", "h6"}) {
    const std::string html = std::string("<") + tag + ">title</" + tag + ">body";
    EXPECT_EQ(htmlToPlainText(html), "title\n\nbody") << tag;
  }
  EXPECT_EQ(htmlToPlainText("<hr>after"), "after");
}

TEST(HtmlToPlainText, DecodesEntities) {
  EXPECT_EQ(htmlToPlainText("Tom &amp; Jerry"), "Tom & Jerry");
  EXPECT_EQ(htmlToPlainText("a&nbsp;b"),
            "a\xC2\xA0"
            "b");
  EXPECT_EQ(htmlToPlainText("&#65;&#66;"), "AB");
  EXPECT_EQ(htmlToPlainText("&#x2014;"), "\xE2\x80\x94");
  // Not entities: left as written rather than swallowed.
  EXPECT_EQ(htmlToPlainText("&notanentity;"), "&notanentity;");
  EXPECT_EQ(htmlToPlainText("100% & up"), "100% & up");
  EXPECT_EQ(htmlToPlainText("<p>Literal &lt;b&gt;word&lt;/b&gt; and &#60;tag&#62;</p>"),
            "Literal <b>word</b> and <tag>");
}

TEST(HtmlToPlainText, TrimsSurroundingWhitespace) {
  EXPECT_EQ(htmlToPlainText("<p>only</p>"), "only");
  EXPECT_EQ(htmlToPlainText("text   "), "text");
  EXPECT_EQ(htmlToPlainText("a\tb"), "a b");
  EXPECT_EQ(htmlToPlainText("<br>a"), "a");
}

TEST(HtmlToPlainText, SurvivesMalformedMarkup) {
  EXPECT_EQ(htmlToPlainText("a <b"), "a <b");
  EXPECT_EQ(htmlToPlainText("x < y"), "x < y");
  EXPECT_EQ(htmlToPlainText("<!-- comment -->kept"), "kept");
}

uint32_t seed;
uint32_t rnd() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}

// bembeyaz-html's plain module against the converter it replaced (OldHtmlToPlainText.h): random
// fragments of tags, entities, breaks and text give the same text, never longer than the HTML.
TEST(HtmlToPlainText, MatchesTheOldConverter) {
  static const char* const PIECES[] = {"<",
                                       ">",
                                       "</",
                                       "<!",
                                       "<?",
                                       "<p>",
                                       "</P>",
                                       "<br>",
                                       "<BR/>",
                                       "<div>",
                                       "<li>",
                                       "<tr>",
                                       "<h1>",
                                       "<H6 x>",
                                       "<hr>",
                                       "< p>",
                                       "</ br>",
                                       "<b>",
                                       "<h7>",
                                       "<p\t>",
                                       "<\nbr>",
                                       "</\vbr>",
                                       "</\fp>",
                                       "</\rli>",
                                       "&#00000000000065;",
                                       "&#000000000000065;",
                                       "&",
                                       ";",
                                       "&amp;",
                                       "&nbsp;",
                                       "&Eacute;",
                                       "&eacute;",
                                       "&#",
                                       "&#x",
                                       "&#X",
                                       "&#65;",
                                       "&#x41;",
                                       "&#x1F600;",
                                       "&#0;",
                                       "&#xD800;",
                                       "&#1114111;",
                                       "&#1114112;",
                                       "&#xFFFFFFFFF;",
                                       "&notanentity;",
                                       "&abcdefghijklmnopq;",
                                       " ",
                                       "  ",
                                       "\t",
                                       "\r",
                                       "\n",
                                       "\r\n",
                                       "x",
                                       "Ab",
                                       "\xC3\xA9",
                                       "1",
                                       "#",
                                       "/"};
  constexpr size_t COUNT = sizeof(PIECES) / sizeof(PIECES[0]);
  seed = 59;
  unsigned diffs = 0;
  for (int c = 0; c < 50000; ++c) {
    std::string html;
    for (int n = static_cast<int>(rnd() % 60); n > 0; --n) {
      html += rnd() % 25 == 0 ? std::string(1, static_cast<char>(rnd())) : std::string(PIECES[rnd() % COUNT]);
    }
    const std::string a = oldPlain::htmlToPlainText(html);
    const std::string b = htmlToPlainText(html);
    if (a != b || a.size() > html.size()) {
      ADD_FAILURE() << "case " << c << " input '" << html << "' old '" << a << "' new '" << b << "'";
      if (++diffs == 5) return;
    }
  }
}

}  // namespace
