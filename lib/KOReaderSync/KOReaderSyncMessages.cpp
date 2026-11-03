#include "KOReaderSyncMessages.h"

#include <ArduinoJson.h>

namespace kosync {

std::string createUserBody(const std::string& username, const std::string& md5Password) {
  JsonDocument doc;
  doc["username"] = username;
  doc["password"] = md5Password;
  std::string body;
  serializeJson(doc, body);
  return body;
}

namespace {
void addMetadata(JsonDocument& doc, const KOReaderMetadata& metadata, const bool extendedMetadata) {
  auto meta = doc["metadata"].to<JsonObject>();
  meta["filename"] = metadata.filename;
  meta["title"] = metadata.title;
  meta["authors"] = metadata.authors;
  if (extendedMetadata) {
    if (!metadata.isbn.empty()) meta["isbn"] = metadata.isbn;
    if (!metadata.asin.empty()) meta["asin"] = metadata.asin;
    if (!metadata.series.empty()) meta["series"] = metadata.series;
    if (metadata.seriesIndex.has_value()) meta["series_index"] = *metadata.seriesIndex;
  }

  JsonDocument extra;
  if (!metadata.extraJson.empty() && deserializeJson(extra, metadata.extraJson) == DeserializationError::Ok) {
    for (JsonPairConst kv : extra.as<JsonObjectConst>()) {
      // Flat strings, numbers, and booleans keep their JSON type; null and
      // nested values are skipped, and the reserved keys above always win.
      const JsonVariantConst value = kv.value();
      if (!(value.is<const char*>() || value.is<bool>() || value.is<long long>() || value.is<double>())) continue;
      if (!meta[kv.key().c_str()].isNull()) continue;
      meta[kv.key().c_str()] = value;
    }
  }
}

void addPosition(JsonDocument& doc, const KOReaderRichPosition& p) {
  auto pos = doc["position"].to<JsonObject>();
  pos["pctQ"] = p.pctQ;
  pos["spine"] = p.spineIndex;
  pos["page"] = p.pageNumber;
  pos["pages"] = p.totalPages;
  if (p.paragraphIndex.has_value()) pos["para"] = *p.paragraphIndex;
  // Server rejects the whole position object if xpath exceeds 120 bytes.
  if (!p.xpath.empty() && p.xpath.size() <= 120) pos["xpath"] = p.xpath;
}
}  // namespace

std::string progressBody(const KOReaderProgress& progress, const bool extendedMetadata, const bool richProgress) {
  JsonDocument doc;
  doc["document"] = progress.document;
  if (progress.metadata.has_value()) addMetadata(doc, *progress.metadata, extendedMetadata);
  doc["progress"] = progress.progress;
  doc["percentage"] = progress.percentage;
  doc["device"] = DEVICE_NAME;
  doc["device_id"] = DEVICE_ID;
  // Enhanced position is opt-in by server type; strict KOSync never receives it.
  if (progress.position.has_value() && richProgress) addPosition(doc, *progress.position);

  std::string body;
  serializeJson(doc, body);
  return body;
}

const char* parseProgress(const char* body, const std::string& documentHash, const bool richProgress,
                          KOReaderProgress& out) {
  JsonDocument doc;
  const DeserializationError error = deserializeJson(doc, body);
  if (error) return error.c_str();

  out.document = documentHash;
  out.progress = doc["progress"].as<std::string>();
  out.percentage = doc["percentage"].as<float>();
  out.device = doc["device"].as<std::string>();
  out.deviceId = doc["device_id"].as<std::string>();
  out.timestamp = doc["timestamp"].as<int64_t>();

  out.position.reset();
  if (richProgress) {
    const JsonObjectConst pos = doc["position"].as<JsonObjectConst>();
    if (!pos.isNull()) {
      KOReaderRichPosition rich;
      rich.pctQ = pos["pctQ"].as<uint32_t>();
      rich.spineIndex = pos["spine"].as<uint16_t>();
      rich.pageNumber = pos["page"].as<uint16_t>();
      const uint16_t pages = pos["pages"].as<uint16_t>();
      rich.totalPages = pages > 0 ? pages : 1;
      const uint16_t para = pos["para"].as<uint16_t>();
      if (para > 0) rich.paragraphIndex = para;
      rich.xpath = pos["xpath"].as<const char*>() ? pos["xpath"].as<const char*>() : "";
      out.position = std::move(rich);
    }
  }
  return nullptr;
}

}  // namespace kosync
