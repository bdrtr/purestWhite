// Lays out seeded random chapters (XHTML plus a stylesheet) through ChapterHtmlSlimParser and
// hashes every page it emits, the page callback's arguments and the anchor table. The group
// hashes are recorded from the parser as it was before its move to Rust began; a slice of that
// move must leave them unchanged.
//   CHAPTER_CORPUS_RECORD=1  prints the hashes to paste into EXPECTED.
//   CHAPTER_CORPUS_DUMP=1    prints one hash per case, to diff two trees and find the first
//                            changed case.
//   CHAPTER_CORPUS_CASE=n    runs case n alone and writes its chapter and stylesheet to the temp
//                            directory.
#include <Epub.h>
#include <Epub/Page.h>
#include <Epub/hyphenation/Hyphenator.h>
#include <GfxRenderer.h>
#include <HalStorage.h>
#include <gtest/gtest.h>
#include <unistd.h>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <functional>
#include <string>
#include <vector>

#include "Epub/parsers/ChapterHtmlSlimParser.h"

namespace {

// xorshift64*: the same sequence on every standard library.
class Rng {
 public:
  explicit Rng(uint64_t seed) : state_(seed * 0x9E3779B97F4A7C15ULL + 1) {}
  uint32_t next() {
    state_ ^= state_ >> 12;
    state_ ^= state_ << 25;
    state_ ^= state_ >> 27;
    return static_cast<uint32_t>((state_ * 0x2545F4914F6CDD1DULL) >> 32);
  }
  int range(int lo, int hi) { return lo + static_cast<int>(next() % static_cast<uint32_t>(hi - lo + 1)); }
  bool chance(int percent) { return range(1, 100) <= percent; }
  template <typename T, size_t N>
  const T& pick(const T (&items)[N]) {
    return items[next() % N];
  }

 private:
  uint64_t state_;
};

struct Hash {
  uint64_t value = 0xCBF29CE484222325ULL;
  void bytes(const void* data, size_t len) {
    const auto* p = static_cast<const uint8_t*>(data);
    for (size_t i = 0; i < len; ++i) value = (value ^ p[i]) * 0x100000001B3ULL;
  }
  void u32(uint32_t v) { bytes(&v, sizeof(v)); }
  void str(const std::string& s) {
    u32(static_cast<uint32_t>(s.size()));
    bytes(s.data(), s.size());
  }
};

const char* const WORDS[] = {"the",
                             "a",
                             "chapter",
                             "reading",
                             "ink",
                             "paper",
                             "Istanbul",
                             "çiçek",
                             "ağaç",
                             "Işık",
                             "öğretmen",
                             "şiir",
                             "Über",
                             "straße",
                             "naïve",
                             "café",
                             "1984",
                             "3.14",
                             "—",
                             "“quoted”",
                             "‘single’",
                             "(paren)",
                             "end.",
                             "comma,",
                             "semi;",
                             "x",
                             "I",
                             "well-known",
                             "e.g.",
                             "שלום",
                             "עולם",
                             "مرحبا",
                             "بالعالم",
                             "日本語",
                             "漢字",
                             "e\xCC\x81",
                             "a\xC2\xAD"
                             "b\xC2\xAD"
                             "c",
                             "\xC2\xA0",
                             "1\xE2\x80\xAF"
                             "000",
                             "\xEF\xBB\xBF",
                             "tab\there",
                             "line\nbreak",
                             "  ",
                             "&amp;",
                             "&lt;tag&gt;",
                             "&nbsp;",
                             "&mdash;",
                             "&copy;",
                             "&bogus;",
                             "&#8212;",
                             "&#x41;",
                             "[1]",
                             "*",
                             "Mr.",
                             "antidisestablishmentarianism",
                             "Donaudampfschifffahrtsgesellschaftskapitän",
                             "hyphenation",
                             "unbelievably",
                             "Rechtsschutzversicherungsgesellschaften"};

const char* const CLASSES[] = {"c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9"};

// CSS declarations the parser reads, with some it ignores.
const char* const DECLARATIONS[] = {"font-size: 1.4em",
                                    "font-size: 0.8em",
                                    "font-size: 2rem",
                                    "font-size: 120%",
                                    "font-size: 9pt",
                                    "font-size: x-large",
                                    "font-size: small",
                                    "font-size: 30px",
                                    "font-weight: bold",
                                    "font-weight: normal",
                                    "font-style: italic",
                                    "font-style: normal",
                                    "font-variant: small-caps",
                                    "font-variant-caps: all-small-caps",
                                    "text-align: center",
                                    "text-align: right",
                                    "text-align: justify",
                                    "text-align: left",
                                    "text-indent: 2em",
                                    "text-indent: -1em",
                                    "text-indent: 10%",
                                    "text-indent: 0",
                                    "margin: 1em 0",
                                    "margin-top: 2em",
                                    "margin-bottom: 0.5em",
                                    "margin-left: 3em",
                                    "margin-right: 10%",
                                    "margin-left: -2em",
                                    "padding: 4px",
                                    "padding-left: 1em",
                                    "padding-top: 6px",
                                    "border: 1px solid black",
                                    "border: 2px double #000",
                                    "border-top: 1px solid",
                                    "border-width: 3px; border-style: dashed",
                                    "background-color: #eee",
                                    "background: rgb(220,220,220)",
                                    "page-break-before: always",
                                    "page-break-after: always",
                                    "break-before: page",
                                    "break-after: page",
                                    "display: none",
                                    "display: block",
                                    "display: inline",
                                    "direction: rtl",
                                    "direction: ltr",
                                    "white-space: pre",
                                    "white-space: pre-wrap",
                                    "white-space: normal",
                                    "text-decoration: underline",
                                    "text-decoration: line-through",
                                    "text-decoration-line: underline",
                                    "vertical-align: super",
                                    "vertical-align: sub",
                                    "list-style-type: none",
                                    "list-style-type: decimal",
                                    "float: left",
                                    "initial-letter: 3",
                                    "initial-letter: 2 1",
                                    "width: 50%",
                                    "height: 3em",
                                    "color: red",
                                    "line-height: 2",
                                    "padding-left: 300px",
                                    "height: 90%",
                                    "height: 100%; margin-top: 2em",
                                    "margin-left: 70%; margin-right: 50%"};

class Generator {
 public:
  explicit Generator(uint64_t seed) : rng_(seed) {}

  std::string css() {
    std::string out;
    const char* const selectors[] = {"p",
                                     "h1",
                                     "h2",
                                     "h3",
                                     "li",
                                     "td",
                                     "th",
                                     "blockquote",
                                     "span",
                                     "div p",
                                     ".c1 > span",
                                     "ul li",
                                     "p:first-child",
                                     "pre",
                                     "a",
                                     "sup",
                                     "em",
                                     "b"};
    for (int i = 0, n = rng_.range(0, 14); i < n; ++i) {
      if (rng_.chance(60)) {
        out += std::string(".") + rng_.pick(CLASSES);
      } else if (rng_.chance(15)) {
        out += std::string("p.") + rng_.pick(CLASSES) + "::first-letter";
      } else {
        out += rng_.pick(selectors);
      }
      out += " { " + declarations(rng_.range(1, 4)) + " }\n";
    }
    return out;
  }

  std::string chapter() {
    std::string out;
    if (rng_.chance(50)) out += "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n";
    if (rng_.chance(30)) out += "<!DOCTYPE html>\n";
    out +=
        "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" "
        "xmlns:xlink=\"http://www.w3.org/1999/xlink\">";
    out += "<head><title>" + text(2) + "</title>";
    if (rng_.chance(30)) out += "<style>p { color: blue }</style>";
    out += "</head>";
    if (rng_.chance(10)) out += text(3);                        // outside <body>: not visible
    const std::string body = rng_.chance(5) ? "BODY" : "body";  // the parser finds <body> in any case
    out += "<" + body + attributes() + ">";
    for (int i = 0, n = rng_.range(1, 25); i < n; ++i) out += block(0);
    out += "</" + body + "></html>";
    if (rng_.chance(5)) out += "<p>after the end</p>";
    if (rng_.chance(5)) out.resize(rng_.range(0, static_cast<int>(out.size())));  // truncated file
    return out;
  }

  std::vector<std::string> ids;

 private:
  std::string declarations(int n) {
    std::string out;
    for (int i = 0; i < n; ++i) out += std::string(rng_.pick(DECLARATIONS)) + "; ";
    return out;
  }

  std::string attributes() {
    std::string out;
    if (rng_.chance(35)) out += std::string(" class=\"") + rng_.pick(CLASSES) + (rng_.chance(20) ? " c1" : "") + "\"";
    if (rng_.chance(20)) out += " style=\"" + declarations(rng_.range(1, 3)) + "\"";
    if (rng_.chance(15)) {
      ids.push_back("id" + std::to_string(ids.size()));
      out += " id=\"" + ids.back() + "\"";
    }
    if (rng_.chance(4)) out += rng_.chance(75) ? " dir=\"rtl\"" : " dir=\"ltr\"";
    if (rng_.chance(2)) out += " hidden=\"hidden\"";
    return out;
  }

  std::string word() {
    if (rng_.chance(2)) return std::string(rng_.range(30, 300), 'w');  // past the word buffer
    return rng_.pick(WORDS);
  }

  std::string text(int maxWords) {
    std::string out;
    for (int i = 0, n = rng_.range(1, maxWords); i < n; ++i) {
      if (i > 0 || rng_.chance(20)) out += rng_.chance(90) ? " " : rng_.chance(50) ? "\n  " : "   ";
      out += word();
    }
    if (rng_.chance(10)) out += " ";
    return out;
  }

  std::string inlineContent(int depth) {
    std::string out;
    for (int i = 0, n = rng_.range(1, depth == 0 ? 6 : 3); i < n; ++i) {
      const int kind = depth > 3 ? 0 : rng_.range(0, 30);
      if (kind <= 12) {
        out += text(12);
      } else if (kind <= 20) {
        const char* const tags[] = {"b",   "strong", "i",     "em",  "u",    "s",    "span", "span", "span",
                                    "sup", "sub",    "small", "big", "code", "cite", "del",  "ins",  "q"};
        const std::string tag = rng_.pick(tags);
        out += "<" + tag + attributes() + ">" + inlineContent(depth + 1) + "</" + tag + ">";
      } else if (kind <= 23) {
        out += "<a" + link() + attributes() + ">" + inlineContent(depth + 1) + "</a>";
      } else if (kind == 24) {
        out += "<ruby>" + text(2) + (rng_.chance(30) ? "<rp>(</rp>" : "") + "<rt>" + text(2) + "</rt>" +
               (rng_.chance(30) ? text(1) + "<rt> x </rt>" : "") + "</ruby>";
      } else if (kind == 25) {
        out += rng_.chance(50) ? "<br/>" : "<br></br>";
      } else if (kind == 26) {
        out +=
            "<img src=\"" + image() + "\"" + (rng_.chance(50) ? " alt=\"" + text(3) + "\"" : "") + attributes() + "/>";
      } else if (kind == 27) {
        out += "<!-- " + text(2) + " -->";
      } else if (kind == 28) {
        out += "<![CDATA[" + text(3) + " <b>raw</b> ]]>";
      } else if (kind == 29) {
        out += rng_.chance(50) ? "<script>var x = 1;</script>" : "<style>p { x: y }</style>";
      } else {
        out += "<span epub:type=\"pagebreak\" id=\"pb" + std::to_string(rng_.range(0, 99)) + "\" title=\"" +
               std::to_string(rng_.range(1, 300)) + "\"/>";
      }
    }
    return out;
  }

  // Names the corpus's Epub stub serves (some with a fragment or an escape), and one it does not.
  std::string image() {
    const char* const images[] = {"images/i0.jpg", "images/i1.jpg",  "images/i2.jpg",   "images/i3.jpg",
                                  "images/c.png",  "images/c.png#x", "images/i%30.jpg", "images/c.gif"};
    return rng_.pick(images);
  }

  std::string link() {
    const int kind = rng_.range(0, 5);
    if (kind == 0 && !ids.empty()) return " href=\"#" + ids[rng_.next() % ids.size()] + "\"";
    if (kind == 1) return " epub:type=\"noteref\" href=\"#fn" + std::to_string(rng_.range(0, 9)) + "\"";
    if (kind == 2) return " href=\"notes.xhtml#n" + std::to_string(rng_.range(0, 9)) + "\"";
    if (kind == 3) return rng_.chance(70) ? " href=\"http://example.com/x\"" : " href=\"javascript:void(0)\"";
    if (kind == 4) return " role=\"doc-noteref\" href=\"#fn" + std::to_string(rng_.range(0, 9)) + "\"";
    return "";
  }

  std::string list(int depth) {
    const std::string tag = rng_.chance(50) ? "ul" : "ol";
    std::string out = "<" + tag + attributes() + ">";
    for (int i = 0, n = rng_.range(0, 5); i < n; ++i) {
      out += "<li" + attributes() + ">";
      out += rng_.chance(70) ? inlineContent(depth + 1) : block(depth + 1);
      if (rng_.chance(15) && depth < 3) out += list(depth + 1);
      out += "</li>";
    }
    return out + "</" + tag + ">";
  }

  std::string table(int depth) {
    std::string out = "<table" + attributes() + (rng_.chance(20) ? " border=\"1\"" : "") + ">";
    if (rng_.chance(30)) out += "<caption>" + text(4) + "</caption>";
    const int columns = rng_.range(1, 6);
    const std::string section = rng_.chance(50) ? "" : rng_.chance(50) ? "thead" : "tbody";
    if (!section.empty()) out += "<" + section + ">";
    for (int r = 0, rows = rng_.range(0, 5); r < rows; ++r) {
      out += "<tr" + attributes() + ">";
      for (int c = 0; c < columns; ++c) {
        const std::string cell = rng_.chance(20) ? "th" : "td";
        out += "<" + cell + attributes();
        if (rng_.chance(10)) out += " colspan=\"" + std::to_string(rng_.range(0, 3)) + "\"";
        if (rng_.chance(8)) out += " rowspan=\"" + std::to_string(rng_.range(1, 3)) + "\"";
        out += ">";
        if (rng_.chance(85) || depth > 3) {
          out += inlineContent(depth + 1);
        } else {
          out += rng_.chance(30) ? table(depth + 1) : block(depth + 1);
          if (rng_.chance(40)) out += text(3);  // text after the nested block, still in the cell
        }
        out += "</" + cell + ">";
      }
      out += "</tr>";
    }
    if (!section.empty()) out += "</" + section + ">";
    return out + "</table>";
  }

  std::string block(int depth) {
    const int kind = depth > 3 ? rng_.range(0, 9) : rng_.range(0, 40);
    if (kind <= 14) return "<p" + attributes() + ">" + inlineContent(depth) + "</p>";
    if (kind <= 17) {
      const std::string tag = "h" + std::to_string(rng_.range(1, 6));
      return "<" + tag + attributes() + ">" + inlineContent(depth) + "</" + tag + ">";
    }
    if (kind <= 21) {
      const char* const tags[] = {"div", "blockquote", "section", "aside", "figure", "article", "nav"};
      const std::string tag = rng_.pick(tags);
      std::string out = "<" + tag + attributes();
      if (tag == "aside" && rng_.chance(50))
        out += " epub:type=\"footnote\" id=\"fn" + std::to_string(rng_.range(0, 9)) + "\"";
      out += ">";
      for (int i = 0, n = rng_.range(0, 3); i < n; ++i) out += block(depth + 1);
      if (rng_.chance(30)) out += inlineContent(depth + 1);  // text directly in the container
      return out + "</" + tag + ">";
    }
    if (kind <= 24) return list(depth);
    if (kind <= 27) return table(depth);
    if (kind == 28) return "<hr" + attributes() + "/>";
    if (kind == 29) return "<pre" + attributes() + ">" + text(8) + "\n" + text(8) + "\n\n  " + text(4) + "</pre>";
    if (kind == 30) return "<div role=\"doc-pagebreak\" id=\"page" + std::to_string(rng_.range(0, 99)) + "\"/>";
    if (kind == 31) return "<p" + attributes() + "></p>";
    if (kind == 32) return "<p>" + std::string(rng_.range(1, 3), ' ') + "</p>";
    if (kind == 33) {
      std::string out = "<div" + attributes() + ">";
      for (int i = 0, n = rng_.range(1, 3); i < n; ++i) {
        out += rng_.chance(80) ? "<img src=\"" + image() + "\"" + attributes() + "/>"
                               : "<svg><image xlink:href=\"" + image() + "\"/></svg>";
      }
      return out + (rng_.chance(30) ? text(4) : "") + "</div>";
    }
    if (kind == 34) {
      // A one-letter opening span: the drop cap shapes the parser looks for.
      return std::string("<p") + attributes() + "><span style=\"" +
             (rng_.chance(50) ? "float: left" : "font-size: 3em") + "\">" + rng_.pick(WORDS) + "</span>" +
             inlineContent(depth) + "</p>";
    }
    if (kind == 35) return "<dl><dt>" + text(2) + "</dt><dd>" + inlineContent(depth) + "</dd></dl>";
    return inlineContent(depth);  // text and inline elements straight in the block's parent
  }

  Rng rng_;
};

struct Case {
  std::string chapter;
  std::string css;
  uint64_t hash = 0;
};

// Pages hold cached image paths, so this one is fixed; nothing can be written there, so an image
// the header probe cannot size falls back to its alt text, as with no decoder.
constexpr const char* IMAGE_CACHE = "/nonexistent-chapter-corpus/img";

// The page in words, for CHAPTER_CORPUS_CASE: diff two trees' output to find the first change.
void dumpPage(const Page& page, const uint16_t paragraph, const uint16_t listItem, const uint32_t offset) {
  std::printf("PAGE p=%u li=%u off=%u\n", paragraph, listItem, offset);
  for (const auto& element : page.elements) {
    std::printf("  tag=%d x=%d y=%d", element->getTag(), element->xPos, element->yPos);
    if (element->getTag() == TAG_PageLine) {
      const auto& block = *static_cast<const PageLine&>(*element).getBlock();
      for (uint16_t i = 0; i < block.wordCount(); ++i) std::printf(" [%s@%d]", block.wordText(i), block.wordXpos(i));
    }
    std::printf("\n");
  }
  for (const auto& note : page.footnotes) std::printf("  note %s %s\n", note.number, note.href);
  for (const auto& link : page.links)
    std::printf("  link %s %d,%d %dx%d\n", link.href, link.x, link.y, link.width, link.height);
}

class ChapterCorpus {
 public:
  ChapterCorpus() {
    const auto dir = std::filesystem::temp_directory_path() / ("crosspoint-chapter-corpus-" + std::to_string(getpid()));
    std::filesystem::create_directories(dir);
    dir_ = dir.string();
  }
  ~ChapterCorpus() { std::filesystem::remove_all(dir_); }

  uint64_t run(uint64_t seed, bool keep) {
    Generator generator(seed);
    Rng rng(seed ^ 0x5A5A5A5AULL);
    const std::string css = generator.css();
    const std::string html = generator.chapter();
    const std::string chapterPath = dir_ + "/chapter.xhtml";
    const std::string cssPath = dir_ + "/style.css";
    const std::string pagePath = dir_ + "/page.bin";
    write(chapterPath, html);
    write(cssPath, css);
    if (keep) {
      const auto out = std::filesystem::temp_directory_path();
      write((out / ("chapter-corpus-" + std::to_string(seed) + ".xhtml")).string(), html);
      write((out / ("chapter-corpus-" + std::to_string(seed) + ".css")).string(), css);
    }

    GfxRenderer renderer;
    renderer.glyphSpread = rng.chance(75) ? rng.range(1, 9) : 0;
    const int fontId = rng.range(1, 4);
    renderer.fontMap.emplace(fontId, EpdFontFamily(nullptr));
    renderer.fontLineHeights[fontId] = rng.range(12, 34);
    renderer.fontAscenders[fontId] = renderer.fontLineHeights[fontId] * 3 / 4;
    if (rng.chance(70)) {
      // Sized variants with their own line heights, as the reader's font provider gives them.
      renderer.setFontVariantProvider({&renderer,
                                       [](void* ctx, const int font, const float scale) {
                                         auto& r = *static_cast<GfxRenderer*>(ctx);
                                         const int variant = font * 1000 + static_cast<int>(scale * 100 + 0.5f);
                                         if (r.fontMap.emplace(variant, EpdFontFamily(nullptr)).second) {
                                           r.fontLineHeights[variant] =
                                               static_cast<int>(r.getLineHeight(font) * scale + 0.5f);
                                           r.fontAscenders[variant] = r.fontLineHeights[variant] * 3 / 4;
                                         }
                                         return variant;
                                       },
                                       [](void* ctx, const int font) {
                                         static_cast<GfxRenderer*>(ctx)->fontMap.emplace(font, EpdFontFamily(nullptr));
                                         return true;
                                       }});
    }
    const char* const languages[] = {"en", "de", "tr", "fr", ""};
    Hyphenator::setPreferredLanguage(rng.pick(languages));

    CssParser cssParser{dir_};
    const bool useCss = rng.chance(80);
    if (useCss) {
      HalFile input;
      EXPECT_TRUE(input.open(cssPath.c_str(), "rb"));
      cssParser.loadFromStream(input);
    }
    std::vector<std::string> tocAnchors;
    for (const auto& id : generator.ids) {
      if (rng.chance(20)) tocAnchors.push_back(id);
    }

    const float compressions[] = {0.85f, 1.0f, 1.0f, 1.2f};
    Hash hash;
    ChapterHtmlSlimParser parser(
        std::make_shared<Epub>(), chapterPath, renderer, fontId, rng.pick(compressions), rng.chance(50),
        static_cast<uint8_t>(rng.range(0, 4)), static_cast<uint16_t>(rng.range(120, 481)),
        static_cast<uint16_t>(rng.range(150, 800)), rng.chance(50), rng.chance(15),
        [&](std::unique_ptr<Page> page, const uint16_t paragraph, const uint16_t listItem, const uint32_t offset) {
          hash.u32(0xFA6E);
          hash.u32(paragraph);
          hash.u32(listItem);
          hash.u32(offset);
          {
            HalFile file;
            ASSERT_TRUE(file.open(pagePath.c_str(), "wb"));
            hash.u32(page->serialize(file));
          }
          hash.str(read(pagePath));
          if (keep) dumpPage(*page, paragraph, listItem, offset);
        },
        rng.chance(80), "OEBPS/", IMAGE_CACHE, static_cast<uint8_t>(rng.range(0, 2)), tocAnchors,
        rng.chance(50) ? std::function<void()>([&hash] { hash.u32(0xB0B); }) : nullptr, useCss ? &cssParser : nullptr);
    parser.setTextSpacing(static_cast<int8_t>(rng.range(-2, 3)), static_cast<uint8_t>(rng.range(50, 150)));
    parser.setParagraphIndentSpaces(static_cast<uint8_t>(rng.range(0, 4)));

    const bool parsed = parser.parseAndBuildPages();
    hash.u32(parsed);
    failures += parsed ? 0 : 1;
    for (size_t i = 0; i < parser.anchorCount(); ++i) {
      uint16_t page = 0;
      hash.str(std::string(parser.anchorAt(i, page)));
      hash.u32(page);
    }
    return hash.value;
  }

 private:
  static void write(const std::string& path, const std::string& data) {
    HalFile file;
    ASSERT_TRUE(file.open(path.c_str(), "wb"));
    ASSERT_EQ(file.write(data.data(), data.size()), data.size());
  }

  static std::string read(const std::string& path) {
    std::FILE* file = std::fopen(path.c_str(), "rb");
    std::string data;
    if (!file) return data;
    char buffer[4096];
    for (size_t n; (n = std::fread(buffer, 1, sizeof(buffer), file)) > 0;) data.append(buffer, n);
    std::fclose(file);
    return data;
  }

  std::string dir_;

 public:
  size_t failures = 0;
};

constexpr size_t GROUPS = 16;
constexpr size_t CASES_PER_GROUP = 100;

// Recorded at 3e3f88e2, before the parser's flow moved to Rust.
constexpr uint64_t EXPECTED[GROUPS] = {
    0x4c75fc5f254a6a9fULL, 0x5aaa1426d6a03855ULL, 0x0c0d1529a08b38f2ULL, 0x7a5f506b54282db2ULL,
    0x99368716e448facbULL, 0x5491533fff0e4c98ULL, 0xedbe14ad09c818c1ULL, 0x84bd64f9d60b1f00ULL,
    0x351297423168bc7aULL, 0xea286f95bc287cc4ULL, 0xa09f1528cda70e9cULL, 0x4a82c407514b2726ULL,
    0x37b9377a570fe3b1ULL, 0x90d1415a9e666ad9ULL, 0x52f3c70739ade832ULL, 0xa641a491737d93bfULL,
};

TEST(ChapterCorpus, PagesMatchTheRecordedParser) {
  const bool record = std::getenv("CHAPTER_CORPUS_RECORD") != nullptr;
  const bool dump = std::getenv("CHAPTER_CORPUS_DUMP") != nullptr;
  const char* keepCase = std::getenv("CHAPTER_CORPUS_CASE");
  const long keep = keepCase ? std::atol(keepCase) : -1;
  ChapterCorpus corpus;
  if (keep >= 0) {
    std::printf("case %ld %016llx\n", keep, static_cast<unsigned long long>(corpus.run(keep, true)));
    GTEST_SKIP() << "wrote one case";
  }
  for (size_t group = 0; group < GROUPS; ++group) {
    Hash groupHash;
    for (size_t i = 0; i < CASES_PER_GROUP; ++i) {
      const uint64_t seed = group * CASES_PER_GROUP + i;
      const uint64_t caseHash = corpus.run(seed, static_cast<long>(seed) == keep);
      if (dump)
        std::printf("case %llu %016llx\n", static_cast<unsigned long long>(seed),
                    static_cast<unsigned long long>(caseHash));
      groupHash.bytes(&caseHash, sizeof(caseHash));
    }
    if (record) {
      if (group + 1 == GROUPS)
        std::printf("// %zu of %zu chapters failed to parse\n", corpus.failures, GROUPS * CASES_PER_GROUP);
      std::printf("    0x%016llxULL,\n", static_cast<unsigned long long>(groupHash.value));
    } else {
      EXPECT_EQ(groupHash.value, EXPECTED[group]) << "group " << group << " (cases " << group * CASES_PER_GROUP << "-"
                                                  << (group + 1) * CASES_PER_GROUP - 1 << ")";
    }
  }
}

}  // namespace
