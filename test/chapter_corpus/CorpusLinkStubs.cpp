// ParserLinkStubs.cpp's stand-ins, with PNG and JPEG supported so the parser places images; no
// decoder reads the cached files, so dimensions come from the header probe alone.
#include <Epub/blocks/ImageBlock.h>
#include <Epub/converters/ImageDecoderFactory.h>
#include <Epub/converters/ImageToFramebufferDecoder.h>
#include <GfxRenderer.h>

#include <utility>

ImageBlock::ImageBlock(std::string imagePath, std::string srcPath, int16_t width, int16_t height)
    : imagePath(std::move(imagePath)), srcPath(std::move(srcPath)), width(width), height(height) {}

bool ImageDecoderFactory::isFormatSupported(const std::string& path) {
  const auto ext = path.substr(path.rfind('.') == std::string::npos ? path.size() : path.rfind('.'));
  return ext == ".jpg" || ext == ".jpeg" || ext == ".png";
}
ImageToFramebufferDecoder* ImageDecoderFactory::getDecoder(const std::string&) { return nullptr; }

void ImageBlock::render(GfxRenderer&, int, int) {}
void ImageBlock::renderPlaceholder(GfxRenderer&, int, int) const {}
bool ImageBlock::needsDecode() const { return false; }
