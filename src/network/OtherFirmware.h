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

// The CrossPoint version the image names ("1.6.5-x4pro"), read from the
// "CrossPoint-ESP32-<version>" user agent every CrossPoint build carries (its
// app description names the Arduino library build instead). Empty when none
// is found. Reads the whole image.
void version(const Info& info, char* out, size_t outSize);

// Checks the whole image (segments, checksum, SHA-256, chip and board) and
// makes it the next boot. The caller restarts.
bool select(const Info& info);

}  // namespace other_firmware
