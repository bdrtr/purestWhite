#include "PersistableStore.h"

#include <HalStorage.h>
#include <Logging.h>
#include <StoredPassword.h>

#include <limits>

bool PersistableStoreBase::writeDocToFile(const char* path, const JsonDocument& doc) {
  String json;
  serializeJson(doc, json);
  return writeTextToFile(path, json);
}

bool PersistableStoreBase::writeTextToFile(const char* path, const String& json) {
  Storage.mkdir("/.crosspoint");
  if (!Storage.writeFile(path, json)) {
    LOG_ERR("PERSIST", "Failed to write %s", path);
    return false;
  }
  return true;
}

bool PersistableStoreBase::readTextFromFile(const char* path, String& json) {
  if (!Storage.exists(path)) {
    return false;  // Expected on first boot — not an error.
  }
  json = Storage.readFile(path);
  if (json.isEmpty()) {
    LOG_ERR("PERSIST", "Failed to read %s (empty)", path);
    return false;
  }
  return true;
}

bool PersistableStoreBase::readDocFromFile(const char* path, JsonDocument& doc) {
  String json;
  if (!readTextFromFile(path, json)) {
    return false;
  }
  auto error = deserializeJson(doc, json);
  if (error) {
    LOG_ERR("PERSIST", "JSON parse error in %s: %s", path, error.c_str());
    return false;
  }
  return true;
}

std::string PersistableStoreBase::extractPassword(JsonVariantConst doc, bool& needsResave) {
  bool valid = false;
  return extractPassword(doc, needsResave, std::numeric_limits<size_t>::max(), valid);
}

std::string PersistableStoreBase::extractPassword(JsonVariantConst doc, bool& needsResave, const size_t maxLength,
                                                  bool& valid) {
  return obfuscation::readStoredPassword(doc["password_obf"] | "", doc["password"] | "", maxLength, needsResave, valid);
}
