#include "DictHtmlPages.h"

#include <Arduino.h>
#include <Epub/parsers/ChapterHtmlSlimParser.h>
#include <HalStorage.h>
#include <Logging.h>
#include <Memory.h>
#include <bembeyaz/html.h>

#include <algorithm>
#include <cstring>

#include "CrossPointSettings.h"

namespace {

// Normalized XHTML staged here for the file-driven parser; truncated on each
// use, removed after the parse.
constexpr const char* TMP_HTML_PATH = "/.crosspoint/dicthtml.tmp";

// ENTRY gate: is there room to start a styled layout at all? Keeps enough
// contiguous heap for the parser's 16KB SD-font advance scratch plus
// page/layout allocations. Falling back to plain text is cheaper than entering
// a throwing allocation path under pressure.
constexpr size_t MIN_STYLED_FREE_HEAP = 40 * 1024;
constexpr size_t MIN_STYLED_MAX_ALLOC = 20 * 1024;

// RETAIN gate: is there still room to keep the pages coming? Checked per
// completed page, and necessarily much lower than the entry gate, because it
// measures a different heap: the parser is alive and holding its working set.
//
// Measured on an X4 with a 2554-byte definition: 50772 bytes free on entry,
// 29400 one page in. The parse and layout cost ~21KB while they run, all of it
// returned when the parser is destroyed. Testing the entry number here charged
// that cost against the gate deciding whether the layout may continue, so the
// styled path refused its own first page unless entry heap was around 61KB --
// which, stacked over the reader, it never is. Every HTML definition silently
// took the plain-text path.
//
// The pages actually retained are bounded by the two count caps below, so this
// only has to catch genuine exhaustion. A single page's layout was seen costing
// ~8KB between two checks, so 16KB keeps about that much in hand at the low
// point while still sitting far below any successful entry heap.
constexpr size_t MIN_STYLED_RETAIN_HEAP = 16 * 1024;
constexpr size_t MIN_STYLED_RETAIN_ALLOC = 8 * 1024;

// Bound retained layout independently of input bytes: compact markup can emit
// far more objects than its source size suggests.
constexpr size_t MAX_STYLED_PAGES = 64;
constexpr size_t MAX_STYLED_PAGE_ELEMENTS = 512;

class BufferedFileWriter {
 public:
  explicit BufferedFileWriter(HalFile& file) : file(file) {}

  bool append(const char* data, size_t len) {
    while (len > 0) {
      const size_t available = sizeof(buffer) - used;
      const size_t chunk = std::min(available, len);
      memcpy(buffer + used, data, chunk);
      used += chunk;
      data += chunk;
      len -= chunk;
      if (used == sizeof(buffer) && !flush()) return false;
    }
    return true;
  }

  bool flush() {
    if (used == 0) return true;
    if (file.write(buffer, used) != used) return false;
    used = 0;
    return true;
  }

 private:
  HalFile& file;
  // Fixed stack staging avoids an expansion-sized XHTML heap allocation.
  char buffer[128] = {};
  size_t used = 0;
};

// StarDict HTML is tag soup; the XML parser is strict. bembeyaz-html's xhtml module makes the
// fragment a well-formed XHTML document (html.h); structural damage it cannot repair (mismatched
// tags, unquoted attribute values) surfaces as a parse error and the caller falls back to the
// plain-text path. test/dict_html mirrors this: change both.
bool writeNormalizedXhtml(const std::string& html, HalFile& file) {
  BufferedFileWriter out(file);
  return bb_html_normalize_xhtml(
             html.data(), html.size(),
             [](void* context, const char* piece, const size_t len) {
               return static_cast<BufferedFileWriter*>(context)->append(piece, len);
             },
             &out) &&
         out.flush();
}

}  // namespace

bool buildDictionaryHtmlPages(GfxRenderer& renderer, const std::string& definition, const uint16_t viewportWidth,
                              const uint16_t viewportHeight, std::vector<std::unique_ptr<Page>>& pagesOut) {
  if (ESP.getFreeHeap() < MIN_STYLED_FREE_HEAP || ESP.getMaxAllocHeap() < MIN_STYLED_MAX_ALLOC) {
    LOG_ERR("DHTML", "Low heap for styled definition (%u free, %u max block)", ESP.getFreeHeap(),
            ESP.getMaxAllocHeap());
    return false;
  }

  {
    HalFile tmp = Storage.open(TMP_HTML_PATH, O_WRITE | O_CREAT | O_TRUNC);
    if (!tmp) {
      LOG_ERR("DHTML", "Cannot create %s", TMP_HTML_PATH);
      return false;
    }
    if (!writeNormalizedXhtml(definition, tmp)) {
      LOG_ERR("DHTML", "Short write to %s", TMP_HTML_PATH);
      return false;
    }
  }  // destructor closes the file before the parser reopens the same path

  pagesOut.clear();
  // One fixed allocation (256 bytes on C3); pages must outlive the parser.
  pagesOut.reserve(MAX_STYLED_PAGES);

  bool ok = false;
  bool resourceLimitHit = false;
  const char* limitReason = nullptr;
  size_t retainedElements = 0;
  {
    const std::string tmpPath = TMP_HTML_PATH;  // the parser stores a reference
    // Heap-allocated as Section does — the parser object is far too large for
    // a stack local. Null epub is safe: imageRendering=2 suppresses <img>
    // handling, the only path that dereferences it.
    auto parser = makeUniqueNoThrow<ChapterHtmlSlimParser>(
        nullptr, tmpPath, renderer, SETTINGS.getReaderFontId(), SETTINGS.getReaderLineCompression(),
        SETTINGS.extraParagraphSpacing, SETTINGS.paragraphAlignment, viewportWidth, viewportHeight,
        SETTINGS.hyphenationEnabled, SETTINGS.focusReadingEnabled,
        [&pagesOut, &resourceLimitHit, &retainedElements, &limitReason](std::unique_ptr<Page> page, uint16_t, uint16_t,
                                                                        uint32_t) {
          if (resourceLimitHit) return;
          const size_t pageElements = page->elements.size();
          // Name the limit that fired. The three causes mean different things --
          // the count caps say the definition is genuinely too big to hold,
          // while the heap floor says only that this moment was a bad one -- and
          // a single "exceeded the budget" message cannot tell them apart.
          if (pagesOut.size() >= MAX_STYLED_PAGES) {
            limitReason = "page count";
          } else if (pageElements > MAX_STYLED_PAGE_ELEMENTS - retainedElements) {
            limitReason = "element count";
          } else if (ESP.getFreeHeap() < MIN_STYLED_RETAIN_HEAP || ESP.getMaxAllocHeap() < MIN_STYLED_RETAIN_ALLOC) {
            limitReason = "free heap";
          }
          if (limitReason != nullptr) {
            LOG_ERR("DHTML", "Styled definition stopped on %s (pages=%u elements=%u free=%u contig=%u)", limitReason,
                    static_cast<unsigned>(pagesOut.size()), static_cast<unsigned>(retainedElements + pageElements),
                    ESP.getFreeHeap(), ESP.getMaxAllocHeap());
            resourceLimitHit = true;
            pagesOut.clear();
            return;
          }
          retainedElements += pageElements;
          pagesOut.push_back(std::move(page));
        },
        /*embeddedStyle=*/false, /*contentBase=*/"", /*imageBasePath=*/"", /*imageRendering=*/2);
    if (!parser) {
      LOG_ERR("DHTML", "OOM: ChapterHtmlSlimParser");
    } else {
      parser->setTextSpacing(SETTINGS.getCharacterSpacing(), SETTINGS.wordSpacing);
      parser->setParagraphIndentSpaces(SETTINGS.paragraphIndentSpaces);
      ok = parser->parseAndBuildPages();  // closes the file on both outcomes
    }
  }
  Storage.remove(TMP_HTML_PATH);

  if (resourceLimitHit) {
    LOG_ERR("DHTML", "Styled definition exceeded page heap budget");
  }
  if (!ok || resourceLimitHit || pagesOut.empty()) {
    if (!ok) {
      LOG_ERR("DHTML", "Definition parse/layout failed");
    } else if (!resourceLimitHit && pagesOut.empty()) {
      LOG_ERR("DHTML", "Definition produced no styled pages");
    }
    pagesOut.clear();
    return false;
  }
  return true;
}
