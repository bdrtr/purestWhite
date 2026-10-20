#include "OtherFirmware.h"

#include <Logging.h>
#include <esp_ota_ops.h>

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <memory>
#include <new>

#include "FirmwareFlasher.h"
#include "OtaBootSwitch.h"

namespace other_firmware {

bool find(Info& out) {
  const esp_partition_t* other = esp_ota_get_next_update_partition(nullptr);
  if (!other || other == esp_ota_get_running_partition()) return false;
  esp_app_desc_t desc;
  if (esp_ota_get_partition_description(other, &desc) != ESP_OK) return false;
  out.partition = other;
  return true;
}

void version(const Info& info, char* out, const size_t outSize) {
  if (outSize == 0) return;
  out[0] = '\0';
  static constexpr char PREFIX[] = "CrossPoint-ESP32-";
  constexpr size_t PREFIX_LEN = sizeof(PREFIX) - 1;
  constexpr size_t CHUNK = 4096;
  // Each read overlaps the previous one by enough to hold a prefix and a version.
  constexpr size_t OVERLAP = PREFIX_LEN + 32;
  std::unique_ptr<char[]> buf(new (std::nothrow) char[CHUNK + 1]);
  if (!buf) return;
  for (size_t offset = 0; offset < info.partition->size; offset += CHUNK - OVERLAP) {
    const size_t len = std::min<size_t>(CHUNK, info.partition->size - offset);
    if (esp_partition_read(info.partition, offset, buf.get(), len) != ESP_OK) return;
    buf[len] = '\0';
    for (const char* hit = static_cast<const char*>(memmem(buf.get(), len, PREFIX, PREFIX_LEN)); hit;
         hit = static_cast<const char*>(memmem(hit + 1, len - (hit + 1 - buf.get()), PREFIX, PREFIX_LEN))) {
      const char* start = hit + PREFIX_LEN;
      const size_t room = len - (start - buf.get());
      const size_t versionLen = strnlen(start, room);
      // A version ends at its NUL inside this read; the prefix alone (a scanner's own needle) is skipped.
      if (versionLen == 0 || versionLen == room || versionLen >= outSize) continue;
      memcpy(out, start, versionLen);
      out[versionLen] = '\0';
      return;
    }
    if (len < CHUNK) return;
  }
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
