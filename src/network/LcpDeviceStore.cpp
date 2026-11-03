#include "LcpDeviceStore.h"

#include <ObfuscationUtils.h>
#include <StoreText.h>
#include <bembeyaz/store_records.h>

namespace lcpdevice {

namespace {
// The JSON is Rust's (bembeyaz_store::records): one record, the root object.

// The fields of the record read.
void copyRecord(const bb_store_text* fields, size_t, void* context) {
  auto* out = static_cast<std::string*>(context);
  for (size_t i = 0; i < BB_STORE_LCP_DEVICE_FIELDS; i++) out[i] = storeString(fields[i]);
}
}  // namespace

bool LcpDeviceStore::toJsonText(String& json) const {
  ObfuscatedTexts obfuscated;
  bb_store_text fields[BB_STORE_LCP_DEVICE_FIELDS];
  fields[BB_STORE_LCP_DEVICE_ID] = storeText(deviceId);
  fields[BB_STORE_LCP_DEVICE_PRIV_OBF] = obfuscated.add(privHex);
  if (!obfuscated.ok()) return false;
  StoreJsonSink sink{json, true};
  return bb_store_records_write(BB_STORE_RECORDS_LCP_DEVICE, fields, 1, appendStoreJson, &sink) && sink.ok;
}

const char* LcpDeviceStore::fromJsonText(const String& json) {
  std::string read[BB_STORE_LCP_DEVICE_FIELDS];
  const char* error = bb_store_records_read(json.c_str(), json.length(), BB_STORE_RECORDS_LCP_DEVICE, copyRecord, read);
  if (error) return error;
  deviceId = std::move(read[BB_STORE_LCP_DEVICE_ID]);
  bool ok = false;
  bool tooLong = false;
  privHex = obfuscation::deobfuscateFromBase64(read[BB_STORE_LCP_DEVICE_PRIV_OBF].c_str(), 64, &ok, &tooLong);
  return nullptr;
}

}  // namespace lcpdevice
