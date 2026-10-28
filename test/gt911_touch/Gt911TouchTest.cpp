// purestwhite-gt911 (through lib/hal/WireBus.h) against freeink-sdk's InputManager GT911 path,
// which it replaces in the bare-metal firmware. Each side drives its own model controller; both
// see the same frames and the same bus failures, and must make the same pin writes, pauses and
// transfers and find the same address, contacts and Home key.
#include <BoardConfig.h>
#include <gtest/gtest.h>
#include <purestwhite/gt911.h>
#include <purestwhite/touch.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <map>
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
// And touch.rs's.
static_assert(sizeof(pw_touch_point) == 4 && sizeof(pw_touch_events) == 30 && offsetof(pw_touch_events, activity) == 28,
              "pw_touch_events layout");

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

// purestwhite-touch against InputManager's touch classifiers: one model controller each, both fed
// the same fingers. Fingers touch down, hold, creep or flick and lift; a second or third finger
// joins now and then, sometimes with more contacts reported than the controller keeps; the Home
// key is tapped and held; polls find no new frame or fail; a caller suppresses a contact after a
// long press or at random, after reading the poll's events. After every poll each side's queries
// must give the same events, at the same normalized points.
void gestures(uint32_t seed) {
  std::mt19937 rng(seed);
  Gt911 sdkChip(seed), ourChip(seed);
  InputManager sdk;
  sdk.gt911Addr = 0x5D;
  sdk.touchDataEnabled = true;
  pw_touch* ours = pw_touch_new();
  const pw_gt911_config config = ourConfig();
  const auto& t = BoardConfig::ACTIVE.touch;
  const int offset = t.gt911CoordsAtByte0 ? 0 : 1;

  // Each finger in raw controller coordinates, inside the raw ranges and a little beyond.
  struct Finger {
    int a, b, va, vb;
  };
  std::array<Finger, 3> fingers{};
  int down = 0;   // fingers on the panel
  int phase = 0;  // polls until the fingers' motion changes
  bool home = false;
  int homeLeft = 0;  // polls until the Home key changes
  bool changed = false;
  std::map<std::string, int> seen;
  unsigned long now = 5000;
  const auto place = [&](Finger& f) {
    f.a = static_cast<int>(rng() % 520) - 20;
    f.b = static_cast<int>(rng() % 840) - 20;
  };
  const auto speed = [&](Finger& f) {
    switch (rng() % 4) {
      case 0:
      case 1:  // still
        f.va = f.vb = 0;
        break;
      case 2:  // creeping
        f.va = static_cast<int>(rng() % 5) - 2;
        f.vb = static_cast<int>(rng() % 5) - 2;
        break;
      default:  // flicking
        f.va = static_cast<int>(rng() % 41) - 20;
        f.vb = static_cast<int>(rng() % 41) - 20;
    }
  };
  for (int poll = 0; poll < 200000; ++poll) {
    now += 4 + rng() % 30;
    hostMillis = now;

    if (--phase <= 0) {
      phase = 1 + static_cast<int>(rng() % 60);
      const int before = down;
      if (down == 0) {
        down = rng() % 3 == 0 ? 0 : (rng() % 8 == 0 ? 2 : 1);
      } else {
        const int r = rng() % 6;
        down = r < 3 ? 0 : (r == 3 ? std::min(down + 1, 3) : (r == 4 ? std::max(down - 1, 0) : down));
      }
      for (int i = before; i < down; ++i) place(fingers[i]);
      for (int i = 0; i < down; ++i) speed(fingers[i]);
      changed |= down != before;
    }
    for (int i = 0; i < down; ++i) {
      if (fingers[i].va || fingers[i].vb) {
        fingers[i].a += fingers[i].va;
        fingers[i].b += fingers[i].vb;
        changed = true;
      }
    }
    if (--homeLeft <= 0) {
      home = !home && rng() % 3 == 0;
      // Short taps and holds past the long press.
      homeLeft = home ? 1 + static_cast<int>(rng() % 70) : 20 + static_cast<int>(rng() % 200);
      changed = true;
    }
    // The controller sets a new frame for a change, but not always by the next poll.
    if (changed && rng() % 5 != 0) {
      changed = false;
      std::array<uint8_t, 15 * 8> points{};
      for (int i = 0; i < down; ++i) {
        const uint16_t x = static_cast<uint16_t>(std::clamp(fingers[i].a, 0, 1200));
        const uint16_t y = static_cast<uint16_t>(std::clamp(fingers[i].b, 0, 1200));
        uint8_t* record = points.data() + i * 8;
        record[0] = static_cast<uint8_t>(i + 1);
        record[offset] = x & 0xFF;
        record[offset + 1] = x >> 8;
        record[offset + 2] = y & 0xFF;
        record[offset + 3] = y >> 8;
      }
      // Now and then more contacts reported than the controller keeps.
      const uint8_t count = down == 3 && rng() % 4 == 0 ? 6 : static_cast<uint8_t>(down);
      const uint8_t status = 0x80 | (home ? 0x10 : 0) | count;
      for (Gt911* c : {&sdkChip, &ourChip}) {
        c->status = status;
        c->points = points;
      }
    }
    const int failPercent = rng() % 20 == 0 ? 50 : 0;
    for (Gt911* c : {&sdkChip, &ourChip}) {
      c->failPercent = failPercent;
      c->events.clear();
    }

    active = &sdkChip;
    sdk.update();
    active = &ourChip;
    const pw_i2c bus = wireBus(Wire);
    pw_gt911_frame frame{};
    const uint8_t kind = pw_gt911_read(&bus, 0x5D, &config, &frame);
    pw_touch_events e{};
    pw_touch_update(ours, static_cast<uint32_t>(now), kind, &frame, &e);
    ASSERT_EQ(sdkChip.status, ourChip.status) << "poll " << poll;

    const std::string where = "poll " + std::to_string(poll);
    float nx = 0, ny = 0, mx = 0, my = 0, ex = 0, ey = 0, fx = 0, fy = 0;
    // A point of ours, normalized by the SDK.
    const auto norm = [&](const pw_touch_point& p, float& x, float& y) { sdk.normalizeTouchPoint(p.x, p.y, x, y); };
    const auto check = [&](const char* name, bool sdkHas, bool ourHas) {
      ASSERT_EQ(sdkHas, ourHas) << where << " " << name;
      if (ourHas) ++seen[name];
    };

    ASSERT_NO_FATAL_FAILURE(check("pressed", sdk.wasTouchPressed(), e.pressed));
    if (sdk.wasTouchPressedAt(nx, ny)) {
      ASSERT_TRUE(e.pressed) << where;
      norm(e.pressed_at, mx, my);
      ASSERT_EQ(nx, mx) << where;
      ASSERT_EQ(ny, my) << where;
    }
    ASSERT_NO_FATAL_FAILURE(check("released", sdk.wasTouchReleased(), e.released));
    ASSERT_NO_FATAL_FAILURE(check("tap", sdk.wasTouchTap(nx, ny), e.tap));
    if (e.tap) {
      norm(e.tap_at, mx, my);
      ASSERT_EQ(nx, mx) << where;
      ASSERT_EQ(ny, my) << where;
    }
    ASSERT_NO_FATAL_FAILURE(check("swipe", sdk.wasSwipe(nx, ny, ex, ey), e.swipe));
    if (e.swipe) {
      norm(e.swipe_start, mx, my);
      norm(e.swipe_end, fx, fy);
      ASSERT_EQ(nx, mx) << where;
      ASSERT_EQ(ny, my) << where;
      ASSERT_EQ(ex, fx) << where;
      ASSERT_EQ(ey, fy) << where;
    }
    ASSERT_NO_FATAL_FAILURE(check("long press", sdk.wasTouchLongPress(nx, ny), e.long_press));
    if (e.long_press) {
      norm(e.long_press_at, mx, my);
      ASSERT_EQ(nx, mx) << where;
      ASSERT_EQ(ny, my) << where;
    }
    ASSERT_NO_FATAL_FAILURE(check("home pressed", sdk.wasHomeKeyPressed(), e.home_pressed));
    ASSERT_NO_FATAL_FAILURE(check("home tapped", sdk.wasHomeKeyTapped(), e.home_tapped));
    ASSERT_NO_FATAL_FAILURE(check("home long press", sdk.wasHomeKeyLongPressed(), e.home_long_press));
    ASSERT_NO_FATAL_FAILURE(check("activity", sdk.wasTouchActivity(), e.activity));
    ASSERT_NO_FATAL_FAILURE(check("down", sdk.isTouchPressed(), pw_touch_is_pressed(ours)));
    unsigned long sdkHeld = 0;
    uint32_t ourHeld = 0;
    pw_touch_point at{};
    ASSERT_NO_FATAL_FAILURE(check("tap candidate", sdk.isTouchTapCandidate(nx, ny, sdkHeld),
                                  pw_touch_tap_candidate(ours, static_cast<uint32_t>(now), &at, &ourHeld)));
    if (sdk.isTouchTapCandidate(nx, ny, sdkHeld)) {
      norm(at, mx, my);
      ASSERT_EQ(nx, mx) << where;
      ASSERT_EQ(ny, my) << where;
      ASSERT_EQ(sdkHeld, ourHeld) << where;
    }
    ASSERT_NO_FATAL_FAILURE(check("held", sdk.isTouchHeldAt(nx, ny), pw_touch_held_at(ours, &at)));
    if (sdk.isTouchHeldAt(nx, ny)) {
      norm(at, mx, my);
      ASSERT_EQ(nx, mx) << where;
      ASSERT_EQ(ny, my) << where;
    }
    ASSERT_EQ(sdk.lastTouchHeldMs(), pw_touch_last_held_ms(ours)) << where;
    if (sdk.touchMultiContactSequence) ++seen["multi"];

    // A caller acting on what it saw, now and then: after a long press as an app would, and at
    // random.
    if ((e.long_press && rng() % 2 == 0) || rng() % 300 == 0) {
      sdk.suppressTouchContact();
      pw_touch_suppress(ours);
      ++seen["suppress"];
    }
  }
  pw_touch_free(ours);
  for (const char* name : {"pressed", "released", "tap", "swipe", "long press", "home pressed", "home tapped",
                           "home long press", "activity", "down", "tap candidate", "held", "multi", "suppress"}) {
    EXPECT_GT(seen[name], 50) << name;
  }
}

TEST_F(Gt911TouchTest, GesturesMatchOnTheX4Pro) { gestures(17); }

TEST_F(Gt911TouchTest, GesturesMatchWithIdsFlipsAndOffsetRanges) {
  auto& t = BoardConfig::ACTIVE.touch;
  t.gt911CoordsAtByte0 = false;
  t.swapXY = false;
  t.flipX = true;
  t.rawMinX = 30;
  t.rawMaxX = 1030;
  t.rawMinY = 200;
  t.rawMaxY = 900;
  gestures(19);
}

}  // namespace
