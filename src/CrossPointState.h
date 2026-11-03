#pragma once
#include <PersistableStore.h>

#include <cstdint>
#include <string>

class CrossPointState : public PersistableStore<CrossPointState> {
  CrossPointState() = default;

  friend class PersistableStore<CrossPointState>;

 public:
  static constexpr uint8_t SLEEP_RECENT_COUNT = 16;

  std::string openEpubPath;
  uint16_t recentSleepImages[SLEEP_RECENT_COUNT] = {};
  uint8_t recentSleepPos = 0;
  uint8_t recentSleepFill = 0;
  uint16_t recentOverlaySleepImages[SLEEP_RECENT_COUNT] = {};
  uint8_t recentOverlaySleepPos = 0;
  uint8_t recentOverlaySleepFill = 0;
  uint8_t readerActivityLoadCount = 0;
  bool lastSleepFromReader = false;
  bool showBootScreen = true;

  static const char* getFilePath() { return "/.crosspoint/state.json"; }
  // state.json's text, read and written by Rust (bembeyaz_store::state). See PersistableStore.
  bool toJsonText(String& json) const;
  const char* fromJsonText(const String& json);

  bool isRecentSleep(uint16_t idx, uint8_t checkCount) const;
  bool isRecentOverlaySleep(uint16_t idx, uint8_t checkCount) const;

  void pushRecentSleep(uint16_t idx);
  void pushRecentOverlaySleep(uint16_t idx);
};

#define APP_STATE CrossPointState::getInstance()
