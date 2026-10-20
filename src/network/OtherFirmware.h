#pragma once

#include <esp_partition.h>

// The firmware in the OTA slot that is not running: the one an SD card update
// replaced. It stays in flash, so the device can start it again without
// reflashing (two firmwares side by side).
namespace other_firmware {

struct Info {
  const esp_partition_t* partition = nullptr;
};

// Whether the other slot holds an app; false when it holds none. Cheap: reads
// its header only, and does not check the image.
bool find(Info& out);

// The firmware and version the image names: "purestWhite next 0.0.1" from
// this firmware's ID, or "CrossPoint 1.6.5" from the "CrossPoint-ESP32-<version>"
// user agent every CrossPoint build carries (the app description names the
// Arduino library build instead). Empty when neither is found. Reads the
// image's flash data segments.
void name(const Info& info, char* out, size_t outSize);

// Checks the whole image (segments, checksum, SHA-256, chip and board) and
// makes it the next boot. The caller restarts.
bool select(const Info& info);

}  // namespace other_firmware
