// The chapter parser's image display size as it was at 59d0836b (ChapterHtmlSlimParser.cpp, the
// startElement image branch), verbatim but for the inputs becoming parameters and the debug logs
// dropped. CssImageSizeTest compares bb_css_image_size with it.
#pragma once

#include "CssStyle.h"

namespace old_image_size {

inline void displaySize(const CssStyle& cssStyle, const int16_t naturalWidth, const int16_t naturalHeight,
                        const float emSize, const int containerWidth, const uint16_t viewportHeight, int& outWidth,
                        int& outHeight) {
  struct {
    int16_t width;
    int16_t height;
  } dims{naturalWidth, naturalHeight};
  int displayWidth = 0;
  int displayHeight = 0;
  const CssStyle& imgStyle = cssStyle;
  const bool hasCssHeight = imgStyle.hasImageHeight();
  const bool hasCssWidth = imgStyle.hasImageWidth();

  if (hasCssHeight && hasCssWidth && dims.width > 0 && dims.height > 0) {
    // Both CSS height and width set: resolve both, then clamp to viewport preserving requested ratio
    displayHeight = static_cast<int>(
        imgStyle.imageHeight.toPixels(emSize, static_cast<float>(viewportHeight)) + 0.5f);
    displayWidth =
        static_cast<int>(imgStyle.imageWidth.toPixels(emSize, static_cast<float>(containerWidth)) + 0.5f);
    if (displayHeight < 1) displayHeight = 1;
    if (displayWidth < 1) displayWidth = 1;
    if (displayWidth > containerWidth || displayHeight > viewportHeight) {
      float scaleX =
          (displayWidth > containerWidth) ? static_cast<float>(containerWidth) / displayWidth : 1.0f;
      float scaleY = (displayHeight > viewportHeight)
                         ? static_cast<float>(viewportHeight) / displayHeight
                         : 1.0f;
      float scale = (scaleX < scaleY) ? scaleX : scaleY;
      displayWidth = static_cast<int>(displayWidth * scale + 0.5f);
      displayHeight = static_cast<int>(displayHeight * scale + 0.5f);
      if (displayWidth < 1) displayWidth = 1;
      if (displayHeight < 1) displayHeight = 1;
    }
  } else if (hasCssHeight && !hasCssWidth && dims.width > 0 && dims.height > 0) {
    // Use CSS height (resolve % against viewport height) and derive width from aspect ratio
    displayHeight = static_cast<int>(
        imgStyle.imageHeight.toPixels(emSize, static_cast<float>(viewportHeight)) + 0.5f);
    if (displayHeight < 1) displayHeight = 1;
    displayWidth =
        static_cast<int>(displayHeight * (static_cast<float>(dims.width) / dims.height) + 0.5f);
    if (displayHeight > viewportHeight) {
      displayHeight = viewportHeight;
      // Rescale width to preserve aspect ratio when height is clamped
      displayWidth =
          static_cast<int>(displayHeight * (static_cast<float>(dims.width) / dims.height) + 0.5f);
      if (displayWidth < 1) displayWidth = 1;
    }
    if (displayWidth > containerWidth) {
      displayWidth = containerWidth;
      // Rescale height to preserve aspect ratio when width is clamped
      displayHeight =
          static_cast<int>(displayWidth * (static_cast<float>(dims.height) / dims.width) + 0.5f);
      if (displayHeight < 1) displayHeight = 1;
    }
    if (displayWidth < 1) displayWidth = 1;
  } else if (hasCssWidth && !hasCssHeight && dims.width > 0 && dims.height > 0) {
    // Use CSS width (resolve % against container width) and derive height from aspect ratio
    displayWidth =
        static_cast<int>(imgStyle.imageWidth.toPixels(emSize, static_cast<float>(containerWidth)) + 0.5f);
    if (displayWidth > containerWidth) displayWidth = containerWidth;
    if (displayWidth < 1) displayWidth = 1;
    displayHeight =
        static_cast<int>(displayWidth * (static_cast<float>(dims.height) / dims.width) + 0.5f);
    if (displayHeight > viewportHeight) {
      displayHeight = viewportHeight;
      // Rescale width to preserve aspect ratio when height is clamped
      displayWidth =
          static_cast<int>(displayHeight * (static_cast<float>(dims.width) / dims.height) + 0.5f);
      if (displayWidth < 1) displayWidth = 1;
    }
    if (displayHeight < 1) displayHeight = 1;
  } else {
    // Scale to fit container while maintaining aspect ratio
    int maxWidth = containerWidth;
    int maxHeight = viewportHeight;
    float scaleX = (dims.width > maxWidth) ? (float)maxWidth / dims.width : 1.0f;
    float scaleY = (dims.height > maxHeight) ? (float)maxHeight / dims.height : 1.0f;
    float scale = (scaleX < scaleY) ? scaleX : scaleY;
    if (scale > 1.0f) scale = 1.0f;

    displayWidth = (int)(dims.width * scale);
    displayHeight = (int)(dims.height * scale);
  }
  outWidth = displayWidth;
  outHeight = displayHeight;
}

}  // namespace old_image_size
