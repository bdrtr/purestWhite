#pragma once

#include <cstddef>

// This firmware's own name. Protocol names that servers and plugins expect
// (CrossPoint Sync, the plugin family, the "CrossPoint-ESP32-" user agent)
// stay CrossPoint's.
// The build defines FREEINK_DEVICE_X4PRO=1; BoardConfig.h defines it 0 elsewhere.
#if FREEINK_DEVICE_X4PRO
#define FIRMWARE_NAME "purestWhite next"
#else
#define FIRMWARE_NAME "purestWhite"
#endif

namespace firmware_name {
// Names this image's firmware and version, so the other OTA slot can be named
// by finding it in that image (see OtherFirmware.cpp).
inline constexpr char ID_PREFIX[] = "purestWhite-firmware:";
inline constexpr char ID[] = "purestWhite-firmware:" FIRMWARE_NAME " " CROSSPOINT_VERSION;
// "purestWhite next 0.0.1"
inline constexpr const char* NAME_AND_VERSION = ID + sizeof(ID_PREFIX) - 1;
}  // namespace firmware_name
