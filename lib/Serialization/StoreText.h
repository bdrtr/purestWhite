#pragma once

#include <Arduino.h>
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

// A bb_store_write that appends to the std::string at context.
inline void appendStoreString(const char* data, const size_t len, void* context) {
  static_cast<std::string*>(context)->append(data, len);
}

// A bb_store_decode over the device's obfuscation (obfuscation::deobfuscateFromBase64, the limit
// checked first); context is unused. Its plain copy is wiped before it is freed.
bool decodeStoredSecret(const char* data, size_t len, size_t most, bb_store_write put, void* putContext, bool* tooLong,
                        void* context);

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

// The entries a store keeps, as Rust loads them (bb_store_records_load): copy makes an entry of a
// record's fields (the secret ones plain).
template <typename Entry>
struct LoadedRecords {
  std::vector<Entry>& entries;
  void (*copy)(const bb_store_text* fields, Entry& entry);
};

// A bb_store_record_read for a LoadedRecords<Entry>.
template <typename Entry>
void addRecord(const bb_store_text* fields, const size_t count, void* context) {
  auto& loaded = *static_cast<LoadedRecords<Entry>*>(context);
  if (loaded.entries.empty()) loaded.entries.reserve(count);
  Entry entry;
  loaded.copy(fields, entry);
  loaded.entries.push_back(std::move(entry));
}
