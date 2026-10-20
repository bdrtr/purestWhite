#include "OtherFirmware.h"

#include <Logging.h>
#include <esp_app_format.h>
#include <esp_ota_ops.h>
#include <soc/soc.h>

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <memory>
#include <new>

#include "FirmwareFlasher.h"
#include "OtaBootSwitch.h"
#include "util/FirmwareName.h"

namespace other_firmware {

bool find(Info& out) {
  const esp_partition_t* other = esp_ota_get_next_update_partition(nullptr);
  if (!other || other == esp_ota_get_running_partition()) return false;
  esp_app_desc_t desc;
  if (esp_ota_get_partition_description(other, &desc) != ESP_OK) return false;
  out.partition = other;
  return true;
}

namespace {
// The NUL-terminated text that follows each prefix in buf[0, len), until fn
// accepts one. A prefix alone (a scanner's own needle) or a text cut off by
// the end of the read is skipped.
template <typename Fn>
bool forEachTextAfter(const char* buf, const size_t len, const char* prefix, Fn&& fn) {
  const size_t prefixLen = strlen(prefix);
  for (const char* hit = static_cast<const char*>(memmem(buf, len, prefix, prefixLen)); hit;
       hit = static_cast<const char*>(memmem(hit + 1, len - (hit + 1 - buf), prefix, prefixLen))) {
    const char* start = hit + prefixLen;
    const size_t room = len - (start - buf);
    const size_t textLen = strnlen(start, room);
    if (textLen == 0 || textLen == room) continue;
    if (fn(start, textLen)) return true;
  }
  return false;
}
}  // namespace

void name(const Info& info, char* out, const size_t outSize) {
  if (outSize == 0) return;
  out[0] = '\0';
  static constexpr char CROSSPOINT_PREFIX[] = "CrossPoint-ESP32-";
  static constexpr char CROSSPOINT_NAME[] = "CrossPoint ";
  constexpr size_t CHUNK = 4096;
  // Each read overlaps the previous one by enough to hold a prefix and a name.
  constexpr size_t OVERLAP = 96;
  std::unique_ptr<char[]> buf(new (std::nothrow) char[CHUNK + 1]);
  if (!buf) return;
  // Every image of this firmware also carries the CrossPoint user agent, so
  // that one only counts when the image has no ID.
  char crosspoint[48] = {};
  const auto keepCrosspoint = [&](const char* text, const size_t textLen) {
    if (sizeof(CROSSPOINT_NAME) - 1 + textLen >= sizeof(crosspoint)) return false;
    snprintf(crosspoint, sizeof(crosspoint), "%s%.*s", CROSSPOINT_NAME, static_cast<int>(textLen), text);
    return true;
  };
  const auto takeId = [&](const char* text, const size_t textLen) {
    if (textLen >= outSize) return false;
    memcpy(out, text, textLen);
    out[textLen] = '\0';
    return true;
  };
  // True once the ID is found, or a read fails.
  const auto scan = [&](const size_t start, const size_t size) {
    for (size_t pos = 0; pos < size; pos += CHUNK - OVERLAP) {
      const size_t len = std::min<size_t>(CHUNK, size - pos);
      if (esp_partition_read(info.partition, start + pos, buf.get(), len) != ESP_OK) return true;
      buf[len] = '\0';
      if (forEachTextAfter(buf.get(), len, firmware_name::ID_PREFIX, takeId)) return true;
      if (!crosspoint[0]) forEachTextAfter(buf.get(), len, CROSSPOINT_PREFIX, keepCrosspoint);
      if (len < CHUNK) break;
    }
    return false;
  };
  // Strings live in the flash data (DROM) segments: about 2.6 MB of the 6.5 MB slot.
  esp_image_header_t header;
  if (esp_partition_read(info.partition, 0, &header, sizeof(header)) != ESP_OK) return;
  size_t offset = sizeof(header);
  for (uint8_t i = 0; i < header.segment_count && i < ESP_IMAGE_MAX_SEGMENTS; i++) {
    esp_image_segment_header_t segment;
    if (offset + sizeof(segment) > info.partition->size ||
        esp_partition_read(info.partition, offset, &segment, sizeof(segment)) != ESP_OK) {
      break;
    }
    const size_t dataOffset = offset + sizeof(segment);
    if (segment.data_len > info.partition->size - dataOffset) break;
    offset = dataOffset + segment.data_len;
    if (segment.load_addr < SOC_DROM_LOW || segment.load_addr >= SOC_DROM_HIGH) continue;
    if (scan(dataOffset, segment.data_len)) return;
  }
  if (strlen(crosspoint) < outSize) strcpy(out, crosspoint);
}

bool select(const Info& info) {
  const auto result = firmware_flash::validateImagePartition(info.partition);
  if (result != firmware_flash::Result::OK) {
    LOG_ERR("BOOT", "other firmware in %s fails its check: %s", info.partition->label,
            firmware_flash::resultName(result));
    return false;
  }
  LOG_INF("BOOT", "starting the firmware in %s", info.partition->label);
  return ota_boot::switchTo(info.partition);
}

}  // namespace other_firmware
