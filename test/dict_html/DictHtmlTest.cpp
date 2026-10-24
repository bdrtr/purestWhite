// bembeyaz-html's xhtml module, written through DictHtmlPages.cpp's BufferedFileWriter as the
// reader does, against the tag-soup normalizer it replaced (OldDictHtml.h): random tag soup, and
// writes that come up short, must give the same file and the same result.
#include <bembeyaz/html.h>
#include <gtest/gtest.h>

#include <cstdint>
#include <string>

#include "OldDictHtml.h"

namespace {

uint32_t seed;
uint32_t rnd() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}

// The new writeNormalizedXhtml (DictHtmlPages.cpp).
bool newNormalize(const std::string& html, HalFile& file) {
  oldDictHtml::BufferedFileWriter out(file);
  return bb_html_normalize_xhtml(
             html.data(), html.size(),
             [](void* context, const char* piece, const size_t len) {
               return static_cast<oldDictHtml::BufferedFileWriter*>(context)->append(piece, len);
             },
             &out) &&
         out.flush();
}

std::string soup() {
  static const char* const PIECES[] = {"<",
                                       ">",
                                       "</",
                                       "<!--",
                                       "-->",
                                       "<!",
                                       "<?",
                                       "?>",
                                       "&",
                                       "&amp;",
                                       "&#x",
                                       "&#X",
                                       "&#",
                                       "#",
                                       ";",
                                       "'",
                                       "\"",
                                       "=",
                                       "/",
                                       "br",
                                       "BR",
                                       "img",
                                       "a",
                                       "Hr",
                                       "wbr",
                                       "Col",
                                       "b",
                                       "ABCDEFGHIJKLMNOPQRS",
                                       " ",
                                       "x",
                                       "\xC3\xA9",
                                       "\n",
                                       "12",
                                       "fF",
                                       "<br>",
                                       "</br>",
                                       "<i>",
                                       "</I>",
                                       "<a href=\"x>y\">",
                                       "<img src='q'/>",
                                       "&lt;",
                                       "&#123;"};
  constexpr size_t COUNT = sizeof(PIECES) / sizeof(PIECES[0]);
  std::string s;
  for (int n = static_cast<int>(rnd() % 120); n > 0; --n) {
    if (rnd() % 20 == 0) {
      s += static_cast<char>(rnd());
    } else {
      s += PIECES[rnd() % COUNT];
    }
  }
  return s;
}

}  // namespace

TEST(DictHtml, MatchesTheOldNormalizer) {
  seed = 53;
  unsigned diffs = 0;
  for (int c = 0; c < 50000; ++c) {
    const std::string html = soup();
    HalFile a, b;
    if (rnd() % 8 == 0) a.room = b.room = rnd() % 600;
    const bool okA = oldDictHtml::writeNormalizedXhtml(html, a);
    const bool okB = newNormalize(html, b);
    if (okA != okB || a.bytes != b.bytes) {
      ADD_FAILURE() << "case " << c << " input '" << html << "' old '" << std::string(a.bytes.begin(), a.bytes.end())
                    << "' new '" << std::string(b.bytes.begin(), b.bytes.end()) << "'";
      if (++diffs == 5) return;
    }
  }
}
