// purestwhite-gt911 (through lib/hal/WireBus.h) against freeink-sdk's InputManager GT911 path,
// which it replaces in the bare-metal firmware. Each side drives its own model controller; both
// see the same frames and the same bus failures, and must make the same pin writes, pauses and
// transfers and find the same address, contacts and Home key.
#include <BoardConfig.h>
#include <gtest/gtest.h>
#include <purestwhite/gt911.h>

#include <array>
#include <cstdint>
#include <cstring>
#include <random>
#include <string>
#include <vector>

#include "WireBus.h"

#define private public
#include <InputManager.h>
#undef private

// The layout gt911.rs asserts on its side.
static_assert(sizeof(pw_gt911_config) == 14 && alignof(pw_gt911_config) == 2, "pw_gt911_config layout");
static_assert(sizeof(pw_gt911_contact) == 6 && sizeof(pw_gt911_frame) == 28, "pw_gt911_frame layout");
static_assert(sizeof(pw_gt911_pins) == 4 * sizeof(void*), "pw_gt911_pins layout");

namespace {

// What the controller and its pins saw, in order. Switches to output are not logged: the SDK
// makes both pins outputs at the start of each reset, ours as it drives them.
struct Event {
  enum Kind { Write, Input, Pause, Transmit, Request } kind;
  int a, b;
  std::vector<uint8_t> bytes;
  bool operator==(const Event&) const = default;
};

std::string describe(const Event& e) {
  static const char* names[] = {"write", "input", "pause", "transmit", "request"};
  std::string s = std::string(names[e.kind]) + " " + std::to_string(e.a) + " " + std::to_string(e.b);
  for (const uint8_t b : e.bytes) s += " " + std::to_string(b);
  return s;
}

// A GT911. The level INT drives as RST rises selects its address (low 0x5D, high 0x14); without
// a reset, or with INT not an output then, it answers at `addr`. Transfers fail at random; all
// randomness is the controller's own, so two controllers driven the same way stay the same.
struct Gt911 {
  std::mt19937 rng;
  int failPercent = 0;
  int nackProbes = 0;            // address-only probes refused first (still starting)
  int shortReads = 0;            // the next reads of more than a byte stop a byte early
  int shortened = 0;             // reads it stopped early
  bool statusDelivered = false;  // a status read gave its byte since the last clear of this flag
  uint8_t addr = 0x5D;
  bool strapped = false;  // ignores INT: keeps addr through resets
  int intLevel = LOW, rstLevel = HIGH;
  bool intOutput = false;
  uint16_t pointer = 0;
  uint8_t status = 0;
  std::array<uint8_t, 15 * 8> points{};
  std::vector<Event> events;

  explicit Gt911(uint32_t seed) : rng(seed) {}
  bool fails() { return failPercent > 0 && static_cast<int>(rng() % 100) < failPercent; }
};

Gt911* active = nullptr;

constexpr int RST = 4, INT = 10;

void hook() {
  onDigitalWrite = [](int pin, int level) {
    active->events.push_back({Event::Write, pin, level, {}});
    if (pin == INT) active->intLevel = level;
    if (pin == RST) {
      if (active->rstLevel == LOW && level == HIGH && !active->strapped && active->intOutput) {
        active->addr = active->intLevel == HIGH ? 0x14 : 0x5D;
      }
      active->rstLevel = level;
    }
  };
  onPinMode = [](int pin, int mode) {
    if (pin == INT) active->intOutput = mode == OUTPUT;
    if (mode == INPUT) active->events.push_back({Event::Input, pin, 0, {}});
  };
  onDelay = [](unsigned long ms) { active->events.push_back({Event::Pause, static_cast<int>(ms), 0, {}}); };
  onTransmit = [](uint8_t addr, const std::vector<uint8_t>& bytes, bool stop) {
    Gt911& c = *active;
    c.events.push_back({Event::Transmit, addr, stop, bytes});
    if (addr != c.addr || c.fails()) return 2;
    if (bytes.empty()) {
      if (c.nackProbes > 0) {
        --c.nackProbes;
        return 2;
      }
      return 0;
    }
    if (bytes.size() >= 2) c.pointer = static_cast<uint16_t>(bytes[0] << 8 | bytes[1]);
    if (bytes.size() == 3 && c.pointer == 0x814E) c.status = bytes[2];
    return 0;
  };
  onRequest = [](uint8_t addr, uint8_t len, std::vector<uint8_t>& out) -> uint8_t {
    Gt911& c = *active;
    c.events.push_back({Event::Request, addr, len, {}});
    if (addr != c.addr || c.fails()) return 0;
    uint8_t n = len;
    if (c.shortReads > 0 && n > 1) {
      --c.shortReads;
      --n;
      ++c.shortened;
    }
    if (c.pointer == 0x814E && n > 0) c.statusDelivered = true;
    for (uint8_t i = 0; i < n; ++i) {
      const unsigned at = c.pointer + i;
      if (at == 0x814E) {
        out.push_back(c.status);
      } else if (at >= 0x8150 && at < 0x8150 + c.points.size()) {
        out.push_back(c.points[at - 0x8150]);
      } else {
        out.push_back(0);
      }
    }
    return n;
  };
}

pw_gt911_config ourConfig() {
  const auto& t = BoardConfig::ACTIVE.touch;
  return {t.i2cAddress, t.i2cAddressAlt, t.rawMinX, t.rawMaxX, t.rawMinY,
          t.rawMaxY,    t.swapXY,        t.flipX,   t.flipY,   t.gt911CoordsAtByte0};
}

pw_gt911_pins ourPins() {
  pw_gt911_pins pins{};
  pins.set_reset = [](void*, bool high) {
    pinMode(BoardConfig::ACTIVE.touch.reset, OUTPUT);
    digitalWrite(BoardConfig::ACTIVE.touch.reset, high ? HIGH : LOW);
  };
  // The level first, then the drive, as gt911.h asks.
  pins.drive_int = [](void*, bool high) {
    digitalWrite(BoardConfig::ACTIVE.touch.irq, high ? HIGH : LOW);
    pinMode(BoardConfig::ACTIVE.touch.irq, OUTPUT);
  };
  pins.release_int = [](void*) { pinMode(BoardConfig::ACTIVE.touch.irq, INPUT); };
  return pins;
}

// What InputManager::beginGt911 does before the reset: the touch supply and the bus, the
// caller's part with purestwhite-gt911.
void powerAndBus() {
  const auto& t = BoardConfig::ACTIVE.touch;
  if (t.powerEnable >= 0) {
    pinMode(t.powerEnable, OUTPUT);
    digitalWrite(t.powerEnable, t.powerEnableActiveHigh ? HIGH : LOW);
    delay(50);
  }
  if (t.sda >= 0 && t.scl >= 0) {
    Wire.begin(t.sda, t.scl, 400000);
    Wire.setTimeOut(10);
  }
}

uint8_t ourBegin() {
  powerAndBus();
  const auto& t = BoardConfig::ACTIVE.touch;
  const pw_i2c bus = wireBus(Wire);
  const pw_gt911_config config = ourConfig();
  const pw_gt911_pins pins = ourPins();
  return pw_gt911_begin(&bus, t.reset >= 0 && t.irq >= 0 ? &pins : nullptr, &config);
}

class Gt911TouchTest : public testing::Test {
 protected:
  void SetUp() override {
    saved = BoardConfig::ACTIVE.touch;
    hook();
  }
  void TearDown() override { BoardConfig::ACTIVE.touch = saved; }
  BoardConfig::TouchConfig saved{};
};

TEST_F(Gt911TouchTest, BeginMakesTheSameResetsProbesAndFindings) {
  int found[3] = {};
  // Pins: both wired, RST missing, INT missing. Addresses: both, no alternate, no primary.
  for (int wiring = 0; wiring < 3; ++wiring) {
    for (int addresses = 0; addresses < 3; ++addresses) {
      for (int nack = 0; nack <= 5; ++nack) {
        for (const uint8_t at : {uint8_t{0x5D}, uint8_t{0x14}, uint8_t{0x33}}) {
          for (const bool strapped : {false, true}) {
            auto& t = BoardConfig::ACTIVE.touch;
            t = saved;
            if (wiring == 1) t.reset = -1;
            if (wiring == 2) t.irq = -1;
            if (addresses == 1) t.i2cAddressAlt = 0;
            if (addresses == 2) t.i2cAddress = 0;
            Gt911 sdkChip(1), ourChip(1);
            for (Gt911* c : {&sdkChip, &ourChip}) {
              c->nackProbes = nack;
              c->addr = at;
              c->strapped = strapped;
            }
            InputManager sdk;
            active = &sdkChip;
            sdk.beginGt911();
            active = &ourChip;
            const uint8_t ours = ourBegin();
            const std::string where = "wiring " + std::to_string(wiring) + " addresses " + std::to_string(addresses) +
                                      " nack " + std::to_string(nack) + " at " + std::to_string(at) + " strapped " +
                                      std::to_string(strapped);
            ASSERT_EQ(ours, sdk.gt911Addr) << where;
            ASSERT_EQ(sdkChip.events.size(), ourChip.events.size()) << where;
            for (size_t i = 0; i < sdkChip.events.size(); ++i) {
              ASSERT_EQ(sdkChip.events[i], ourChip.events[i])
                  << where << " event " << i << ": " << describe(sdkChip.events[i]) << " / "
                  << describe(ourChip.events[i]);
            }
            ASSERT_EQ(sdkChip.addr, ourChip.addr) << where;
            ++found[ours == 0x5D ? 0 : ours == 0x14 ? 1 : 2];
          }
        }
      }
    }
  }
  // Every outcome happened.
  EXPECT_GT(found[0], 0);
  EXPECT_GT(found[1], 0);
  EXPECT_GT(found[2], 0);
}

// One long session per board layout: frames come and go, the bus fails at random, and every
// poll must make the same transfers and give the same contacts and Home key.
void session(uint32_t seed) {
  std::mt19937 rng(seed);
  Gt911 sdkChip(seed), ourChip(seed);
  InputManager sdk;
  sdk.gt911Addr = 0x5D;
  const pw_gt911_config config = ourConfig();
  const auto& t = BoardConfig::ACTIVE.touch;
  int kinds[4] = {};
  int atRangeStart = 0;  // coordinates equal to rawMinX or rawMinY, where map_axis turns
  unsigned long now = 5000;
  for (int poll = 0; poll < 30000; ++poll) {
    now += 10 + rng() % 30;
    hostMillis = now;
    // Sometimes a new frame: 0 to 6 contacts (some beyond what is kept), the Home key, and
    // coordinates inside and outside the raw range.
    if (rng() % 3 == 0) {
      const uint8_t count = rng() % 4 == 0 ? 0 : rng() % 7;
      // Mostly what the controller sends; sometimes every other bit as well.
      const uint8_t status = rng() % 4 == 0 ? 0x80 | (rng() & 0x7F) : 0x80 | (rng() % 3 == 0 ? 0x10 : 0) | count;
      std::array<uint8_t, 15 * 8> points{};
      for (auto& b : points) b = static_cast<uint8_t>(rng());
      for (int i = 0; i < 15; ++i) {
        const uint16_t x = rng() % 1300, y = rng() % 1300;
        atRangeStart += (x == t.rawMinX || x == t.rawMinY) + (y == t.rawMinX || y == t.rawMinY);
        const int at = i * 8 + (rng() % 2);
        points[at] = x & 0xFF;
        points[at + 1] = x >> 8;
        points[at + 2] = y & 0xFF;
        points[at + 3] = y >> 8;
      }
      for (Gt911* c : {&sdkChip, &ourChip}) {
        c->status = status;
        c->points = points;
      }
    } else if (rng() % 50 == 0) {
      // A status without a new frame, which must not be cleared.
      const uint8_t status = rng() & 0x7F;
      for (Gt911* c : {&sdkChip, &ourChip}) c->status = status;
    }
    const int failPercent = rng() % 10 == 0 ? 40 : 0;
    const int shortReads = rng() % 40 == 0 ? 1 : 0;
    for (Gt911* c : {&sdkChip, &ourChip}) {
      c->failPercent = failPercent;
      c->shortReads = shortReads;
      c->statusDelivered = false;
      c->events.clear();
    }

    const InputManager::TouchSnapshot before = sdk.touchSnapshot;
    const bool homeBefore = sdk.touchHomeKeyDown;
    active = &sdkChip;
    sdk.pollGt911(now);
    active = &ourChip;
    const pw_i2c bus = wireBus(Wire);
    // A sentinel shows what the call wrote; home_key stays a valid bool.
    pw_gt911_frame frame;
    std::memset(&frame, 0xA5, sizeof frame);
    frame.home_key = false;
    pw_gt911_frame sentinel;
    std::memcpy(&sentinel, &frame, sizeof sentinel);
    const uint8_t kind = pw_gt911_read(&bus, 0x5D, &config, &frame);
    ASSERT_LT(kind, 4) << "poll " << poll;
    ++kinds[kind];
    // Only a failed status read is FAILED: the SDK times the Home key's long press otherwise.
    ASSERT_EQ(kind == PW_GT911_FAILED, !ourChip.statusDelivered) << "poll " << poll;
    ASSERT_EQ(sdkChip.statusDelivered, ourChip.statusDelivered) << "poll " << poll;

    ASSERT_EQ(sdkChip.events.size(), ourChip.events.size()) << "poll " << poll;
    for (size_t i = 0; i < sdkChip.events.size(); ++i) {
      ASSERT_EQ(sdkChip.events[i], ourChip.events[i])
          << "poll " << poll << " event " << i << ": " << describe(sdkChip.events[i]) << " / "
          << describe(ourChip.events[i]);
    }
    ASSERT_EQ(sdkChip.status, ourChip.status) << "poll " << poll;

    const auto& s = sdk.touchSnapshot;
    const auto sameSnapshot = [&](const InputManager::TouchSnapshot& a, const InputManager::TouchSnapshot& b) {
      if (a.count != b.count || a.reportedCount != b.reportedCount || a.idsStable != b.idsStable) return false;
      for (int i = 0; i < a.count; ++i) {
        if (a.points[i].id != b.points[i].id || a.points[i].point.x != b.points[i].point.x ||
            a.points[i].point.y != b.points[i].point.y)
          return false;
      }
      return true;
    };
    switch (kind) {
      case PW_GT911_FAILED:
      case PW_GT911_NOT_READY:
        ASSERT_TRUE(sameSnapshot(s, before)) << "poll " << poll;
        ASSERT_EQ(sdk.touchHomeKeyDown, homeBefore) << "poll " << poll;
        ASSERT_EQ(std::memcmp(&frame, &sentinel, sizeof frame), 0) << "poll " << poll;
        break;
      case PW_GT911_CONTACTS_FAILED: {
        ASSERT_TRUE(sameSnapshot(s, before)) << "poll " << poll;
        ASSERT_EQ(sdk.touchHomeKeyDown, frame.home_key) << "poll " << poll;
        // Everything after home_key, padding included, as it was.
        ASSERT_EQ(std::memcmp(reinterpret_cast<const char*>(&frame) + 1, reinterpret_cast<const char*>(&sentinel) + 1,
                              sizeof frame - 1),
                  0)
            << "poll " << poll;
        break;
      }
      case PW_GT911_FRAME:
        ASSERT_EQ(sdk.touchHomeKeyDown, frame.home_key) << "poll " << poll;
        ASSERT_EQ(s.count, frame.count) << "poll " << poll;
        ASSERT_EQ(s.reportedCount, frame.reported) << "poll " << poll;
        ASSERT_EQ(s.idsStable, !t.gt911CoordsAtByte0) << "poll " << poll;
        for (int i = 0; i < frame.count; ++i) {
          ASSERT_EQ(s.points[i].id, frame.contacts[i].id) << "poll " << poll << " contact " << i;
          ASSERT_EQ(s.points[i].point.x, frame.contacts[i].x) << "poll " << poll << " contact " << i;
          ASSERT_EQ(s.points[i].point.y, frame.contacts[i].y) << "poll " << poll << " contact " << i;
        }
        break;
      default:
        FAIL() << "kind " << int(kind);
    }
  }
  for (int k = 0; k < 4; ++k) EXPECT_GT(kinds[k], 100) << "kind " << k;
  // The cases that tell a cut read and the range edges apart happened.
  EXPECT_GT(ourChip.shortened, 50);
  EXPECT_GT(atRangeStart, 20);
}

TEST_F(Gt911TouchTest, PollsMatchOnTheX4Pro) { session(7); }

TEST_F(Gt911TouchTest, PollsMatchWithIdsFlipsAndOffsetRanges) {
  auto& t = BoardConfig::ACTIVE.touch;
  t.gt911CoordsAtByte0 = false;
  t.swapXY = false;
  t.flipX = true;
  t.flipY = true;
  t.rawMinX = 30;
  t.rawMaxX = 1030;
  t.rawMinY = 200;
  t.rawMaxY = 900;
  session(11);
}

// Ranges the SDK's 16-bit arithmetic wraps: x upside down, y empty.
TEST_F(Gt911TouchTest, PollsMatchWithDegenerateRanges) {
  auto& t = BoardConfig::ACTIVE.touch;
  t.rawMinX = 900;
  t.rawMaxX = 100;
  t.rawMinY = 400;
  t.rawMaxY = 400;
  t.flipX = true;
  session(13);
}

}  // namespace
