#pragma once

#include <Arduino.h>
#include <ArduinoJson.h>
#include <Logging.h>

#include <mutex>
#include <string>
#include <type_traits>
#include <utility>

/**
 * @brief Non-template core of PersistableStore.
 *
 * All ArduinoJson parse/serialize machinery is instantiated once here (in
 * PersistableStore.cpp) instead of in every store's translation unit. GCC
 * emits the JSON serializer/parser templates as local .isra clones per TU
 * (~0.5KB each), so keeping serializeJson/deserializeJson out of the stores
 * is what makes the abstraction flash-neutral.
 */
class PersistableStoreBase {
 protected:
  PersistableStoreBase() = default;
  ~PersistableStoreBase() = default;

  // Serializes saveToFile/loadFromFile against each other across FreeRTOS
  // tasks, so the JSON snapshot cannot tear mid-serialize and two concurrent
  // saves cannot write their documents out of order. Concurrent saves are
  // reachable: the web server task saves settings while the main task can too.
  //
  // It is deliberately held across the SD write. That is safe only because the
  // read path does NOT take it — derived stores build their snapshots (e.g.
  // CrossPointSettings::statusBarSpec) unlocked. If you ever lock this mutex on
  // a read path, you put it on the render path and stall rendering behind SD
  // I/O, and you create a storeMutex/storageMutex ordering hazard. Don't.
  mutable std::mutex storeMutex;

  // fromJson() implementations call this (instead of saveToFile()) when the
  // on-disk JSON used a legacy shape that was upgraded in memory.
  // loadFromFile() performs the save after releasing storeMutex; calling
  // saveToFile() from inside fromJson() would deadlock on storeMutex.
  void requestResave() { resaveRequested = true; }

  bool resaveRequested = false;

 public:
  // Public so non-store JSON files (e.g. per-book bookmarks) can reuse them
  // instead of instantiating serializeJson/deserializeJson in their own TU —
  // that per-TU duplication is exactly what this class exists to prevent.

  // Serializes doc and writes it to path (ensures /.crosspoint exists). Logs on failure.
  static bool writeDocToFile(const char* path, const JsonDocument& doc);

  // Reads path and parses it into doc. Returns false silently when the file
  // does not exist (expected on first boot); logs on read/parse failure.
  static bool readDocFromFile(const char* path, JsonDocument& doc);

  // Like writeDocToFile/readDocFromFile, for a store that makes its own JSON
  // text (one whose JSON is read and written in Rust: see PersistableStore).
  // writeTextToFile creates /.crosspoint and logs on failure.
  static bool writeTextToFile(const char* path, const String& json);
  // False silently when the file does not exist; logs when it is empty.
  static bool readTextFromFile(const char* path, String& json);

 protected:
  /**
   * Helper function for extracting an obfuscated password from a JSON value.
   * Accepts JsonVariantConst so callers can pass either a whole JsonDocument
   * or a JsonObject element (e.g. inside an array iteration).
   * If the decoded password requires a resave (e.g. from plaintext fallback), `needsResave` is set to true.
   */
  static std::string extractPassword(JsonVariantConst doc, bool& needsResave);
  static std::string extractPassword(JsonVariantConst doc, bool& needsResave, size_t maxLength, bool& valid);
  // (A store whose JSON is read in Rust calls obfuscation::readStoredPassword with the stored
  // texts instead: StoredPassword.h.)
};

// Whether a store writes and reads its own JSON text (toJsonText/fromJsonText)
// or an ArduinoJson document (toJson/fromJson).
template <typename T, typename = void>
struct StoresJsonText : std::false_type {};
template <typename T>
struct StoresJsonText<T, std::void_t<decltype(std::declval<T&>().fromJsonText(std::declval<const String&>())),
                                     decltype(std::declval<const T&>().toJsonText(std::declval<String&>()))>>
    : std::true_type {};
template <typename T, typename = void>
struct StoresJsonDoc : std::false_type {};
template <typename T>
struct StoresJsonDoc<T, std::void_t<decltype(std::declval<T&>().fromJson(std::declval<JsonVariantConst>())),
                                    decltype(std::declval<const T&>().toJson(std::declval<JsonDocument&>()))>>
    : std::true_type {};

/**
 * @brief Base class for persistable singletons using CRTP.
 *
 * Derived classes must provide:
 * - A private default constructor
 * - friend class PersistableStore<Derived>;
 * - static const char* getFilePath();
 * - void toJson(JsonDocument& doc) const;
 * - bool fromJson(JsonVariantConst doc);
 * or, for a store whose JSON is read and written in Rust, in place of the last
 * two:
 * - bool toJsonText(String& json) const;   (false when the text could not be
 *   made: nothing is written then)
 * - const char* fromJsonText(const String& json);   (nullptr when read, else
 *   ArduinoJson's name for why not; loadFromFile logs it)
 *
 * Note for implementers: read string values as `const char*` (e.g.
 * `obj["name"] | ""`), never as `| std::string("")` — ArduinoJson's
 * std::string converter drags a per-TU copy of the whole JSON serializer
 * into flash via its serializeJson fallback.
 *
 * Concurrency: saveToFile/loadFromFile lock storeMutex, so toJson/fromJson (or
 * toJsonText/fromJsonText) always run under it. fromJson/fromJsonText must
 * signal legacy-shape upgrades with requestResave(), never by calling
 * saveToFile() directly (deadlock).
 */
template <typename T>
class PersistableStore : public PersistableStoreBase {
 protected:
  PersistableStore() = default;
  ~PersistableStore() = default;

 public:
  // Delete copy constructor and assignment
  PersistableStore(const PersistableStore&) = delete;
  PersistableStore& operator=(const PersistableStore&) = delete;

  static T& getInstance() {
    static T instance;
    return instance;
  }

  bool saveToFile() const {
    // Here, not in the class body, where T (which derives from this) is not complete yet.
    static_assert(StoresJsonText<T>::value != StoresJsonDoc<T>::value,
                  "a store needs public toJson/fromJson or toJsonText/fromJsonText (exactly one pair)");
    std::lock_guard<std::mutex> lock(storeMutex);
    if constexpr (StoresJsonText<T>::value) {
      String json;
      if (!static_cast<const T*>(this)->toJsonText(json)) {
        LOG_ERR("PERSIST", "Failed to make %s", T::getFilePath());
        return false;
      }
      return writeTextToFile(T::getFilePath(), json);
    } else {
      JsonDocument doc;
      static_cast<const T*>(this)->toJson(doc);
      return writeDocToFile(T::getFilePath(), doc);
    }
  }

  bool loadFromFile() {
    // Here, not in the class body, where T (which derives from this) is not complete yet.
    static_assert(StoresJsonText<T>::value != StoresJsonDoc<T>::value,
                  "a store needs public toJson/fromJson or toJsonText/fromJsonText (exactly one pair)");
    bool ok;
    bool doResave;
    {
      std::lock_guard<std::mutex> lock(storeMutex);
      resaveRequested = false;
      if constexpr (StoresJsonText<T>::value) {
        String json;
        if (!readTextFromFile(T::getFilePath(), json)) {
          return false;
        }
        const char* error = static_cast<T*>(this)->fromJsonText(json);
        if (error) {
          LOG_ERR("PERSIST", "JSON parse error in %s: %s", T::getFilePath(), error);
          return false;
        }
        ok = true;
      } else {
        JsonDocument doc;
        if (!readDocFromFile(T::getFilePath(), doc)) {
          return false;
        }
        ok = static_cast<T*>(this)->fromJson(doc.as<JsonVariantConst>());
      }
      // Read the flag under the lock that guards the fromJson()/fromJsonText() that set it.
      doResave = resaveRequested;
      resaveRequested = false;
    }
    // Deliberately outside the lock: saveToFile() takes storeMutex itself.
    if (ok && doResave && !saveToFile()) {
      LOG_ERR("PERSIST", "Failed to resave %s after format update", T::getFilePath());
    }
    return ok;
  }
};
