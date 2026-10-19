// What ParsedText lays out for a generated corpus of paragraphs: Latin, Turkish and German words
// (hyphenation in three languages), soft hyphens, dashes, no-break spaces, NFD marks, CJK,
// Korean, Arabic and Hebrew, emoji; every style, underline, links, ruby (single and grouped),
// inline font sizes, glued and gap-less tokens, Focus Reading, every alignment and direction,
// text indents, letter and word spacing, SD-card fonts, and paragraphs laid out in several passes
// (a soft flush without the last line, a line limit, words added in between). The metrics stub
// in stubs/GfxRenderer.h varies by codepoint, style and font and kerns pairs, so the hash pins
// every measurement. Each emitted line reports its words, positions, styles, focus splits, source
// ranges, fonts, ruby and links. The reports hash to values recorded from the C++ ParsedText
// before the Rust port. Rerun with PARSED_TEXT_PRINT=1 to list the cases. The hashes freeze the
// corpus: add new cases as a new test.

#include <Epub/ParsedText.h>
#include <Epub/blocks/TextBlock.h>
#include <Epub/hyphenation/Hyphenator.h>
#include <GfxRenderer.h>
#include <gtest/gtest.h>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <memory>
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

const char* const kLatin[] = {"the",
                              "quick",
                              "brown",
                              "fox",
                              "a",
                              "I",
                              "extraordinary",
                              "internationalization",
                              "reconnaissance",
                              "hyphenation",
                              "incomprehensibilities",
                              "co-operate",
                              "well\xE2\x80\x94known",
                              "e.g.",
                              "U.S.A.",
                              "(aside)",
                              "\xE2\x80\x9Cquoted\xE2\x80\x9D",
                              "don\xE2\x80\x99t",
                              "123,456.78",
                              "x\xC2\xB2",
                              "word,",
                              "end.",
                              "why?",
                              "!",
                              "...",
                              "\xE2\x80\x94",
                              "\xE2\x80\x93",
                              "A-B-C",
                              "--",
                              "self-evident",
                              "pre\xE2\x80\x91war",
                              "WWWWWWWWWWWWWWWWWWWWWWWW",
                              "mmmmmmmmmmmmmmmmmm"};
const char* const kTurkish[] = {
    "\xC3\xA7\x65koslovakyal\xC4\xB1la\xC5\x9Ft\xC4\xB1ramad\xC4\xB1klar\xC4\xB1m\xC4\xB1zdanm\xC4\xB1\xC5\x9Fs\xC4\xB1"
    "n\xC4\xB1z",
    "\xC4\xB0stanbul",
    "a\xC4\x9F\x61\xC3\xA7",
    "\xC3\xB6\xC4\x9Fretmenlerimizden",
    "kitaplar\xC4\xB1m\xC4\xB1zdan",
    "\xC3\xBC\x6Eiversitesinde"};
const char* const kGerman[] = {"Fu\xC3\x9Fg\xC3\xA4nger", "Quadratkilometer", "Donaudampfschiffahrt",
                               "Stra\xC3\x9F\x65nbahnhaltestelle", "na\xC3\xAFv"};
const char* const kSpecial[] = {"hy\xC2\xADphen\xC2\xAD\x61tion",
                                "soft\xC2\xAD",
                                "\xC2\xAD\x61",
                                "e\xCC\x81te\xCC\x81",
                                "Tie\xCC\x82\xCC\x81ng Vie\xCC\xA3t",
                                "\xCC\x81",
                                "\xF0\x9F\x98\x80",
                                "a\xF0\x9F\x98\x80"
                                "b"};
const char* const kCjk[] = {
    "\xE6\x97\xA5\xE6\x9C\xAC\xE8\xAA\x9E\xE3\x81\xAE\xE6\x96\x87\xE7\xAB\xA0\xE3\x81\xA7\xE3\x81\x99\xE3\x80\x82",
    "\xE4\xB8\xAD\xE6\x96\x87\xEF\xBC\x8C\xE6\xB5\x8B\xE8\xAF\x95\xEF\xBC\x81",
    "\xE3\x80\x8C\xE5\xBC\x95\xE7\x94\xA8\xE3\x80\x8D\xE3\x81\xA7\xE3\x81\x99",
    "\xE6\xBC\xA2\xE5\xAD\x97",
    "\xE3\x81\x8B\xE3\x81\xAA",
    "\xEF\xBC\x88\xE6\xB3\xA8\xEF\xBC\x89",
    "abc\xE6\x97\xA5\xE6\x9C\xAC"
    "def"};
const char* const kKorean[] = {"\xED\x95\x9C\xEA\xB5\xAD\xEC\x96\xB4",
                               "\xEB\xAC\xB8\xEC\x9E\xA5\xEC\x9E\x85\xEB\x8B\x88\xEB\x8B\xA4.",
                               "\xEC\x95\x88\xEB\x85\x95\xED\x95\x98\xEC\x84\xB8\xEC\x9A\x94",
                               "\xEB\x8C\x80\xED\x95\x9C\xEB\xAF\xBC\xEA\xB5\xAD\xEC\x9D\x98\xED\x97\x8C\xEB\xB2\x95"};
const char* const kRtl[] = {"\xD9\x85\xD8\xB1\xD8\xAD\xD8\xA8\xD8\xA7",
                            "\xD8\xA7\xD9\x84\xD8\xB9\xD8\xB1\xD8\xA8\xD9\x8A\xD8\xA9",
                            "\xD7\xA9\xD7\x9C\xD7\x95\xD7\x9D",
                            "\xD7\xA2\xD7\x91\xD7\xA8\xD7\x99\xD7\xAA",
                            "(\xD9\x85\xD8\xB1\xD8\xAD\xD8\xA8\xD8\xA7)",
                            "123\xD9\x85",
                            "\xD9\xA1\xD9\xA2\xD9\xA3"};
const char* const kRuby[] = {"\xE3\x81\xB5\xE3\x82\x8A\xE3\x81\x8C\xE3\x81\xAA", "\xE3\x81\x8B",
                             "\xE3\x81\x8B\xE3\x82\x93\xE3\x81\x98\xE3\x81\x8B\xE3\x82\x93\xE3\x81\x98", "ab"};
const char* const kHrefs[] = {"#note1", "chapter2.xhtml#x", "notes.xhtml#n12", "a"};
const char* const kLanguages[] = {"en", "tr", "de", "", "en-US"};
const uint8_t kStyles[] = {EpdFontFamily::REGULAR,
                           EpdFontFamily::REGULAR,
                           EpdFontFamily::REGULAR,
                           EpdFontFamily::BOLD,
                           EpdFontFamily::ITALIC,
                           EpdFontFamily::BOLD_ITALIC,
                           EpdFontFamily::SUP,
                           EpdFontFamily::SUB,
                           EpdFontFamily::SMALL_CAPS,
                           EpdFontFamily::STRIKETHROUGH,
                           EpdFontFamily::BOLD | EpdFontFamily::SMALL_CAPS};
const CssTextAlign kAligns[] = {CssTextAlign::Justify, CssTextAlign::Left, CssTextAlign::Center, CssTextAlign::Right,
                                CssTextAlign::None};
const uint16_t kWidths[] = {30, 60, 90, 140, 200, 300, 460};
const float kScales[] = {0.75f, 1.2f, 1.5f, 0.5f, 2.0f, 1.21f};

std::string word(Rng& rng, const uint32_t script) {
  switch (script) {
    case 0:
      return rng.pick(kTurkish);
    case 1:
      return rng.pick(kGerman);
    case 2:
      return rng.pick(kSpecial);
    case 3:
      return rng.pick(kCjk);
    case 4:
      return rng.pick(kKorean);
    case 5:
      return rng.pick(kRtl);
    default:
      return rng.pick(kLatin);
  }
}

void reportLine(std::string& out, TextBlock& block, const uint32_t lineOffset, const int linesAfter,
                const int sectionFont) {
  const auto& style = block.getBlockStyle();
  out += "L " + std::to_string(lineOffset) + " " + std::to_string(linesAfter) + " rtl" + std::to_string(style.isRtl) +
         " a" + std::to_string(static_cast<int>(style.alignment)) + "\n";
  for (uint16_t i = 0; i < block.wordCount(); ++i) {
    const auto range = block.wordSourceRange(i);
    out += " " + escaped(block.wordText(i)) + " x" + std::to_string(block.wordXpos(i)) + " s" +
           std::to_string(static_cast<int>(block.wordStyle(i))) + (block.wordHasDiscretionaryHyphen(i) ? " dh" : "") +
           " f" + std::to_string(block.focusBoundary(i)) + "/" + std::to_string(block.focusSuffixX(i)) + " r" +
           std::to_string(range.start) + "-" + std::to_string(range.end) + " font" +
           std::to_string(block.wordFontId(i, sectionFont)) + (block.wordStartsParagraph(i) ? " P" : "") + "\n";
  }
  for (const auto& ruby : block.getRubyTexts()) out += " ruby " + escaped(ruby) + "\n";
  for (const auto& link : block.takeLinkSpans()) {
    out += " link " + std::string(link.href) + " " + std::to_string(link.x) + " " + std::to_string(link.width) + " " +
           std::to_string(link.topLift) + "\n";
  }
}

// One generated paragraph, laid out in one to three passes; returns the report.
std::string paragraph(Rng& rng) {
  std::string out;
  GfxRenderer renderer;
  renderer.sdFont = rng.chance(15);
  Hyphenator::setPreferredLanguage(rng.pick(kLanguages));

  BlockStyle style;
  style.alignment = kAligns[rng.below(5)];
  style.textAlignDefined = rng.chance(40);
  style.directionDefined = rng.chance(20);
  style.isRtl = rng.chance(15);
  style.textIndentDefined = rng.chance(30);
  style.textIndent = static_cast<int16_t>(static_cast<int>(rng.below(50)) - 20);
  const bool hyphenation = rng.chance(50);
  const bool focus = rng.chance(20);
  const uint8_t indentSpaces = static_cast<uint8_t>(rng.below(4));
  ParsedText text(hyphenation, focus, style, indentSpaces);

  const int sectionFont = rng.chance(20) ? 3 : 0;
  const int8_t characterSpacing = static_cast<int8_t>(static_cast<int>(rng.below(6)) - 2);
  const uint8_t wordSpacing = static_cast<uint8_t>(rng.chance(60) ? 100 : 50 + rng.below(151));
  const uint16_t width = kWidths[rng.below(sizeof(kWidths) / sizeof(kWidths[0]))];
  const uint32_t mainScript = rng.below(10);
  uint32_t offset = rng.chance(5) ? 70000 : rng.below(1000);

  const auto addWords = [&](const uint32_t count) {
    for (uint32_t w = 0; w < count; ++w) {
      const uint32_t script = rng.chance(70) ? mainScript : rng.below(10);
      std::string token = rng.chance(4) ? std::string("\xC2\xA0") : word(rng, script);
      if (rng.chance(3)) token = " ";
      const auto fontStyle = static_cast<EpdFontFamily::Style>(rng.chance(75) ? 0 : kStyles[rng.below(11)]);
      const bool underline = rng.chance(8);
      const bool attach = rng.chance(15);
      const uint8_t linkId = rng.chance(6) ? text.addLinkTarget(rng.pick(kHrefs)) : 0;
      uint8_t sizeSlot = 0;
      if (rng.chance(8)) {
        sizeSlot = text.sizeSlotFor(kScales[rng.below(6)]);
        if (rng.chance(80)) text.setSizeSlotFontId(sizeSlot, static_cast<int32_t>(5 + sizeSlot * 2));
      }
      const bool noSpace = rng.chance(4);
      text.addWord(token, fontStyle, underline, attach, offset, linkId, sizeSlot, noSpace);
      offset += static_cast<uint32_t>(token.size()) + 1 + (rng.chance(1) ? 70000 : 0);
    }
    if (!text.isEmpty() && rng.chance(12)) {
      const size_t at = rng.below(static_cast<uint32_t>(text.size()));
      if (rng.chance(50)) {
        text.setRubyForWordAt(at, rng.pick(kRuby));
      } else {
        text.setRubyGroupAt(at, 1 + rng.below(3), rng.pick(kRuby));
      }
    }
  };

  const auto layout = [&](const bool includeLast, const size_t maxLines) {
    text.layoutAndExtractLines(
        renderer, sectionFont, width,
        [&](std::unique_ptr<TextBlock> block, const uint32_t lineOffset) {
          reportLine(out, *block, lineOffset, text.linesAfterCurrentLine(), sectionFont);
        },
        includeLast, characterSpacing, wordSpacing, maxLines);
    out += "pass " + std::to_string(text.size()) + " " + std::to_string(text.hadDroppedWords()) + "\n";
  };

  addWords(1 + rng.below(rng.chance(20) ? 120 : 30));
  if (rng.chance(10)) text.suppressFirstLineIndent();
  const uint32_t passes = rng.below(3);
  for (uint32_t p = 0; p < passes; ++p) {
    layout(false, rng.chance(30) ? 1 + rng.below(3) : SIZE_MAX);
    if (rng.chance(50)) addWords(1 + rng.below(15));
  }
  layout(true, SIZE_MAX);
  if (renderer.sdFont) out += "cold " + std::to_string(renderer.coldMeasurements) + "\n";
  return out;
}

// Recorded from the C++ ParsedText as of 6911d85.
constexpr uint64_t kHash = 0x92AE4B591932C04EULL;

}  // namespace

TEST(ParsedTextCorpus, LayoutsMatchTheRecordedParsedText) {
  const bool print = std::getenv("PARSED_TEXT_PRINT") != nullptr;
  Rng rng{0x5EED0FA11ULL};
  Hash hash;
  for (int i = 0; i < 12000; ++i) {
    const std::string report = paragraph(rng);
    hash.add(report);
    if (print) std::printf("--- %d\n%s", i, report.c_str());
  }
  Hyphenator::setPreferredLanguage("");
  EXPECT_EQ(hash.value, kHash) << "0x" << std::hex << std::uppercase << hash.value << "ULL";
}
