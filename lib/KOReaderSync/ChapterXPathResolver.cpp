#include "ChapterXPathResolver.h"

#include <Epub/VisibleTextUtils.h>
#include <Logging.h>
#include <Print.h>
#include <Utf8.h>
#include <XmlParser.h>
#include <XmlParserUtils.h>

#include <algorithm>
#include <cmath>
#include <cstring>
#include <string>
#include <utility>
#include <vector>

namespace {
std::string stripPrefix(const char* name) {
  if (!name) {
    return "";
  }

  const char* local = std::strrchr(name, ':');
  return local ? std::string(local + 1) : std::string(name);
}

struct NameCounter {
  std::string name;
  int count;
};

// Mirrors crengine's IsEmptySpace(): only these four count as empty space.
bool isWhitespaceOnly(const char* data, const int len) {
  for (int i = 0; i < len; i++) {
    const char c = data[i];
    if (c != ' ' && c != '\r' && c != '\n' && c != '\t') {
      return false;
    }
  }
  return true;
}

struct ParentState {
  std::vector<NameCounter> children;

  int nextIndex(const std::string& name) {
    for (auto& child : children) {
      if (child.name == name) {
        child.count++;
        return child.count;
      }
    }

    children.push_back({name, 1});
    return 1;
  }
};

struct PathSegment {
  std::string name;
  int index;
};

std::string buildParagraphXPath(const int spineIndex, const std::vector<PathSegment>& path, const int textNodeIndex,
                                const size_t charOffset) {
  std::string xpath = "/body/DocFragment[" + std::to_string(spineIndex + 1) + "]/body";
  for (const auto& segment : path) {
    xpath += "/" + segment.name + "[" + std::to_string(segment.index) + "]";
  }
  if (textNodeIndex > 0) {
    xpath += "/text()[" + std::to_string(textNodeIndex) + "]." + std::to_string(charOffset);
  }
  return xpath;
}

size_t countUtf8Codepoints(const char* data, const int len) {
  if (!data || len <= 0) {
    return 0;
  }

  size_t count = 0;
  const unsigned char* ptr = reinterpret_cast<const unsigned char*>(data);
  const unsigned char* end = ptr + len;
  while (ptr < end) {
    utf8NextCodepoint(&ptr);
    count++;
  }

  return count;
}

class ParagraphTextCounter final : public Print {
 public:
  ParagraphTextCounter() {
    if (!parser.create()) {
      LOG_ERR("KOX", "Failed to create XML parser");
      return;
    }

    parser.setUserData(this);
    parser.setElementHandler(&ParagraphTextCounter::startElement, &ParagraphTextCounter::endElement);
    parser.setCharacterDataHandler(&ParagraphTextCounter::characterData);
  }

  bool ok() const { return static_cast<bool>(parser) && parseOk; }

  bool finish() {
    if (!parser || !parseOk || stopped) {
      return parseOk;
    }

    if (!parser.parse(nullptr, 0, true)) {
      LOG_ERR("KOX", "Final XML parse error: %s", parser.errorString());
      parseOk = false;
    }
    return parseOk;
  }

  size_t write(uint8_t c) override { return write(&c, 1); }

  size_t write(const uint8_t* buffer, size_t size) override {
    if (!parser || !parseOk || stopped) {
      return size;
    }

    if (!parser.parse(buffer, size, false)) {
      if (!parser.stopped()) {
        LOG_ERR("KOX", "XML parse error: %s", parser.errorString());
        parseOk = false;
      }
    }

    return size;
  }

  // Visible codepoints up to the end of the last non-whitespace run. Trailing formatting
  // whitespace is excluded so a 100% target still resolves inside real text.
  size_t totalVisibleChars() const { return lastTextEndChars; }

 private:
  static void startElement(void* userData, const char* name, const char**) {
    auto* self = static_cast<ParagraphTextCounter*>(userData);
    self->onStartElement(name);
  }

  static void endElement(void* userData, const char* name) {
    auto* self = static_cast<ParagraphTextCounter*>(userData);
    self->onEndElement(name);
  }

  static void characterData(void* userData, const char* data, const int len) {
    auto* self = static_cast<ParagraphTextCounter*>(userData);
    self->onCharacterData(data, len);
  }

  void onStartElement(const char* rawName) {
    const std::string name = stripPrefix(rawName);

    if (!insideBody) {
      if (name == "body") {
        insideBody = true;
        bodyDepth = depth;
      }
      depth++;
      return;
    }

    if (nonVisibleDepth > 0 || VisibleTextUtils::isNonVisibleElement(name)) {
      nonVisibleDepth++;
    }
    depth++;
  }

  void onEndElement(const char* rawName) {
    const std::string name = stripPrefix(rawName);

    depth--;
    if (!insideBody) {
      return;
    }

    if (depth == bodyDepth && name == "body") {
      insideBody = false;
      nonVisibleDepth = 0;
      return;
    }

    if (nonVisibleDepth > 0) {
      nonVisibleDepth--;
    }
  }

  void onCharacterData(const char* data, const int len) {
    if (!insideBody || nonVisibleDepth > 0 || len <= 0) {
      return;
    }

    visibleChars += countUtf8Codepoints(data, len);
    if (!isWhitespaceOnly(data, len)) {
      // Trailing whitespace inside this run is single-byte, so bytes equal codepoints here.
      int trailing = 0;
      while (trailing < len && isWhitespaceOnly(data + len - 1 - trailing, 1)) {
        trailing++;
      }
      lastTextEndChars = visibleChars - static_cast<size_t>(trailing);
    }
  }

 private:
  XmlParser parser;
  bool parseOk = true;
  bool insideBody = false;
  bool stopped = false;
  int depth = 0;
  int bodyDepth = -1;
  uint16_t nonVisibleDepth = 0;
  size_t visibleChars = 0;
  size_t lastTextEndChars = 0;
};

class XPathParagraphResolver final : public Print {
 public:
  explicit XPathParagraphResolver(const int targetParagraph) : targetParagraph(targetParagraph) {
    if (!parser.create()) {
      LOG_ERR("KOX", "Failed to create XML parser");
      return;
    }

    parser.setUserData(this);
    parser.setElementHandler(&XPathParagraphResolver::startElement, &XPathParagraphResolver::endElement);
  }

  bool ok() const { return static_cast<bool>(parser) && parseOk; }

  bool finish() {
    if (!parser || !parseOk || stopped) {
      return parseOk;
    }

    if (!parser.parse(nullptr, 0, true)) {
      LOG_ERR("KOX", "Final XML parse error: %s", parser.errorString());
      parseOk = false;
    }
    return parseOk;
  }

  bool hasMatch() const { return !xpath.empty(); }
  const std::string& getXPath() const { return xpath; }

  size_t write(uint8_t c) override { return write(&c, 1); }

  size_t write(const uint8_t* buffer, size_t size) override {
    if (!parser || !parseOk || stopped) {
      return size;
    }

    if (!parser.parse(buffer, size, false)) {
      if (!parser.stopped()) {
        LOG_ERR("KOX", "XML parse error: %s", parser.errorString());
        parseOk = false;
      }
    }

    return size;
  }

  int spineIndex = 0;

 private:
  static void startElement(void* userData, const char* name, const char**) {
    auto* self = static_cast<XPathParagraphResolver*>(userData);
    self->onStartElement(name);
  }

  static void endElement(void* userData, const char* name) {
    auto* self = static_cast<XPathParagraphResolver*>(userData);
    self->onEndElement(name);
  }

  void onStartElement(const char* rawName) {
    const std::string name = stripPrefix(rawName);

    if (!insideBody) {
      if (name == "body") {
        insideBody = true;
        bodyDepth = depth;
        parentStates.emplace_back();
      }
      depth++;
      return;
    }

    const int siblingIndex = parentStates.back().nextIndex(name);
    path.push_back({name, siblingIndex});
    parentStates.emplace_back();

    // Count both <p> and <li> as paragraph-like positions, matching how the section
    // layout tracks them (xpathParagraphIndex and xpathListItemIndex). This ensures
    // KOReader progress in list items maps to the correct XPath.
    if (name == "p") {
      paragraphCount++;
    } else if (name == "li") {
      paragraphCount++;
    }
    if (paragraphCount == targetParagraph) {
      xpath = buildParagraphXPath(spineIndex, path, 0, 0);
      stopped = true;
      parser.stop();
    }

    depth++;
  }

  void onEndElement(const char* rawName) {
    const std::string name = stripPrefix(rawName);

    depth--;
    if (!insideBody) {
      return;
    }

    if (depth == bodyDepth && name == "body") {
      insideBody = false;
      parentStates.clear();
      path.clear();
      return;
    }

    if (!path.empty()) {
      path.pop_back();
    }
    if (!parentStates.empty()) {
      parentStates.pop_back();
    }
  }

  XmlParser parser;
  const int targetParagraph;
  bool parseOk = true;
  bool insideBody = false;
  bool stopped = false;
  int depth = 0;
  int bodyDepth = -1;
  int paragraphCount = 0;
  std::vector<ParentState> parentStates;
  std::vector<PathSegment> path;
  std::string xpath;
};

class XPathProgressResolver final : public Print {
 public:
  enum class BoundaryMode { Exclusive, Inclusive };

  explicit XPathProgressResolver(const size_t targetVisibleChar,
                                 const BoundaryMode boundaryMode = BoundaryMode::Exclusive)
      : targetVisibleChar(targetVisibleChar), boundaryMode(boundaryMode) {
    if (!parser.create()) {
      LOG_ERR("KOX", "Failed to create XML parser");
      return;
    }

    parser.setUserData(this);
    parser.setElementHandler(&XPathProgressResolver::startElement, &XPathProgressResolver::endElement);
    parser.setCharacterDataHandler(&XPathProgressResolver::characterData);
    parser.setCommentHandler(&XPathProgressResolver::comment);
    parser.setProcessingInstructionHandler(&XPathProgressResolver::processingInstruction);
    parser.setCdataSectionHandler(&XPathProgressResolver::startCdataSection, &XPathProgressResolver::endCdataSection);
  }

  bool ok() const { return static_cast<bool>(parser) && parseOk; }

  bool finish() {
    if (!parser || !parseOk || stopped) {
      return parseOk;
    }

    if (!parser.parse(nullptr, 0, true)) {
      LOG_ERR("KOX", "Final XML parse error: %s", parser.errorString());
      parseOk = false;
    }
    return parseOk;
  }

  bool hasMatch() const { return !xpath.empty(); }
  const std::string& getXPath() const { return xpath; }

  size_t write(uint8_t c) override { return write(&c, 1); }

  size_t write(const uint8_t* buffer, size_t size) override {
    if (!parser || !parseOk || stopped) {
      return size;
    }

    if (!parser.parse(buffer, size, false)) {
      if (!parser.stopped()) {
        LOG_ERR("KOX", "XML parse error: %s", parser.errorString());
        parseOk = false;
      }
    }

    return size;
  }

  int spineIndex = 0;

 private:
  static void startElement(void* userData, const char* name, const char**) {
    auto* self = static_cast<XPathProgressResolver*>(userData);
    self->onStartElement(name);
  }

  static void endElement(void* userData, const char* name) {
    auto* self = static_cast<XPathProgressResolver*>(userData);
    self->onEndElement(name);
  }

  static void characterData(void* userData, const char* data, const int len) {
    auto* self = static_cast<XPathProgressResolver*>(userData);
    self->onCharacterData(data, len);
  }

  static void comment(void* userData, const char*) {
    auto* self = static_cast<XPathProgressResolver*>(userData);
    self->onMarkupBoundary();
  }

  static void processingInstruction(void* userData, const char*, const char*) {
    auto* self = static_cast<XPathProgressResolver*>(userData);
    self->onMarkupBoundary();
  }

  static void startCdataSection(void* userData) {
    auto* self = static_cast<XPathProgressResolver*>(userData);
    self->onMarkupBoundary();
  }

  static void endCdataSection(void* userData) {
    auto* self = static_cast<XPathProgressResolver*>(userData);
    self->onMarkupBoundary();
  }

  void onStartElement(const char* rawName) {
    const std::string name = stripPrefix(rawName);

    if (!insideBody) {
      if (name == "body") {
        insideBody = true;
        bodyDepth = depth;
        parentStates.emplace_back();
        textNodeIndexStack.push_back(0);
      }
      depth++;
      return;
    }

    pendingTextNode = true;
    const int siblingIndex = parentStates.back().nextIndex(name);
    path.push_back({name, siblingIndex});
    parentStates.emplace_back();
    textNodeIndexStack.push_back(0);

    if (nonVisibleDepth > 0 || VisibleTextUtils::isNonVisibleElement(name)) {
      nonVisibleDepth++;
    }

    depth++;
  }

  void onEndElement(const char* rawName) {
    const std::string name = stripPrefix(rawName);

    depth--;
    if (!insideBody) {
      return;
    }

    if (depth == bodyDepth && name == "body") {
      insideBody = false;
      parentStates.clear();
      path.clear();
      textNodeIndexStack.clear();
      nonVisibleDepth = 0;
      return;
    }

    if (nonVisibleDepth > 0) {
      nonVisibleDepth--;
    }

    pendingTextNode = true;
    if (!textNodeIndexStack.empty()) {
      textNodeIndexStack.pop_back();
    }
    if (!path.empty()) {
      path.pop_back();
    }
    if (!parentStates.empty()) {
      parentStates.pop_back();
    }
  }

  // Visible-offset counting mirrors ChapterHtmlSlimParser::characterData (every body
  // codepoint outside non-visible elements), so offsets recorded by the page LUT line up
  // with what is counted here regardless of which elements hold the text.
  void onCharacterData(const char* data, const int len) {
    if (!insideBody || nonVisibleDepth > 0 || len <= 0 || stopped) {
      return;
    }

    const size_t codepointCount = countUtf8Codepoints(data, len);
    if (codepointCount == 0) {
      return;
    }

    const bool whitespaceOnly = isWhitespaceOnly(data, len);

    // Start a new text node on first non-empty content after any structural boundary.
    // Only counting non-empty nodes matches KOReader's text()[N] indexing behavior,
    // which skips empty text nodes created by bare <a id="anchor"/> anchors. crengine
    // also drops a whitespace-only run that is the first child of a block element, but
    // "block" there comes from the computed style (display, white-space), which this
    // resolver does not see, so such a run is counted here like any other.
    if (pendingTextNode) {
      if (!textNodeIndexStack.empty()) {
        textNodeIndexStack.back()++;
      }
      textNodeStartChars = visibleChars;
      pendingTextNode = false;
    }

    // Whitespace-only runs are never used as anchors: a target that falls inside one is
    // carried forward and resolves at the start of the next text run instead.
    const size_t nextVisibleChars = visibleChars + codepointCount;
    const bool targetReached = boundaryMode == BoundaryMode::Inclusive ? targetVisibleChar <= nextVisibleChars
                                                                       : targetVisibleChar < nextVisibleChars;
    if (targetReached && !whitespaceOnly) {
      const size_t delta = targetVisibleChar > visibleChars ? targetVisibleChar - visibleChars : 0;
      const int texNode = textNodeIndexStack.empty() ? 0 : textNodeIndexStack.back();
      const size_t charOff = visibleChars - textNodeStartChars + delta;
      xpath = buildParagraphXPath(spineIndex, path, texNode, charOff);
      stopped = true;
      parser.stop();
      return;
    }

    visibleChars = nextVisibleChars;
  }

  void onMarkupBoundary() {
    if (!insideBody || nonVisibleDepth > 0 || stopped) {
      return;
    }

    pendingTextNode = true;
  }

  XmlParser parser;
  const size_t targetVisibleChar;
  const BoundaryMode boundaryMode;
  bool parseOk = true;
  bool insideBody = false;
  bool stopped = false;
  bool pendingTextNode = true;
  int depth = 0;
  int bodyDepth = -1;
  uint16_t nonVisibleDepth = 0;
  size_t visibleChars = 0;
  size_t textNodeStartChars = 0;
  std::vector<int> textNodeIndexStack;
  std::vector<ParentState> parentStates;
  std::vector<PathSegment> path;
  std::string xpath;
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

  XPathParagraphResolver resolver(paragraphIndex);
  if (!resolver.ok()) {
    return "";
  }

  resolver.spineIndex = spineIndex;
  if (!epub->readItemContentsToStream(href, resolver, 1024) || !resolver.finish()) {
    return "";
  }

  if (resolver.hasMatch()) {
    LOG_DBG("KOX", "Resolved paragraph %u in spine %d -> %s", paragraphIndex, spineIndex, resolver.getXPath().c_str());
    return resolver.getXPath();
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

  XPathProgressResolver resolver(visibleTextOffset);
  if (!resolver.ok()) {
    return "";
  }

  resolver.spineIndex = spineIndex;
  if (!epub->readItemContentsToStream(href, resolver, 1024) || !resolver.finish()) {
    return "";
  }

  if (resolver.hasMatch()) {
    LOG_DBG("KOX", "Resolved visible offset %u in spine %d -> %s", visibleTextOffset, spineIndex,
            resolver.getXPath().c_str());
    return resolver.getXPath();
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

  ParagraphTextCounter counter;
  if (!counter.ok() || !epub->readItemContentsToStream(href, counter, 1024) || !counter.finish()) {
    return "";
  }

  const size_t totalVisibleChars = counter.totalVisibleChars();
  if (totalVisibleChars == 0) {
    return "";
  }

  const float clamped = std::max(0.0f, std::min(1.0f, intraSpineProgress));
  const size_t targetVisibleChar =
      std::max<size_t>(1, std::min(totalVisibleChars, static_cast<size_t>(std::ceil(clamped * totalVisibleChars))));

  XPathProgressResolver resolver(targetVisibleChar, XPathProgressResolver::BoundaryMode::Inclusive);
  if (!resolver.ok()) {
    return "";
  }

  resolver.spineIndex = spineIndex;
  if (!epub->readItemContentsToStream(href, resolver, 1024) || !resolver.finish()) {
    return "";
  }

  if (resolver.hasMatch()) {
    LOG_DBG("KOX", "Resolved progress %.3f in spine %d -> %s", intraSpineProgress, spineIndex,
            resolver.getXPath().c_str());
    return resolver.getXPath();
  }

  LOG_DBG("KOX", "Could not resolve progress %.3f in spine %d", intraSpineProgress, spineIndex);
  return "";
}
