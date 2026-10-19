#include <gtest/gtest.h>

#include <cstdio>
#include <string>

#include "FsHelpers.h"

namespace {

using namespace std::string_view_literals;

TEST(ReflowableBookExtension, IncludesTextFormatsAndExcludesImageBooks) {
  for (const auto path : {"/Books/book.epub"sv, "book.TXT"sv, "book.mD"sv}) {
    EXPECT_TRUE(FsHelpers::hasReflowableBookExtension(path));
  }
  for (const auto path : {"book.xtc"sv, "book.XTCH"sv, "book.bmp"sv, "book.epub.bak"sv, ""sv}) {
    EXPECT_FALSE(FsHelpers::hasReflowableBookExtension(path));
  }
}

TEST(IsSafePathComponent, AcceptsNamesWithRepeatedDots) {
  EXPECT_TRUE(FsHelpers::isSafePathComponent("volume..2.epub"sv));
  EXPECT_TRUE(FsHelpers::isSafePathComponent("notes...txt"sv));
  EXPECT_TRUE(FsHelpers::isSafePathComponent(".hidden"sv));
  EXPECT_TRUE(FsHelpers::isSafePathComponent("a.b"sv));
  EXPECT_TRUE(FsHelpers::isSafePathComponent("book.epub"sv));
}

TEST(IsSafePathComponent, RejectsEmptyAndExactDotComponents) {
  EXPECT_FALSE(FsHelpers::isSafePathComponent(""sv));
  EXPECT_FALSE(FsHelpers::isSafePathComponent("."sv));
  EXPECT_FALSE(FsHelpers::isSafePathComponent(".."sv));
}

TEST(IsSafePathComponent, RejectsPathSeparatorsAnywhereInTheComponent) {
  EXPECT_FALSE(FsHelpers::isSafePathComponent("a/b"sv));
  EXPECT_FALSE(FsHelpers::isSafePathComponent("a\\b"sv));
  EXPECT_FALSE(FsHelpers::isSafePathComponent("../x"sv));
  EXPECT_FALSE(FsHelpers::isSafePathComponent("x/.."sv));
}

TEST(NormalisePath, CollapsesParentReferenceWithinPath) {
  EXPECT_EQ(FsHelpers::normalisePath("/Books/../.crosspoint/x"), ".crosspoint/x");
}

TEST(NormalisePath, DropsLeadingParentReferencesPastRoot) { EXPECT_EQ(FsHelpers::normalisePath("/../../etc"), "etc"); }

// Sanitizes into a buffer of `size` bytes (at most 64, the size ScreenshotUtil uses).
std::string sanitize(const char* input, const size_t size = 64) {
  char out[64];
  FsHelpers::sanitizePathComponentForFat32(input, out, size);
  return out;
}

// Book titles from the EPUBs attached to #2103 and #2199.
constexpr char kTitle2103[] = "Богиня глюкозы. Нормализуйте уровень сахара в крови, чтобы изменить свою жизнь";
constexpr char kTitle2199[] = "Вглядываясь в солнце. Жизнь без страха смерти";

TEST(SanitizePathComponentForFat32, KeepsTitleThatFits) {
  EXPECT_EQ(sanitize("Эдем (полный перевод)"), "Эдем-(полный-перевод)");
}

// The readers copy the title into ScreenshotInfo::title (char[64]) with snprintf, which can
// end the copy partway through a Cyrillic letter.
TEST(SanitizePathComponentForFat32, DropsLetterCutOffByCaller) {
  char title[64];
  snprintf(title, sizeof(title), "%s", kTitle2103);
  EXPECT_EQ(sanitize(title), "Богиня-глюкозы.-Нормализуйте-уров");
  snprintf(title, sizeof(title), "%s", kTitle2199);
  EXPECT_EQ(sanitize(title), "Вглядываясь-в-солнце.-Жизнь-без-ст");
}

TEST(SanitizePathComponentForFat32, DoesNotSplitLetterAtBufferLimit) {
  // Each letter of "Жизнь" is 2 bytes. 7 bytes of room holds "Жиз" and half of "н".
  EXPECT_EQ(sanitize("Жизнь", 8), "Жиз");
  EXPECT_EQ(sanitize(kTitle2103), "Богиня-глюкозы.-Нормализуйте-уров");
}

}  // namespace

// decodeUriEscapes and normalisePath, alone and as the OPF parser chains them, over generated paths;
// the hash was recorded from the C++ helpers before the Rust port.
TEST(PathHelpers, GeneratedPathsMatchTheRecordedHelpers) {
  static const char* const kPieces[] = {"/",         "/",        "//",      ".",       "..",  "../", "./", "a",  "bc",
                                        "%",         "%2",       "%2F",     "%2f",     "%41", "%zz", "%%", "%0", "%00",
                                        "%e2%80%94", "\xC3\xA9", "x.xhtml", "%2e%2e/", "\\",  " ",   "#f", "?q"};
  uint64_t state = 0x5EEDF00DULL;
  uint64_t hash = 0xCBF29CE484222325ULL;
  const auto add = [&hash](const std::string& s) {
    for (const char c : s) {
      hash ^= static_cast<uint8_t>(c);
      hash *= 0x100000001B3ULL;
    }
    hash ^= 0xFF;
    hash *= 0x100000001B3ULL;
  };
  for (int i = 0; i < 100000; i++) {
    std::string path;
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    const int pieces = static_cast<int>((state >> 40) % 12);
    for (int p = 0; p < pieces; p++) {
      state ^= state << 13;
      state ^= state >> 7;
      state ^= state << 17;
      path += kPieces[(state >> 33) % (sizeof(kPieces) / sizeof(kPieces[0]))];
    }
    add(path);
    add(FsHelpers::decodeUriEscapes(path));
    add(FsHelpers::normalisePath(path));
    add(FsHelpers::normalisePath(FsHelpers::decodeUriEscapes(path)));
  }
  EXPECT_EQ(hash, 0x02B66160A70509D4ULL) << std::hex << std::uppercase << "0x" << hash << "ULL";
}
