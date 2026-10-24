#pragma once

#include <HalStorage.h>
#include <bembeyaz/bmp.h>

#include <cstdint>
#include <optional>

#include "BitmapHelpers.h"

#pragma pack(push, 1)
struct BmpHeader {
  struct {
    uint16_t bfType;
    uint32_t bfSize;
    uint16_t bfReserved1;
    uint16_t bfReserved2;
    uint32_t bfOffBits;
  } fileHeader;
  struct {
    uint32_t biSize;
    int32_t biWidth;
    int32_t biHeight;
    uint16_t biPlanes;
    uint16_t biBitCount;
    uint32_t biCompression;
    uint32_t biSizeImage;
    int32_t biXPelsPerMeter;
    int32_t biYPelsPerMeter;
    uint32_t biClrUsed;
    uint32_t biClrImportant;
  } infoHeader;
  struct RgbQuad {
    uint8_t rgbBlue;
    uint8_t rgbGreen;
    uint8_t rgbRed;
    uint8_t rgbReserved;
  };
  RgbQuad colors[2];
};
#pragma pack(pop)

enum class BmpReaderError : uint8_t {
  Ok = 0,
  FileInvalid,
  SeekStartFailed,

  NotBMP,
  DIBTooSmall,

  BadPlanes,
  UnsupportedBpp,
  UnsupportedCompression,

  BadDimensions,
  ImageTooLarge,
  PaletteTooLarge,

  SeekPixelDataFailed,
  BufferTooSmall,
  OomRowBuffer,
  ShortReadRow,
  OomDitherer,
};

class Bitmap {
 public:
  static const char* errorToString(BmpReaderError err);

  explicit Bitmap(HalFile& file, bool dithering = false, bool originalThresholds = false)
      : file(file), dithering(dithering), originalThresholds(originalThresholds) {}
  ~Bitmap();
  BmpReaderError parseHeaders();
  BmpReaderError readNextRow(uint8_t* data, uint8_t* rowBuffer) const;
  BmpReaderError rewindToData() const;
  int getWidth() const { return bmp.width; }
  int getHeight() const { return bmp.height; }
  bool isTopDown() const { return bmp.top_down; }
  bool hasGreyscale() const { return bmp.bits_per_pixel > 1; }
  int getRowBytes() const { return bmp.row_bytes; }
  bool is1Bit() const { return bmp.bits_per_pixel == 1; }
  uint16_t getBpp() const { return bmp.bits_per_pixel; }
  uint32_t getDataOffset() const { return bmp.data_offset; }

 private:
  HalFile& file;
  bool dithering = false;
  bool originalThresholds = false;
  bb_bmp bmp{};

  // How rows become 2-bit levels, chosen by parseHeaders (mutable for const methods).
  mutable int prevRowY = -1;  // the row last read
  mutable std::optional<Ditherer> ditherer;
};
