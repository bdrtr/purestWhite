#include "ChapterXPathResolver.h"

#include <Logging.h>
#include <Print.h>
#include <bembeyaz/kosync.h>
#include <bembeyaz/xml.h>

#include <algorithm>
#include <cmath>
#include <string>

namespace {
// A chapter streamed into a bembeyaz-kosync resolver.
class Resolver final : public Print {
 public:
  Resolver(const int kind, const uint64_t target, const int spineIndex)
      : resolver(bb_kosync_new(kind, target, static_cast<uint32_t>(spineIndex), BB_XML_DEFAULT_LIMIT)) {
    if (!resolver) LOG_ERR("KOX", "Failed to create XML parser");
  }
  ~Resolver() override { bb_kosync_free(resolver); }
  Resolver(const Resolver&) = delete;
  Resolver& operator=(const Resolver&) = delete;

  bool ok() const { return resolver && !bb_kosync_failed(resolver); }

  size_t write(uint8_t c) override { return write(&c, 1); }

  size_t write(const uint8_t* buffer, size_t size) override {
    log(bb_kosync_parse(resolver, reinterpret_cast<const char*>(buffer), size, false), "XML parse error");
    return size;
  }

  bool finish() {
    log(bb_kosync_parse(resolver, nullptr, 0, true), "Final XML parse error");
    return ok();
  }

  std::string xpath() const {
    size_t len = 0;
    const char* xpath = bb_kosync_xpath(resolver, &len);
    return xpath ? std::string(xpath, len) : std::string();
  }

  size_t visibleCodepoints() const { return static_cast<size_t>(bb_kosync_visible_codepoints(resolver)); }

 private:
  static void log(const int error, const char* what) {
    if (error != BB_XML_OK) LOG_ERR("KOX", "%s: %s", what, bb_xml_error_message(error));
  }

  bb_kosync* resolver;
};
}  // namespace

std::string ChapterXPathResolver::findXPathForParagraph(const std::shared_ptr<Epub>& epub, const int spineIndex,
                                                        const uint16_t paragraphIndex) {
  if (!epub || paragraphIndex == 0 || spineIndex < 0 || spineIndex >= epub->getSpineItemsCount()) {
    return "";
  }

  const auto href = epub->getSpineItem(spineIndex).href;
  if (href.empty()) {
    return "";
  }

  Resolver resolver(BB_KOSYNC_PARAGRAPH, paragraphIndex, spineIndex);
  if (!resolver.ok()) {
    return "";
  }

  if (!epub->readItemContentsToStream(href, resolver, 1024) || !resolver.finish()) {
    return "";
  }

  if (std::string xpath = resolver.xpath(); !xpath.empty()) {
    LOG_DBG("KOX", "Resolved paragraph %u in spine %d -> %s", paragraphIndex, spineIndex, xpath.c_str());
    return xpath;
  }

  LOG_DBG("KOX", "Paragraph %u not found in spine %d", paragraphIndex, spineIndex);
  return "";
}

std::string ChapterXPathResolver::findXPathForVisibleTextOffset(const std::shared_ptr<Epub>& epub, const int spineIndex,
                                                                const uint32_t visibleTextOffset) {
  if (!epub || spineIndex < 0 || spineIndex >= epub->getSpineItemsCount()) {
    return "";
  }

  const auto href = epub->getSpineItem(spineIndex).href;
  if (href.empty()) {
    return "";
  }

  Resolver resolver(BB_KOSYNC_OFFSET, visibleTextOffset, spineIndex);
  if (!resolver.ok()) {
    return "";
  }

  if (!epub->readItemContentsToStream(href, resolver, 1024) || !resolver.finish()) {
    return "";
  }

  if (std::string xpath = resolver.xpath(); !xpath.empty()) {
    LOG_DBG("KOX", "Resolved visible offset %u in spine %d -> %s", visibleTextOffset, spineIndex, xpath.c_str());
    return xpath;
  }

  LOG_DBG("KOX", "Visible offset %u not found in spine %d", visibleTextOffset, spineIndex);
  return "";
}

std::string ChapterXPathResolver::findXPathForProgress(const std::shared_ptr<Epub>& epub, const int spineIndex,
                                                       const float intraSpineProgress) {
  if (!epub || spineIndex < 0 || spineIndex >= epub->getSpineItemsCount()) {
    return "";
  }

  const auto href = epub->getSpineItem(spineIndex).href;
  if (href.empty()) {
    return "";
  }

  if (!(intraSpineProgress > 0.0f)) {
    return "/body/DocFragment[" + std::to_string(spineIndex + 1) + "]/body";
  }

  Resolver counter(BB_KOSYNC_COUNT, 0, spineIndex);
  if (!counter.ok() || !epub->readItemContentsToStream(href, counter, 1024) || !counter.finish()) {
    return "";
  }

  const size_t totalVisibleChars = counter.visibleCodepoints();
  if (totalVisibleChars == 0) {
    return "";
  }

  const float clamped = std::max(0.0f, std::min(1.0f, intraSpineProgress));
  const size_t targetVisibleChar =
      std::max<size_t>(1, std::min(totalVisibleChars, static_cast<size_t>(std::ceil(clamped * totalVisibleChars))));

  Resolver resolver(BB_KOSYNC_OFFSET_INCLUSIVE, targetVisibleChar, spineIndex);
  if (!resolver.ok()) {
    return "";
  }

  if (!epub->readItemContentsToStream(href, resolver, 1024) || !resolver.finish()) {
    return "";
  }

  if (std::string xpath = resolver.xpath(); !xpath.empty()) {
    LOG_DBG("KOX", "Resolved progress %.3f in spine %d -> %s", intraSpineProgress, spineIndex, xpath.c_str());
    return xpath;
  }

  LOG_DBG("KOX", "Could not resolve progress %.3f in spine %d", intraSpineProgress, spineIndex);
  return "";
}
