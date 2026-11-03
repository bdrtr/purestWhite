#include "CrossPointState.h"

#include <bembeyaz/store_state.h>

#include <cstddef>
#include <cstring>

// The JSON and the rings are Rust's (bembeyaz_store::state); this file passes the fields to Rust
// and takes them back.
namespace {
static_assert(CrossPointState::SLEEP_RECENT_COUNT == 16, "bb_store_state_ring holds 16 images");
static_assert(sizeof(bb_store_state_ring) == 34 && offsetof(bb_store_state_ring, pos) == 32,
              "bb_store_state_ring layout");
static_assert(sizeof(bb_store_state) == 72 && offsetof(bb_store_state, recent_overlay_sleep) == 34 &&
                  offsetof(bb_store_state, reader_activity_load_count) == 68 &&
                  offsetof(bb_store_state, show_boot_screen) == 70,
              "bb_store_state layout");

bb_store_state_ring ringOf(const uint16_t* images, const uint8_t pos, const uint8_t fill) {
  bb_store_state_ring ring{};
  std::memcpy(ring.images, images, sizeof(ring.images));
  ring.pos = pos;
  ring.fill = fill;
  return ring;
}

void takeRing(const bb_store_state_ring& ring, uint16_t* images, uint8_t& pos, uint8_t& fill) {
  std::memcpy(images, ring.images, sizeof(ring.images));
  pos = ring.pos;
  fill = ring.fill;
}

// The text Rust writes, appended to json; ok turns false when the String cannot grow.
struct Text {
  String& json;
  bool ok;
};

void appendTo(const char* data, const size_t len, void* context) {
  auto* text = static_cast<Text*>(context);
  text->ok = text->ok && text->json.concat(data, static_cast<unsigned int>(len));
}

void assignTo(const char* data, const size_t len, void* context) {
  static_cast<std::string*>(context)->assign(data, len);
}
}  // namespace

bool CrossPointState::isRecentSleep(uint16_t idx, uint8_t checkCount) const {
  const bb_store_state_ring ring = ringOf(recentSleepImages, recentSleepPos, recentSleepFill);
  return bb_store_state_is_recent(&ring, idx, checkCount);
}

bool CrossPointState::isRecentOverlaySleep(uint16_t idx, uint8_t checkCount) const {
  const bb_store_state_ring ring = ringOf(recentOverlaySleepImages, recentOverlaySleepPos, recentOverlaySleepFill);
  return bb_store_state_is_recent(&ring, idx, checkCount);
}

void CrossPointState::pushRecentSleep(uint16_t idx) {
  bb_store_state_ring ring = ringOf(recentSleepImages, recentSleepPos, recentSleepFill);
  bb_store_state_push_recent(&ring, idx);
  takeRing(ring, recentSleepImages, recentSleepPos, recentSleepFill);
}

void CrossPointState::pushRecentOverlaySleep(uint16_t idx) {
  bb_store_state_ring ring = ringOf(recentOverlaySleepImages, recentOverlaySleepPos, recentOverlaySleepFill);
  bb_store_state_push_recent(&ring, idx);
  takeRing(ring, recentOverlaySleepImages, recentOverlaySleepPos, recentOverlaySleepFill);
}

bool CrossPointState::toJsonText(String& json) const {
  bb_store_state state{};
  state.recent_sleep = ringOf(recentSleepImages, recentSleepPos, recentSleepFill);
  state.recent_overlay_sleep = ringOf(recentOverlaySleepImages, recentOverlaySleepPos, recentOverlaySleepFill);
  state.reader_activity_load_count = readerActivityLoadCount;
  state.last_sleep_from_reader = lastSleepFromReader;
  state.show_boot_screen = showBootScreen;
  Text text{json, true};
  bb_store_state_write(&state, openEpubPath.data(), openEpubPath.size(), appendTo, &text);
  return text.ok;
}

const char* CrossPointState::fromJsonText(const String& json) {
  bb_store_state state{};
  std::string path;
  const char* error = bb_store_state_read(json.c_str(), json.length(), &state, assignTo, &path);
  if (error) return error;
  openEpubPath = std::move(path);
  takeRing(state.recent_sleep, recentSleepImages, recentSleepPos, recentSleepFill);
  takeRing(state.recent_overlay_sleep, recentOverlaySleepImages, recentOverlaySleepPos, recentOverlaySleepFill);
  readerActivityLoadCount = state.reader_activity_load_count;
  lastSleepFromReader = state.last_sleep_from_reader;
  showBootScreen = state.show_boot_screen;
  return nullptr;
}
