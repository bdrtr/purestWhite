// purestwhite-gauge against the freeink-sdk CW2017 path it replaced in HalPowerManager, on the
// SDK's recording Wire stub. The SDK keeps the gauge's state in function statics, so the two run
// as the device does, in one long session: each drives its own model chip, both chips see the
// same clock, bus failures, restarts and disturbances, and every reading must give the same
// result after the same transfers and pauses.
#include <BatteryMonitor.h>
#include <BoardConfig.h>
#include <gtest/gtest.h>
#include <purestwhite/gauge.h>

#include <algorithm>
#include <array>
#include <cstdint>
#include <random>
#include <vector>

#include "WireBus.h"

namespace {

constexpr uint8_t ADDR = 0x63;
constexpr uint8_t VERSION = 0x00, SOC = 0x04, MODE = 0x08, SOC_ALERT = 0x0B, BATINFO = 0x10;

struct Write {
  uint8_t reg, value;
  bool operator==(const Write&) const = default;
};

// A CW2017: after the soft reset (MODE 0x00 after 0x30) VERSION reads 0xA0 for a while and SOC
// is not yet valid. Transfers fail at random. All randomness is the chip's own, so two chips
// driven the same way stay the same.
struct Chip {
  std::array<uint8_t, 256> regs{};
  std::mt19937 rng;
  int failPercent = 0;
  int starting = 0;
  int socInvalid = 0;
  uint8_t lastMode = 0;
  std::vector<Write> writes;
  int operations = 0;
  unsigned long clock = 0;

  explicit Chip(uint32_t seed) : rng(seed) {}
  bool fails() {
    ++operations;
    return failPercent > 0 && static_cast<int>(rng() % 100) < failPercent;
  }
};

Chip* active = nullptr;

// What one reading did.
struct Outcome {
  bool ok;
  uint16_t percent;
  std::vector<Write> writes;
  int operations;
  unsigned long pausedMs;
};

Outcome on(Chip& chip, const unsigned long now, bool (*read)(uint16_t&)) {
  active = &chip;
  chip.writes.clear();
  chip.operations = 0;
  hostMillis = now;
  uint16_t percent = 0;
  const bool ok = read(percent);
  return {ok, ok ? percent : uint16_t{0}, chip.writes, chip.operations, hostMillis - now};
}

bool sdkRead(uint16_t& percent) {
  static const BatteryMonitor battery;
  return battery.readPercentageChecked(percent);
}

pw_cw2017 gauge = {};
bool ourRead(uint16_t& percent) {
  const auto& g = BoardConfig::ACTIVE.batteryGauge;
  const pw_i2c bus = wireBus(startedWire(g.i2cBus, g.i2cSda, g.i2cScl, g.i2cHz));
  uint8_t soc = 0;
  if (!pw_cw2017_soc(&gauge, &bus, g.gaugeAddr, static_cast<uint32_t>(millis()), &soc)) return false;
  percent = soc;
  return true;
}

// The profile the SDK and purestwhite-gauge upload, read from a chip they wrote it to.
std::array<uint8_t, 80> profileOf(const Chip& chip) {
  std::array<uint8_t, 80> p{};
  std::copy_n(chip.regs.begin() + BATINFO, 80, p.begin());
  return p;
}

}  // namespace

bool hostI2cWrite(const uint8_t addr, const std::vector<uint8_t>& bytes) {
  Chip& c = *active;
  if (addr != ADDR || c.fails()) return false;
  for (size_t i = 1; i < bytes.size(); ++i) {
    const auto reg = static_cast<uint8_t>(bytes[0] + i - 1);
    c.writes.push_back({reg, bytes[i]});
    c.regs[reg] = bytes[i];
    if (reg == MODE) {
      if (bytes[i] == 0x00 && c.lastMode == 0x30) {
        c.starting = static_cast<int>(c.rng() % 60);
        c.socInvalid = static_cast<int>(c.rng() % 40);
      }
      c.lastMode = bytes[i];
    }
  }
  return true;
}

bool hostI2cRead(const uint8_t addr, const uint8_t reg, uint8_t* out, const size_t length) {
  Chip& c = *active;
  if (addr != ADDR || length != 1 || c.fails()) return false;
  if (reg == VERSION && c.starting > 0) {
    --c.starting;
    *out = 0xA0;
  } else if (reg == SOC && c.socInvalid > 0) {
    --c.socInvalid;
    *out = 0xFF;
  } else {
    *out = c.regs[reg];
  }
  return true;
}

TEST(Cw2017Gauge, ReadsAsTheSdkThroughALongSession) {
  BoardConfig::ACTIVE = BoardConfig::XTEINK_X4_PRO;
  ASSERT_EQ(BoardConfig::ACTIVE.batteryGauge.gaugeType, BoardConfig::GaugeType::Cw2017);
  ASSERT_EQ(BoardConfig::ACTIVE.batteryGauge.gaugeAddr, ADDR);

  Chip sdk(7), ours(7);
  // A gauge out of the box: asleep, no profile.
  for (Chip* c : {&sdk, &ours}) {
    c->regs[MODE] = 0xF0;
    c->regs[VERSION] = 0xA0;
    c->regs[SOC] = 55;
  }

  std::mt19937 rng(20261025);
  unsigned long now = 0;
  int readings = 0, successes = 0, uploads = 0;
  for (int step = 0; step < 30000; ++step) {
    // The same disturbance on both chips.
    const int fail = step % 500 < 50 ? 30 : (step % 7 == 0 ? 5 : 0);
    sdk.failPercent = ours.failPercent = fail;
    switch (rng() % 40) {
      case 0: {  // power glitch: asleep or restarting
        const uint8_t mode = (rng() & 1) ? 0xF0 : 0x30;
        sdk.regs[MODE] = ours.regs[MODE] = mode;
        sdk.lastMode = ours.lastMode = mode;
        break;
      }
      case 1: {  // the profile flag or a profile byte lost
        const uint8_t reg = (rng() & 1) ? SOC_ALERT : static_cast<uint8_t>(BATINFO + rng() % 80);
        sdk.regs[reg] = ours.regs[reg] = static_cast<uint8_t>(rng());
        break;
      }
      case 2: {  // VERSION drifts
        const uint8_t v = std::array<uint8_t, 4>{0x0D, 0x0F, 0xA0, 0x55}[rng() % 4];
        sdk.regs[VERSION] = ours.regs[VERSION] = v;
        break;
      }
      case 3: {  // a bad state of charge
        const auto soc = static_cast<uint8_t>(rng() % 130);
        sdk.regs[SOC] = ours.regs[SOC] = soc;
        break;
      }
      case 4:
      case 5: {  // charged: full, or just past it
        const uint8_t soc = (rng() & 1) ? 100 : 101;
        sdk.regs[SOC] = ours.regs[SOC] = soc;
        break;
      }
      default: {  // the battery drains a little
        const auto soc =
            static_cast<uint8_t>(sdk.regs[SOC] > 0 && sdk.regs[SOC] <= 100 ? sdk.regs[SOC] - (rng() % 2) : 80);
        sdk.regs[SOC] = ours.regs[SOC] = soc;
        break;
      }
    }
    now += 1 + rng() % 2500;

    const Outcome a = on(sdk, now, sdkRead);
    const Outcome b = on(ours, now, ourRead);
    SCOPED_TRACE(testing::Message() << "step " << step << " at " << now << " ms");
    ASSERT_EQ(a.ok, b.ok);
    ASSERT_EQ(a.percent, b.percent);
    ASSERT_TRUE(a.writes == b.writes);
    ASSERT_EQ(a.operations, b.operations);
    ASSERT_EQ(a.pausedMs, b.pausedMs);
    ASSERT_TRUE(sdk.regs == ours.regs);
    // The pauses stood for elapsed time on the device.
    now += a.pausedMs;
    ++readings;
    successes += a.ok;
    uploads += a.writes.size() >= 80;
  }
  // The session went through every path.
  EXPECT_GT(successes, readings / 4);
  EXPECT_LT(successes, readings);
  EXPECT_GT(uploads, 10);
  EXPECT_EQ(profileOf(ours), profileOf(sdk));
}

TEST(Cw2017Gauge, NullArgumentsFail) {
  const pw_i2c bus = wireBus(Wire);
  pw_cw2017 g = {};
  uint8_t soc = 0;
  EXPECT_FALSE(pw_cw2017_soc(nullptr, &bus, ADDR, 0, &soc));
  EXPECT_FALSE(pw_cw2017_soc(&g, nullptr, ADDR, 0, &soc));
  EXPECT_FALSE(pw_cw2017_soc(&g, &bus, ADDR, 0, nullptr));
}
