#include "KOReaderSyncMessages.h"

#include <bembeyaz/kosync_messages.h>

#include <cstddef>

// The JSON is Rust's (bembeyaz_kosync::messages, over bembeyaz-jsondoc, which reads and writes as
// ArduinoJson did); this hands it the fields and takes back the text.
namespace kosync {

namespace {
// The layouts and numbers bembeyaz-ffi's kosync_messages.rs checks on its side.
constexpr bool kNarrow = sizeof(void*) == 4;
static_assert(sizeof(bb_kosync_update) == (kNarrow ? 116 : 200), "bb_kosync_update layout");
static_assert(offsetof(bb_kosync_update, filename) == (kNarrow ? 24 : 40), "bb_kosync_update layout");
static_assert(offsetof(bb_kosync_update, series_index) == (kNarrow ? 84 : 156), "bb_kosync_update layout");
static_assert(offsetof(bb_kosync_update, pct_q) == (kNarrow ? 92 : 164), "bb_kosync_update layout");
static_assert(offsetof(bb_kosync_update, pages) == (kNarrow ? 100 : 172), "bb_kosync_update layout");
static_assert(offsetof(bb_kosync_update, para) == (kNarrow ? 104 : 176), "bb_kosync_update layout");
static_assert(offsetof(bb_kosync_update, xpath) == (kNarrow ? 108 : 184), "bb_kosync_update layout");
static_assert(sizeof(bb_kosync_received) == 40, "bb_kosync_received layout");
static_assert(offsetof(bb_kosync_received, timestamp) == 8, "bb_kosync_received layout");
static_assert(offsetof(bb_kosync_received, has_position) == 16, "bb_kosync_received layout");
static_assert(offsetof(bb_kosync_received, pct_q) == 20, "bb_kosync_received layout");
static_assert(offsetof(bb_kosync_received, has_para) == 30, "bb_kosync_received layout");
static_assert(offsetof(bb_kosync_received, para) == 32, "bb_kosync_received layout");
static_assert(BB_KOSYNC_FIELD_PROGRESS == 0 && BB_KOSYNC_FIELD_DEVICE == 1 && BB_KOSYNC_FIELD_DEVICE_ID == 2 &&
                  BB_KOSYNC_FIELD_XPATH == 3,
              "bb_kosync field numbers");

bb_kosync_text textOf(const std::string& s) { return bb_kosync_text{s.data(), s.size()}; }

void append(const char* data, const size_t len, void* context) {
  static_cast<std::string*>(context)->append(data, len);
}

bb_kosync_update updateOf(const KOReaderProgress& p) {
  bb_kosync_update u{};
  u.document = textOf(p.document);
  u.progress = textOf(p.progress);
  u.percentage = p.percentage;
  if (p.metadata.has_value()) {
    const KOReaderMetadata& m = *p.metadata;
    u.has_metadata = true;
    u.filename = textOf(m.filename);
    u.title = textOf(m.title);
    u.authors = textOf(m.authors);
    u.extra_json = textOf(m.extraJson);
    u.isbn = textOf(m.isbn);
    u.asin = textOf(m.asin);
    u.series = textOf(m.series);
    u.has_series_index = m.seriesIndex.has_value();
    u.series_index = m.seriesIndex.value_or(0.0f);
  }
  if (p.position.has_value()) {
    const KOReaderRichPosition& r = *p.position;
    u.has_position = true;
    u.pct_q = r.pctQ;
    u.spine = r.spineIndex;
    u.page = r.pageNumber;
    u.pages = r.totalPages;
    u.has_para = r.paragraphIndex.has_value();
    u.para = r.paragraphIndex.value_or(0);
    u.xpath = textOf(r.xpath);
  }
  return u;
}

// The text fields of a progress answer, as Rust hands them over.
struct Fields {
  KOReaderProgress* out;
  std::string xpath;
};

void field(const int which, const char* data, const size_t len, void* context) {
  auto* fields = static_cast<Fields*>(context);
  switch (which) {
    case BB_KOSYNC_FIELD_PROGRESS:
      fields->out->progress.assign(data, len);
      break;
    case BB_KOSYNC_FIELD_DEVICE:
      fields->out->device.assign(data, len);
      break;
    case BB_KOSYNC_FIELD_DEVICE_ID:
      fields->out->deviceId.assign(data, len);
      break;
    case BB_KOSYNC_FIELD_XPATH:
      fields->xpath.assign(data, len);
      break;
    default:
      break;
  }
}
}  // namespace

std::string createUserBody(const std::string& username, const std::string& md5Password) {
  std::string body;
  bb_kosync_create_user_body(textOf(username), textOf(md5Password), append, &body);
  return body;
}

std::string progressBody(const KOReaderProgress& progress, const bool extendedMetadata, const bool richProgress) {
  const bb_kosync_update update = updateOf(progress);
  std::string body;
  bb_kosync_progress_body(&update, extendedMetadata, richProgress, append, &body);
  return body;
}

const char* parseProgress(const char* body, const std::string& documentHash, const bool richProgress,
                          KOReaderProgress& out) {
  bb_kosync_received received{};
  KOReaderProgress parsed{};
  Fields fields{&parsed, {}};
  const char* error = bb_kosync_parse_progress(body, body ? std::char_traits<char>::length(body) : 0, richProgress,
                                               &received, field, &fields);
  if (error) return error;

  out.document = documentHash;
  out.progress = std::move(parsed.progress);
  out.percentage = received.percentage;
  out.device = std::move(parsed.device);
  out.deviceId = std::move(parsed.deviceId);
  out.timestamp = received.timestamp;
  out.position.reset();
  if (received.has_position) {
    KOReaderRichPosition rich;
    rich.pctQ = received.pct_q;
    rich.spineIndex = received.spine;
    rich.pageNumber = received.page;
    rich.totalPages = received.pages;
    if (received.has_para) rich.paragraphIndex = received.para;
    rich.xpath = std::move(fields.xpath);
    out.position = std::move(rich);
  }
  return nullptr;
}

}  // namespace kosync
