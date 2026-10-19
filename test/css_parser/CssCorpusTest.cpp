// CssParser over a generated corpus: 16 batches of pseudo-random stylesheets and inline styles,
// drawn from every property, value form, selector shape and stream construct the parser knows
// (comments, at-rules, nested and stray braces, oversized selectors and declarations), plus
// lookups with ancestors and ::first-letter, a cache round trip and corrupted caches. Each
// batch's answers hash to a value recorded from the C++ implementation. A changed hash names
// the batch; rerun with CSS_CORPUS_PRINT=<batch> to list its cases.
//
// Left out on purpose: initial-letter values of 2^32 and more and font-weight integers past
// 32 bits, where the host and the device read the same text differently (a 64-bit long, and an
// out-of-range float-to-integer conversion).

#include <gtest/gtest.h>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

#include "CssParser.h"

namespace fs = std::filesystem;

namespace {

constexpr size_t kBatches = 16;
constexpr size_t kSheetsPerBatch = 48;
constexpr size_t kInlinePerBatch = 300;

// Recorded from the C++ implementation (lib/Epub/Epub/css/CssParser.cpp at 09112bb).
constexpr uint64_t kBatchHashes[kBatches] = {
    0x51FFAA55D58C8178ULL, 0xFB208B1EBCBB72BBULL, 0x95D0796B1CADFD5FULL, 0xF72C9FB250140049ULL,
    0x64D5FC442B8CE4B0ULL, 0xDCFA930FFA5A14E5ULL, 0x6C65D186FFB13287ULL, 0x5C185C3E1AD2BE2EULL,
    0x02CBFC72D9984BF9ULL, 0x52B4350C486EF960ULL, 0xBA9D06467807DA41ULL, 0xE0C812E4FBA4B490ULL,
    0xCD57F05D3997D72CULL, 0x085AC23074AE3160ULL, 0x324EE8F298B1638DULL, 0xBEEAB778381574BEULL};

struct Rng {
  uint64_t state;
  uint32_t next() {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    return static_cast<uint32_t>(state >> 32);
  }
  uint32_t below(const uint32_t n) { return next() % n; }
  bool chance(const uint32_t n) { return below(n) == 0; }
  template <size_t N>
  const char* pick(const char* const (&items)[N]) {
    return items[below(N)];
  }
};

struct Hash {
  uint64_t value = 0xCBF29CE484222325ULL;  // FNV-1a
  void add(const void* data, const size_t len) {
    for (size_t i = 0; i < len; ++i) {
      value ^= static_cast<const uint8_t*>(data)[i];
      value *= 0x100000001B3ULL;
    }
  }
  void add(const std::string& s) { add(s.data(), s.size() + 1); }
  void add(const uint32_t word) { add(&word, sizeof word); }
};

// Every field, one by one: the struct's padding bytes are not part of the answer.
std::string describe(const CssStyle& s) {
  std::string out;
  char b[64];
  const auto num = [&](const unsigned v) {
    snprintf(b, sizeof b, "%u ", v);
    out += b;
  };
  const auto len = [&](const CssLength& l) {
    uint32_t bits = 0;
    memcpy(&bits, &l.value, sizeof bits);
    snprintf(b, sizeof b, "%08x/%u ", bits, static_cast<unsigned>(l.unit));
    out += b;
  };
  num(static_cast<unsigned>(s.textAlign));
  num(static_cast<unsigned>(s.fontStyle));
  num(static_cast<unsigned>(s.fontWeight));
  num(static_cast<unsigned>(s.textDecoration));
  num(static_cast<unsigned>(s.direction));
  for (const CssLength* l :
       {&s.textIndent, &s.marginTop, &s.marginBottom, &s.marginLeft, &s.marginRight, &s.paddingTop, &s.paddingBottom,
        &s.paddingLeft, &s.paddingRight, &s.imageHeight, &s.imageWidth, &s.fontSize}) {
    len(*l);
  }
  num(static_cast<unsigned>(s.display));
  num(static_cast<unsigned>(s.verticalAlign));
  num(static_cast<unsigned>(s.listStyleType));
  num(s.smallCaps);
  num(s.pageBreakBefore);
  num(s.pageBreakAfter);
  for (const CssBorderSide* side : {&s.borderTop, &s.borderRight, &s.borderBottom, &s.borderLeft}) {
    num(side->width);
    num(static_cast<unsigned>(side->style));
  }
  num(s.borderWidthsDefined);
  num(s.borderStylesDefined);
  num(s.shaded);
  num(s.floatLeft);
  num(s.initialLetter);
  num(s.preserveWhitespace);
  const CssPropertyFlags& d = s.defined;
  const unsigned flags[] = {d.textAlign,     d.fontStyle,       d.fontWeight,     d.textDecoration, d.textIndent,
                            d.marginTop,     d.marginBottom,    d.marginLeft,     d.marginRight,    d.paddingTop,
                            d.paddingBottom, d.paddingLeft,     d.paddingRight,   d.imageHeight,    d.imageWidth,
                            d.display,       d.direction,       d.verticalAlign,  d.listStyleType,  d.fontSize,
                            d.smallCaps,     d.pageBreakBefore, d.pageBreakAfter, d.borderTop,      d.borderRight,
                            d.borderBottom,  d.borderLeft,      d.shaded,         d.floatLeft,      d.initialLetter,
                            d.whiteSpace};
  for (const unsigned f : flags) out += static_cast<char>('0' + f);
  return out;
}

// ---- Values ----

const char* const kUnits[] = {"", "", "px", "em", "EM", "rem", "pt", "%", "ex", "ch", "vh", "in", "e5", "Px"};
const char* const kOddNumbers[] = {
    "0", "1", "1.5", ".5", "5.", "-2", "+3", "+-1", "--1", "1.2.3", "007", "0.83", "1e2", "inf", "nan", "100", "700",
    "650", "-0", "+.25", ".", "-", "+", "12.5.", "1..2", "0.1", "0.3", "2.675", "16", "-.75", "3.0001", "99999999",
    // past f32 precision, the largest finite value and the rounding edges around it and around zero
    "123456789012345678901234567890", "16777217", "33554433.0000001", "0.000000000000000000000000000000000000000000001",
    "0.0000000000000000000000000000000000000000000007", "0.0000000000000000000000000000000000000000000008",
    "340282346638528859811704183484516925440", "340282356779733661637539395458142568448",
    "340282356779733661637539395458142568447", "0.000000000000000000000000000000000000011754943508222875",
    "1.00000005960464477539062500000000000000000000000001", "1.000000059604644775390625"};

std::string randomDigits(Rng& rng) {
  std::string s;
  const uint32_t digits = 1 + rng.below(rng.chance(4) ? 30 : 6);
  const uint32_t point = rng.below(digits + 2);
  for (uint32_t i = 0; i < digits; ++i) {
    if (i == point) s += '.';
    s += static_cast<char>('0' + rng.below(10));
  }
  if (rng.chance(8)) s.insert(0, "-");
  return s;
}

std::string number(Rng& rng) { return rng.chance(3) ? rng.pick(kOddNumbers) : randomDigits(rng); }

std::string length(Rng& rng) { return number(rng) + rng.pick(kUnits); }

const char* const kKeywords[] = {
    "auto",     "inherit",   "initial",   "none",         "normal",   "bold",         "bolder",
    "lighter",  "italic",    "oblique",   "left",         "right",    "center",       "justify",
    "start",    "end",       "underline", "line-through", "overline", "small-caps",   "all-small-caps",
    "always",   "page",      "avoid",     "recto",        "verso",    "super",        "sub",
    "baseline", "rtl",       "ltr",       "pre",          "pre-wrap", "break-spaces", "nowrap",
    "thin",     "medium",    "thick",     "solid",        "dotted",   "dashed",       "double",
    "groove",   "hidden",    "xx-small",  "x-small",      "small",    "large",        "x-large",
    "xx-large", "xxx-large", "smaller",   "larger",       "block",    "inline",       "LEFT",
    "Bold",     "ITALIC",    "None",      "Underline",    "RTL",      "Solid",        "Small-Caps",
    "Always"};
const char* const kColors[] = {"white",
                               "gray",
                               "silver",
                               "black",
                               "transparent",
                               "lavender",
                               "#fff",
                               "#eee",
                               "#ccc",
                               "#abcd",
                               "#a0a0a0",
                               "#a0a0a080",
                               "#ggg",
                               "#12345",
                               "rgb(200,200,200)",
                               "rgb(50%, 60%, 70%)",
                               "rgba(1,2,3,0.5)",
                               "rgb(300, 1000, 2)",
                               "rgb(1,2)",
                               "RGB(9,9,9)",
                               "Gainsboro",
                               "whitesmoke",
                               "dimgrey",
                               "beige",
                               "#",
                               "url(x.png) #ddd",
                               "#DDD no-repeat"};
const char* const kInitialLetters[] = {"1",      "2",   "3.4",    "2.5", "0.5",  "7",          "100", "-1",
                                       "abc",    "2 3", "nan",    "1e1", "+2",   "0.99999999", "6.5", "1.49999999",
                                       "3 drop", "x 2", "4.4999", "",    "1e-1", "05"};
const char* const kWeights[] = {"100",    "400",  "600",        "699",         "700",   "701",    "900",
                                "+800",   "-700", "7e2",        "700.0",       "bold",  "Bolder", "lighter",
                                "normal", "1000", "2147483647", "-2147483648", " 700 ", "7OO"};

std::string value(Rng& rng) {
  std::string out;
  const uint32_t tokens = 1 + rng.below(rng.chance(3) ? 5 : 2);
  for (uint32_t i = 0; i < tokens; ++i) {
    if (i) out += rng.chance(6) ? "\t" : " ";
    switch (rng.below(5)) {
      case 0:
        out += rng.pick(kKeywords);
        break;
      case 1:
        out += rng.pick(kColors);
        break;
      default:
        out += length(rng);
        break;
    }
  }
  return out;
}

const char* const kProperties[] = {"text-align",
                                   "font-style",
                                   "font-weight",
                                   "text-decoration",
                                   "text-decoration-line",
                                   "text-indent",
                                   "margin",
                                   "margin-top",
                                   "margin-bottom",
                                   "margin-left",
                                   "margin-right",
                                   "padding",
                                   "padding-top",
                                   "padding-bottom",
                                   "padding-left",
                                   "padding-right",
                                   "height",
                                   "width",
                                   "display",
                                   "direction",
                                   "vertical-align",
                                   "font-size",
                                   "font-variant",
                                   "font-variant-caps",
                                   "page-break-before",
                                   "break-before",
                                   "page-break-after",
                                   "break-after",
                                   "border",
                                   "border-width",
                                   "border-style",
                                   "border-top",
                                   "border-right-width",
                                   "border-bottom-style",
                                   "border-left",
                                   "border-top-color",
                                   "borders",
                                   "border-",
                                   "border-x",
                                   "background",
                                   "background-color",
                                   "float",
                                   "initial-letter",
                                   "list-style-type",
                                   "white-space",
                                   "color",
                                   "-webkit-hyphens",
                                   "TEXT-ALIGN",
                                   "Font-Weight",
                                   "MARGIN"};

std::string declaration(Rng& rng) {
  const std::string name = rng.pick(kProperties);
  std::string v;
  if (name == "initial-letter") {
    v = rng.pick(kInitialLetters);
  } else if (name == "font-weight" || name == "Font-Weight") {
    v = rng.chance(4) ? value(rng) : rng.pick(kWeights);
  } else if (name.rfind("background", 0) == 0 || (rng.chance(4) && name.rfind("border", 0) == 0)) {
    v = rng.pick(kColors);
    if (rng.chance(2)) v = length(rng) + " " + rng.pick(kKeywords) + " " + v;
  } else {
    v = value(rng);
  }
  if (rng.chance(8)) v += rng.pick({" !important", "!important", " ! important", "!IMPORTANT  ", " !important;x"});
  switch (rng.below(12)) {
    case 0:
      return name + v;  // no colon
    case 1:
      return ":" + v;
    case 2:
      return name + " : ";
    case 3:
      return " " + name + "\t:\n" + v + " ";
    default:
      return name + ":" + (rng.chance(2) ? " " : "") + v;
  }
}

std::string declarations(Rng& rng) {
  std::string out;
  const uint32_t count = rng.below(6);
  for (uint32_t i = 0; i < count; ++i) {
    if (i) out += rng.chance(8) ? ";;" : ";";
    out += declaration(rng);
  }
  if (rng.chance(2)) out += ";";
  return out;
}

// ---- Selectors ----

const char* const kTags[] = {"p", "div", "span", "h1", "li", "P", "Div", "*", "", "", "", "blockquote"};
const char* const kClasses[] = {"a", "b", "c", "Note", "note", "x-y", "\xC3\xA9", "B", "_u", "c1"};
const char* const kIds[] = {"main", "x", "MAIN", "y2"};

std::string compound(Rng& rng) {
  std::string out = rng.pick(kTags);
  if (rng.chance(4)) out += std::string("#") + rng.pick(kIds);
  const uint32_t classes = rng.below(rng.chance(4) ? 6 : 3);
  for (uint32_t i = 0; i < classes; ++i) out += std::string(".") + rng.pick(kClasses);
  if (out.empty()) out = rng.pick(kTags);
  if (rng.chance(40)) out += rng.pick({"..a", ".", "#", "##a", "#a#b", "[x]", ":hover", "+p", "~p", "!"});
  return out;
}

std::string selector(Rng& rng) {
  std::string out = compound(rng);
  const uint32_t ancestors = rng.below(rng.chance(3) ? 4 : 1);
  for (uint32_t i = 0; i < ancestors; ++i) {
    out = compound(rng) + rng.pick({" ", " > ", ">", "  ", "\t", " >", "> ", " > > "}) + out;
  }
  if (rng.chance(10))
    out += rng.pick({"::first-letter", ":first-letter", "::FIRST-LETTER", "::before", ":first-letter "});
  if (rng.chance(30)) out = rng.pick({" ", "", "> p", "p >", "::first-letter", "* ", " p "}) + out;
  return out;
}

std::string selectorGroup(Rng& rng) {
  std::string out = selector(rng);
  const uint32_t extra = rng.below(rng.chance(3) ? 4 : 1);
  for (uint32_t i = 0; i < extra; ++i) out += rng.pick({",", ", ", " ,\n", ",,"}) + selector(rng);
  if (rng.chance(60)) out += "." + std::string(250 + rng.below(20), 'k');
  if (rng.chance(200)) out = "." + std::string(1030, 'w');
  return out;
}

// ---- Stylesheets ----

std::string sheet(Rng& rng) {
  std::string out;
  const uint32_t rules = rng.below(rng.chance(4) ? 40 : 12);
  for (uint32_t i = 0; i < rules; ++i) {
    switch (rng.below(24)) {
      case 0:
        out += "@media print { p { font-weight: bold } .a { margin: 1em } }\n";
        break;
      case 1:
        out += "@import url(\"x.css\");\n";
        break;
      case 2:
        out += "@font-face { font-family: x; src: url(a.ttf) }";
        break;
      case 3:
        out += "@charset \"utf-8\";";
        break;
      case 4:
        out += "/* comment { p { font-weight: bold } */";
        break;
      case 5:
        out += "}";  // stray close
        break;
      case 6:
        out += selectorGroup(rng) + " { " + declarations(rng) + " { nested: 1; } " + declarations(rng) + "}";
        break;
      case 7:
        out += selectorGroup(rng) + "/**/{/* c */" + declarations(rng) + "/*x*/}";
        break;
      case 8:
        out += selectorGroup(rng) + " { " + declaration(rng) + std::string(1030, 'v') + "; " + declarations(rng) + "}";
        break;
      case 9:
        out += "a/b { font-style: italic } p { width: 1/2em }";
        break;
      default:
        out += selectorGroup(rng) + rng.pick({" {", "{", "\n{\n"}) + declarations(rng) + rng.pick({"}", " }\n", "\n}"});
        break;
    }
    out += rng.pick({"\n", " ", "", "\r\n"});
  }
  if (rng.chance(10)) out += rng.pick({"/* open", ".tail { font-weight: bold", "@media x {", ".dangling", "p /", "/"});
  return out;
}

// ---- Lookups ----

std::string classAttr(Rng& rng) {
  std::string out;
  const uint32_t count = rng.below(rng.chance(4) ? 10 : 4);
  for (uint32_t i = 0; i < count; ++i) {
    out += rng.pick({" ", "  ", "\t", "\n"});
    out += rng.pick(kClasses);
  }
  return out;
}

struct Query {
  std::string tag, classes, id;
  std::vector<CssAncestor> ancestors;
  bool firstLetter;
  std::string text;
};

Query query(Rng& rng) {
  Query q;
  q.tag = rng.pick({"p", "div", "span", "h1", "li", "P", "blockquote", "td"});
  q.classes = classAttr(rng);
  q.id = rng.chance(3) ? rng.pick(kIds) : "";
  q.firstLetter = rng.chance(4);
  q.text = q.tag + "|" + q.classes + "|" + q.id + "|" + (q.firstLetter ? "fl" : "") + "|";
  const uint32_t depth = rng.below(7);
  for (uint32_t i = 0; i < depth; ++i) {
    const std::string tag = rng.pick({"div", "p", "blockquote", "section", "body", "DIV", "span"});
    const std::string classes = classAttr(rng);
    const std::string id = rng.chance(4) ? rng.pick(kIds) : "";
    q.ancestors.push_back(CssParser::makeAncestor(tag, classes, id));
    q.text += tag + "." + classes + "#" + id + " ";
  }
  return q;
}

std::vector<uint8_t> readFile(const fs::path& path) {
  std::ifstream input(path, std::ios::binary);
  return {std::istreambuf_iterator<char>(input), std::istreambuf_iterator<char>()};
}

void writeFile(const fs::path& path, const void* data, const size_t size) {
  std::ofstream output(path, std::ios::binary | std::ios::trunc);
  output.write(static_cast<const char*>(data), static_cast<std::streamsize>(size));
}

class Recorder {
 public:
  Recorder(Hash& hash, const bool print) : hash_(hash), print_(print) {}
  void note(const std::string& label, const std::string& answer) {
    hash_.add(answer);
    if (print_) printf("%s => %s\n", label.c_str(), answer.c_str());
  }

 private:
  Hash& hash_;
  bool print_;
};

std::string resolve(const CssParser& parser, const Query& q) {
  return describe(parser.resolveStyle(q.tag, q.classes, q.id, q.ancestors.data(), q.ancestors.size(), q.firstLetter));
}

uint64_t runBatch(const size_t batch, const bool print) {
  const fs::path dir = fs::temp_directory_path() / "crosspoint_css_corpus" / std::to_string(batch);
  fs::remove_all(dir);
  fs::create_directories(dir);
  Rng rng{0x9E3779B97F4A7C15ULL * (batch + 1)};
  Hash hash;
  Recorder rec(hash, print);

  for (size_t n = 0; n < kInlinePerBatch; ++n) {
    const std::string decls = declarations(rng);
    rec.note("inline " + decls, describe(CssParser::parseInlineStyle(decls)));
  }

  for (size_t n = 0; n < kSheetsPerBatch; ++n) {
    CssParser parser(dir.string());
    parser.deleteCache();
    // One to three stylesheets accumulate in one parser.
    const uint32_t sheets = 1 + rng.below(rng.chance(4) ? 3 : 1);
    for (uint32_t s = 0; s < sheets; ++s) {
      const std::string css = sheet(rng);
      writeFile(dir / "in.css", css.data(), css.size());
      HalFile source;
      Storage.openFileForRead("TST", (dir / "in.css").string(), source);
      const auto result = parser.loadFromStream(source);
      rec.note("sheet " + css, std::to_string(static_cast<int>(result)) + " " + std::to_string(parser.ruleCount()) +
                                   " " + std::to_string(parser.hasFirstLetterRules()));
    }

    std::vector<Query> queries;
    for (int i = 0; i < 24; ++i) {
      queries.push_back(query(rng));
      rec.note("  query " + queries.back().text, resolve(parser, queries.back()));
    }

    const bool complete = rng.chance(2);
    // Each call in its own statement: the operands of + are evaluated in no fixed order.
    const bool saved = parser.saveToCache(complete);
    const auto status = parser.inspectCache();
    rec.note("  save", std::to_string(saved) + " " + std::to_string(static_cast<int>(status)));
    const std::vector<uint8_t> cache = readFile(dir / "css_rules.cache");
    std::string bytes;
    for (const uint8_t b : cache) {
      char h[3];
      snprintf(h, sizeof h, "%02x", b);
      bytes += h;
    }
    rec.note("  cache", bytes);

    CssParser reader(dir.string());
    const auto loaded = reader.loadFromCache();
    rec.note("  reload", std::to_string(static_cast<int>(loaded)) + " " + std::to_string(reader.ruleCount()));
    for (const Query& q : queries) rec.note("  requery " + q.text, resolve(reader, q));

    // Corrupted caches: a truncation, flipped bytes, a changed length field.
    for (int m = 0; m < 4 && !cache.empty(); ++m) {
      std::vector<uint8_t> bad = cache;
      std::string what;
      switch (rng.below(3)) {
        case 0:
          bad.resize(rng.below(static_cast<uint32_t>(bad.size())));
          what = "truncate " + std::to_string(bad.size());
          break;
        case 1: {
          const uint32_t at = rng.below(static_cast<uint32_t>(bad.size()));
          bad[at] ^= static_cast<uint8_t>(1 + rng.below(255));
          what = "flip " + std::to_string(at);
          break;
        }
        default:
          bad.push_back(static_cast<uint8_t>(rng.next()));
          what = "extend";
          break;
      }
      writeFile(dir / "css_rules.cache", bad.data(), bad.size());
      CssParser broken(dir.string());
      const auto inspected = broken.inspectCache();
      const auto hydrated = broken.loadFromCache();
      std::string answer = std::to_string(static_cast<int>(inspected)) + " " +
                           std::to_string(static_cast<int>(hydrated)) + " " + std::to_string(broken.ruleCount());
      for (size_t i = 0; i < queries.size(); i += 6) answer += " " + resolve(broken, queries[i]);
      rec.note("  corrupt " + what, answer);
    }
  }
  fs::remove_all(dir);
  return hash.value;
}

TEST(CssCorpusTest, MatchesRecordedAnswers) {
  const char* printBatch = std::getenv("CSS_CORPUS_PRINT");
  bool record = true;
  for (const uint64_t h : kBatchHashes) record = record && h == 0;
  for (size_t batch = 0; batch < kBatches; ++batch) {
    const bool print = printBatch && static_cast<size_t>(std::atoi(printBatch)) == batch;
    const uint64_t got = runBatch(batch, print);
    if (record) {
      printf("0x%016llXULL,%s", static_cast<unsigned long long>(got), batch % 5 == 4 ? "\n" : " ");
    } else {
      EXPECT_EQ(got, kBatchHashes[batch]) << "batch " << batch;
    }
  }
  if (record) printf("\n");
  EXPECT_FALSE(record) << "no hashes recorded yet";
}

}  // namespace
