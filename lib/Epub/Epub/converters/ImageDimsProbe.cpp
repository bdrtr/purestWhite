#include "ImageDimsProbe.h"

size_t ImageDimsProbe::write(const uint8_t b) { return bb_image_probe_feed(&probe, &b, 1); }

size_t ImageDimsProbe::write(const uint8_t* data, const size_t len) {
  // Short once settled: a polite early stop.
  return bb_image_probe_feed(&probe, data, len);
}

bool ImageDimsProbe::getDimensions(ImageDimensions& out) const {
  uint32_t width = 0;
  uint32_t height = 0;
  return bb_image_probe_size(&probe, &width, &height) &&
         ImageToFramebufferDecoder::validateAndStoreDimensions(width, height, out, "image header");
}
