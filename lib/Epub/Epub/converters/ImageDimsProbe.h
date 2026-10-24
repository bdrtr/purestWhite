#pragma once
#include <Print.h>
#include <bembeyaz/image.h>

#include "ImageToFramebufferDecoder.h"

// Streaming JPEG/PNG header parser: finds image dimensions from the first few
// KB of a compressed stream without inflating the whole image. Feed bytes via
// the Print interface (e.g. Epub::readItemContentsToStream with
// allowEarlyStop=true); write() returns short once the dimensions are known or
// the stream is known to be unusable, which the zip layer treats as a polite
// early stop rather than an error.
//
// The parsing is bembeyaz-image's: JPEG marker segments are walked (EXIF/APPn of
// any size skipped without buffering) up to a SOFn frame header; a PNG's IHDR
// fields are read at their fixed offsets.
class ImageDimsProbe : public Print {
 public:
  ImageDimsProbe() { bb_image_probe_init(&probe); }
  size_t write(uint8_t b) override;
  size_t write(const uint8_t* data, size_t len) override;

  // True only when a valid header was found; fills `out`.
  bool getDimensions(ImageDimensions& out) const;

 private:
  bb_image_probe probe;
};
