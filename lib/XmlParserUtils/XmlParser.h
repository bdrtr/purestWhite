#pragma once

#include <bembeyaz/xml.h>

#include <climits>
#include <cstddef>
#include <cstdint>

// Streaming XML parser (rust/crates/bembeyaz-xml), with the events expat reported for the same
// documents. Names keep their prefixes. Text arrives in pieces, line ends as '\n'.
class XmlParser {
 public:
  using StartElementHandler = void (*)(void* userData, const char* name, const char** atts);
  using EndElementHandler = void (*)(void* userData, const char* name);
  using CharacterDataHandler = void (*)(void* userData, const char* s, int len);
  using CommentHandler = void (*)(void* userData, const char* data);
  using ProcessingInstructionHandler = void (*)(void* userData, const char* target, const char* data);
  using CdataSectionHandler = void (*)(void* userData);
  // A reference to an entity nobody declared, when the document has an external DTD subset (which
  // is not read) or after skipUndefinedEntities(); otherwise such a reference is an error.
  using SkippedEntityHandler = void (*)(void* userData, const char* name);

  // Byte limit for one token, for the open element names, and for the DTD; past it the parse
  // fails with BB_XML_NO_MEMORY.
  static constexpr size_t kDefaultLimit = BB_XML_DEFAULT_LIMIT;

  XmlParser() = default;
  ~XmlParser() { destroy(); }
  XmlParser(const XmlParser&) = delete;
  XmlParser& operator=(const XmlParser&) = delete;

  // False when out of memory.
  bool create(const size_t limit = kDefaultLimit) {
    destroy();
    parser_ = bb_xml_new(limit);
    error_ = 0;
    stopped_ = false;
    return parser_ != nullptr;
  }
  void destroy() {
    bb_xml_free(parser_);
    parser_ = nullptr;
  }
  explicit operator bool() const { return parser_ != nullptr; }

  void setUserData(void* userData) { userData_ = userData; }
  void setElementHandler(const StartElementHandler start, const EndElementHandler end) {
    start_ = start;
    end_ = end;
  }
  void setCharacterDataHandler(const CharacterDataHandler handler) { text_ = handler; }
  void setCommentHandler(const CommentHandler handler) { comment_ = handler; }
  void setProcessingInstructionHandler(const ProcessingInstructionHandler handler) { pi_ = handler; }
  void setCdataSectionHandler(const CdataSectionHandler start, const CdataSectionHandler end) {
    cdataStart_ = start;
    cdataEnd_ = end;
  }
  void setSkippedEntityHandler(const SkippedEntityHandler handler) { skipped_ = handler; }
  // Reports references to entities nobody declared to the skipped entity handler instead of
  // failing, even in a standalone document. Call after create(), before parsing.
  void skipUndefinedEntities() { bb_xml_skip_undefined_entities(parser_); }

  // Parses the next len bytes of the document; `last` when nothing follows. False when the
  // document failed or a handler called stop(); error() says which.
  bool parse(const void* data, const size_t len, const bool last) {
    if (!parser_) {
      error_ = BB_XML_NO_MEMORY;
      return false;
    }
    const bb_xml_handlers handlers = {
        start_ ? onStart : nullptr,       end_ ? onEnd : nullptr,         text_ ? onText : nullptr,
        comment_ ? onComment : nullptr,   pi_ ? onPi : nullptr,           cdataStart_ ? onCdataStart : nullptr,
        cdataEnd_ ? onCdataEnd : nullptr, skipped_ ? onSkipped : nullptr,
    };
    error_ = bb_xml_parse(parser_, static_cast<const char*>(data), len, last, &handlers, this);
    return error_ == BB_XML_OK;
  }

  // From a handler: stops the parse, which then fails with BB_XML_ABORTED. For an empty element
  // (<x/>), its end handler still runs.
  void stop() { stopped_ = true; }
  bool stopped() const { return stopped_ && error_ == BB_XML_ABORTED; }

  int error() const { return error_; }
  const char* errorString() const { return bb_xml_error_message(error_); }
  // Where the token that failed starts in the input, in bytes.
  uint64_t errorOffset() const { return parser_ ? bb_xml_error_offset(parser_) : 0; }
  // In a start element handler: where the start tag begins in the input, in bytes.
  uint64_t currentByteIndex() const { return startOffset_; }

 private:
  bb_xml_parser* parser_ = nullptr;
  void* userData_ = nullptr;
  StartElementHandler start_ = nullptr;
  EndElementHandler end_ = nullptr;
  CharacterDataHandler text_ = nullptr;
  CommentHandler comment_ = nullptr;
  ProcessingInstructionHandler pi_ = nullptr;
  CdataSectionHandler cdataStart_ = nullptr;
  CdataSectionHandler cdataEnd_ = nullptr;
  SkippedEntityHandler skipped_ = nullptr;
  uint64_t startOffset_ = 0;
  int error_ = 0;
  bool stopped_ = false;

  static XmlParser& self(void* context) { return *static_cast<XmlParser*>(context); }
  bool goOn() const { return !stopped_; }

  static bool onStart(void* context, const char* name, const char** atts, const uint64_t offset) {
    XmlParser& p = self(context);
    p.startOffset_ = offset;
    p.start_(p.userData_, name, atts);
    return p.goOn();
  }
  static bool onEnd(void* context, const char* name) {
    XmlParser& p = self(context);
    p.end_(p.userData_, name);
    return p.goOn();
  }
  static bool onText(void* context, const char* s, size_t len) {
    XmlParser& p = self(context);
    while (len > 0) {
      const size_t piece = len < static_cast<size_t>(INT_MAX) ? len : static_cast<size_t>(INT_MAX);
      p.text_(p.userData_, s, static_cast<int>(piece));
      s += piece;
      len -= piece;
    }
    return p.goOn();
  }
  static bool onComment(void* context, const char* data) {
    XmlParser& p = self(context);
    p.comment_(p.userData_, data);
    return p.goOn();
  }
  static bool onPi(void* context, const char* target, const char* data) {
    XmlParser& p = self(context);
    p.pi_(p.userData_, target, data);
    return p.goOn();
  }
  static bool onCdataStart(void* context) {
    XmlParser& p = self(context);
    p.cdataStart_(p.userData_);
    return p.goOn();
  }
  static bool onCdataEnd(void* context) {
    XmlParser& p = self(context);
    p.cdataEnd_(p.userData_);
    return p.goOn();
  }
  static bool onSkipped(void* context, const char* name) {
    XmlParser& p = self(context);
    p.skipped_(p.userData_, name);
    return p.goOn();
  }
};
