#include "LcpDeviceStore.h"

#include <ObfuscationUtils.h>

namespace lcpdevice {

void LcpDeviceStore::toJson(JsonDocument& doc) const {
  doc["device_id"] = deviceId;
  doc["priv_obf"] = obfuscation::obfuscateToBase64(privHex);
}

bool LcpDeviceStore::fromJson(JsonVariantConst doc) {
  deviceId = doc["device_id"] | "";
  bool ok = false;
  bool tooLong = false;
  privHex = obfuscation::deobfuscateFromBase64(doc["priv_obf"] | "", 64, &ok, &tooLong);
  return true;
}

}  // namespace lcpdevice
