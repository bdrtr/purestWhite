#pragma once
#include <ArduinoJson.h>
#include <PersistableStore.h>

#include <string>

namespace lcpdevice {

// One enrolled identity per device, persisted like the other credential
// stores: the private key obfuscated with the device key, the id plain.
class LcpDeviceStore : public PersistableStore<LcpDeviceStore> {
 private:
  LcpDeviceStore() = default;
  friend class PersistableStore<LcpDeviceStore>;

 public:
  std::string deviceId;
  std::string privHex;  // 64 hex chars

  static const char* getFilePath() { return "/.crosspoint/lcp_device.json"; }
  void toJson(JsonDocument& doc) const;
  bool fromJson(JsonVariantConst doc);
  bool valid() const { return !deviceId.empty() && privHex.size() == 64; }
};

}  // namespace lcpdevice

#define LCP_DEVICE lcpdevice::LcpDeviceStore::getInstance()
