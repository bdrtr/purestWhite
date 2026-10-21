// Pins lib/GfxRenderer's Bitmap: header checks, palette luminance and the packed 2-bit rows.
#include <gtest/gtest.h>

#include <array>
#include <cstdint>
#include <vector>

#include "Bitmap.h"
#include "BitmapHelpers.h"

namespace {

using Palette = std::vector<std::array<uint8_t, 4>>;  // B, G, R, reserved

void put16(std::vector<uint8_t>& v, const uint16_t x) {
  v.push_back(x & 0xFF);
  v.push_back(x >> 8);
}

void put32(std::vector<uint8_t>& v, const uint32_t x) {
  for (int i = 0; i < 4; i++) v.push_back((x >> (8 * i)) & 0xFF);
}

std::vector<uint8_t> makeBmp(const int32_t width, const int32_t height, const uint16_t bpp, const uint32_t compression,
                             const uint32_t colors, const Palette& palette, const std::vector<uint8_t>& pixels) {
  const uint32_t offset = 54 + 4 * static_cast<uint32_t>(palette.size());
  std::vector<uint8_t> v{'B', 'M'};
  put32(v, offset + pixels.size());
  put32(v, 0);
  put32(v, offset);
  put32(v, 40);
  put32(v, static_cast<uint32_t>(width));
  put32(v, static_cast<uint32_t>(height));
  put16(v, 1);
  put16(v, bpp);
  put32(v, compression);
  for (int i = 0; i < 3; i++) put32(v, 0);
  put32(v, colors);
  put32(v, 0);
  for (const auto& entry : palette) v.insert(v.end(), entry.begin(), entry.end());
  v.insert(v.end(), pixels.begin(), pixels.end());
  return v;
}

Palette grays(const int count) {
  Palette p;
  for (int i = 0; i < count; i++) {
    const auto g = static_cast<uint8_t>(i * 255 / (count - 1));
    p.push_back({g, g, g, 0});
  }
  return p;
}

int lum(const int r, const int g, const int b) { return (77 * r + 150 * g + 29 * b) >> 8; }

// The packed row Bitmap gives without dithering: direct levels for a native palette, otherwise
// quantize().
std::vector<uint8_t> expectedRow(const std::vector<int>& lums, const bool native, const int y) {
  std::vector<uint8_t> out((lums.size() + 3) / 4, 0);
  for (size_t x = 0; x < lums.size(); x++) {
    const int gray = adjustPixel(lums[x]);
    const uint8_t level = native ? gray >> 6 : quantize(gray, static_cast<int>(x), y);
    out[x / 4] |= level << (6 - 2 * (x % 4));
  }
  return out;
}

std::vector<uint8_t> readRow(const Bitmap& bitmap) {
  std::vector<uint8_t> row(bitmap.getRowBytes());
  std::vector<uint8_t> out((bitmap.getWidth() + 3) / 4, 0);
  EXPECT_EQ(bitmap.readNextRow(out.data(), row.data()), BmpReaderError::Ok);
  return out;
}

}  // namespace

TEST(BmpReader, ReadsTheHeadersOfEveryDepth) {
  for (const uint16_t bpp : {1, 2, 4, 8, 24, 32}) {
    const auto data = makeBmp(13, -3, bpp, 0, 0, bpp <= 8 ? grays(1 << bpp) : Palette{},
                              std::vector<uint8_t>(3 * ((13 * bpp + 31) / 32 * 4)));
    HalFile file(data);
    Bitmap bitmap(file);
    ASSERT_EQ(bitmap.parseHeaders(), BmpReaderError::Ok) << bpp;
    EXPECT_EQ(bitmap.getWidth(), 13);
    EXPECT_EQ(bitmap.getHeight(), 3);
    EXPECT_TRUE(bitmap.isTopDown());
    EXPECT_EQ(bitmap.getBpp(), bpp);
    EXPECT_EQ(bitmap.getRowBytes(), (13 * bpp + 31) / 32 * 4);
    EXPECT_EQ(bitmap.is1Bit(), bpp == 1);
    EXPECT_EQ(bitmap.hasGreyscale(), bpp > 1);
  }
}

TEST(BmpReader, RefusesWhatItCannotShow) {
  const auto ok = makeBmp(8, 8, 24, 0, 0, {}, std::vector<uint8_t>(8 * 24));
  const auto check = [](std::vector<uint8_t> data, const BmpReaderError expected) {
    HalFile file(data);
    Bitmap bitmap(file);
    EXPECT_EQ(bitmap.parseHeaders(), expected) << Bitmap::errorToString(expected);
  };
  check(ok, BmpReaderError::Ok);
  auto notBmp = ok;
  notBmp[1] = 'X';
  check(notBmp, BmpReaderError::NotBMP);
  auto smallDib = ok;
  smallDib[14] = 12;
  check(smallDib, BmpReaderError::DIBTooSmall);
  auto planes = ok;
  planes[26] = 0;
  check(planes, BmpReaderError::BadPlanes);
  check(makeBmp(8, 8, 16, 0, 0, {}, {}), BmpReaderError::UnsupportedBpp);
  check(makeBmp(8, 8, 8, 1, 0, grays(256), {}), BmpReaderError::UnsupportedCompression);
  check(makeBmp(8, 8, 24, 3, 0, {}, {}), BmpReaderError::UnsupportedCompression);
  check(makeBmp(8, 8, 32, 3, 0, {}, std::vector<uint8_t>(256)), BmpReaderError::Ok);
  check(makeBmp(8, 8, 8, 0, 300, {}, {}), BmpReaderError::PaletteTooLarge);
  check(makeBmp(-8, 8, 24, 0, 0, {}, {}), BmpReaderError::BadDimensions);
  check(makeBmp(8, 0, 24, 0, 0, {}, {}), BmpReaderError::BadDimensions);
  check(makeBmp(2049, 8, 24, 0, 0, {}, {}), BmpReaderError::ImageTooLarge);
  check(makeBmp(8, 3073, 24, 0, 0, {}, {}), BmpReaderError::ImageTooLarge);
  auto farPixels = ok;
  farPixels[12] = 0x10;
  check(farPixels, BmpReaderError::SeekPixelDataFailed);
  check({}, BmpReaderError::NotBMP);
  check(std::vector<uint8_t>(ok.begin(), ok.begin() + 20), BmpReaderError::BadPlanes);
}

TEST(BmpReader, NativePalettesMapStraightToGrayLevels) {
  // 2 bits, 0 85 170 255: pixels 0 1 2 3 3 2.
  const auto data = makeBmp(6, 1, 2, 0, 0, grays(4), {0x1B, 0xE0, 0, 0});
  HalFile file(data);
  Bitmap bitmap(file);
  ASSERT_EQ(bitmap.parseHeaders(), BmpReaderError::Ok);
  EXPECT_EQ(readRow(bitmap), expectedRow({0, 85, 170, 255, 255, 170}, true, 0));
}

TEST(BmpReader, OtherPalettesAreQuantized) {
  // 4 bits, 16 grays (17 apart: not all near a native level). Pixels 0..15 then 15..0, 70 wide
  // so a row spans more than one 64-pixel piece.
  std::vector<uint8_t> pixels(2 * 36, 0);
  std::vector<int> lums0, lums1;
  for (int x = 0; x < 70; x++) {
    const int a = x % 16, b = 15 - x % 16;
    pixels[x / 2] |= (x & 1) ? a : a << 4;
    pixels[36 + x / 2] |= (x & 1) ? b : b << 4;
    lums0.push_back(a * 17);
    lums1.push_back(b * 17);
  }
  const auto data = makeBmp(70, 2, 4, 0, 16, grays(16), pixels);
  HalFile file(data);
  Bitmap bitmap(file);
  ASSERT_EQ(bitmap.parseHeaders(), BmpReaderError::Ok);
  EXPECT_EQ(readRow(bitmap), expectedRow(lums0, false, 0));
  EXPECT_EQ(readRow(bitmap), expectedRow(lums1, false, 1));
}

TEST(BmpReader, TrueColorRowsUseLuminance) {
  // 24 bits: blue, green, red, white, gray 100. 32 bits the same with alpha.
  const std::vector<std::array<int, 3>> rgb = {{0, 0, 255}, {0, 255, 0}, {255, 0, 0}, {255, 255, 255}, {100, 100, 100}};
  std::vector<int> lums;
  std::vector<uint8_t> px24, px32;
  for (const auto& [r, g, b] : rgb) {
    lums.push_back(lum(r, g, b));
    px24.insert(px24.end(), {static_cast<uint8_t>(b), static_cast<uint8_t>(g), static_cast<uint8_t>(r)});
    px32.insert(px32.end(), {static_cast<uint8_t>(b), static_cast<uint8_t>(g), static_cast<uint8_t>(r), 0x80});
  }
  px24.resize(16);
  for (const auto& [bpp, pixels] : {std::pair{24, px24}, std::pair{32, px32}}) {
    const auto data = makeBmp(5, 1, bpp, 0, 0, {}, pixels);
    HalFile file(data);
    Bitmap bitmap(file);
    ASSERT_EQ(bitmap.parseHeaders(), BmpReaderError::Ok);
    EXPECT_EQ(readRow(bitmap), expectedRow(lums, false, 0)) << bpp;
  }
}

TEST(BmpReader, OneBitRowsFollowThePalette) {
  // Inverted palette: bit 1 is black.
  const auto data = makeBmp(10, 1, 1, 0, 2, {{255, 255, 255, 0}, {0, 0, 0, 0}}, {0b10110000, 0b01000000, 0, 0});
  HalFile file(data);
  Bitmap bitmap(file);
  ASSERT_EQ(bitmap.parseHeaders(), BmpReaderError::Ok);
  EXPECT_EQ(readRow(bitmap), expectedRow({0, 255, 0, 0, 255, 255, 255, 255, 255, 0}, true, 0));
}

TEST(BmpReader, DitheredRowsRepeatAfterRewinding) {
  std::vector<uint8_t> pixels(4 * 40);
  for (size_t i = 0; i < pixels.size(); i++) pixels[i] = static_cast<uint8_t>(i * 37);
  const auto data = makeBmp(40, 4, 8, 0, 0, grays(256), pixels);
  HalFile file(data);
  Bitmap bitmap(file, true);
  ASSERT_EQ(bitmap.parseHeaders(), BmpReaderError::Ok);
  const auto first = readRow(bitmap);
  const auto second = readRow(bitmap);
  ASSERT_EQ(bitmap.rewindToData(), BmpReaderError::Ok);
  EXPECT_EQ(readRow(bitmap), first);
  EXPECT_EQ(readRow(bitmap), second);
}

TEST(BmpReader, ARowCutShortIsAnError) {
  const auto data = makeBmp(8, 2, 8, 0, 0, grays(256), std::vector<uint8_t>(12));
  HalFile file(data);
  Bitmap bitmap(file);
  ASSERT_EQ(bitmap.parseHeaders(), BmpReaderError::Ok);
  std::vector<uint8_t> row(bitmap.getRowBytes()), out(2);
  EXPECT_EQ(bitmap.readNextRow(out.data(), row.data()), BmpReaderError::Ok);
  EXPECT_EQ(bitmap.readNextRow(out.data(), row.data()), BmpReaderError::ShortReadRow);
}
