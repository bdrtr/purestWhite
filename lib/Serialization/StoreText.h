#pragma once

#include <Arduino.h>
#include <StoredPassword.h>
#include <bembeyaz/store.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <string>
#include <vector>

// What the stores whose JSON is Rust's share at the C ABI (rust/include/bembeyaz/store.h).

static_assert(sizeof(bb_store_text) == 2 * sizeof(void*) && offsetof(bb_store_text, len) == sizeof(void*),
              "bb_store_text layout");
static_assert(sizeof(bb_store_number) == 8 && offsetof(bb_store_number, byte) == 1 &&
                  offsetof(bb_store_number, value) == 4,
              "bb_store_number layout");

// The JSON text Rust writes, appended to json; ok turns false when the String cannot grow.
struct StoreJsonSink {
  String& json;
  bool ok;
};

// A bb_store_write that appends to a StoreJsonSink.
inline void appendStoreJson(const char* data, const size_t len, void* context) {
  auto* sink = static_cast<StoreJsonSink*>(context);
  sink->ok = sink->ok && sink->json.concat(data, static_cast<unsigned int>(len));
}

// A view of s, valid while s is not changed.
inline bb_store_text storeText(const std::string& s) { return bb_store_text{s.data(), s.size()}; }
inline bb_store_text storeText(const String& s) { return bb_store_text{s.c_str(), s.length()}; }

// Texts obfuscated for saving (ObfuscationUtils), kept while Rust writes them: each add() gives a
// view that stays valid while this lives. ok turns false when one could not be made (memory ran
// out), so the store saves nothing rather than lose a password.
class ObfuscatedTexts {
 public:
  bb_store_text add(const std::string& plain);
  bool ok() const { return ok_; }

 private:
  std::deque<String> texts_;
  bool ok_ = true;
};

// A copy of t's bytes.
inline std::string storeString(const bb_store_text& t) { return t.len ? std::string(t.data, t.len) : std::string(); }

// obfuscation::readStoredPassword over the stored texts Rust hands over.
inline std::string readStoredPassword(const bb_store_text& passwordObf, const bb_store_text& legacyPassword,
                                      const size_t maxLength, bool& needsResave, bool& valid) {
  return obfuscation::readStoredPassword(storeString(passwordObf).c_str(), storeString(legacyPassword).c_str(),
                                         maxLength, needsResave, valid);
}

// The same with no length limit.
inline std::string readStoredPassword(const bb_store_text& passwordObf, const bb_store_text& legacyPassword,
                                      bool& needsResave) {
  return obfuscation::readStoredPassword(storeString(passwordObf).c_str(), storeString(legacyPassword).c_str(),
                                         needsResave);
}

// A number field read with `| fallback` for a uint8_t: its value when it is an integer within
// uint8_t, else fallback.
inline uint8_t storeByteOr(const bb_store_number& n, const uint8_t fallback) {
  return n.kind == BB_STORE_U32 && n.value <= 0xFF ? static_cast<uint8_t>(n.value) : fallback;
}

// The entries a store keeps from the records Rust reads (bb_store_records_read): parse makes an
// entry of a record's fields, or refuses it; at most `most` are kept, as the stores count them.
template <typename Entry>
struct KeptRecords {
  std::vector<Entry>& entries;
  size_t most;
  bool (*parse)(const bb_store_text* fields, Entry& entry);
};

// A bb_store_record_read for a KeptRecords<Entry>.
template <typename Entry>
void keepRecord(const bb_store_text* fields, const size_t count, void* context) {
  auto& kept = *static_cast<KeptRecords<Entry>*>(context);
  if (kept.entries.empty()) kept.entries.reserve(std::min(count, kept.most));
  if (kept.entries.size() >= kept.most) return;
  Entry entry;
  if (kept.parse(fields, entry)) kept.entries.push_back(std::move(entry));
}
