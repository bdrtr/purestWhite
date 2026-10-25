// purestwhite-rtc against the freeink-sdk Rtc driver it replaced in HalClock, both on the SDK's
// recording Wire stub: the same registers give the same times, and the same calls make the same
// transfers, pauses and failures.
#include <BoardConfig.h>
#include <Rtc.h>
#include <gtest/gtest.h>
#include <purestwhite/rtc.h>

#include <array>
#include <cstdint>
#include <random>
#include <vector>

#include "WireBus.h"

namespace {

constexpr uint8_t ADDR = 0x51;

struct Write {
  uint8_t addr, reg, value;
  bool operator==(const Write&) const = default;
};

// The clock behind the stub, and what reached it.
struct Device {
  std::array<uint8_t, 256> regs{};
  std::vector<Write> writes;
  int operations = 0;
  int failAt = 0;  // the transfer that fails, from 1; 0 for none
  bool dead = false;
} device;

void resetTrace() {
  device.writes.clear();
  device.operations = 0;
  hostMillis = 0;
}

bool failing() { return device.dead || ++device.operations == device.failAt; }

// The time the old HalClock gave an RTC reading, with the firmware's 32-bit long.
int64_t oldEpochFromUtc(const Rtc::DateTime& dt) {
  int y = dt.year;
  const int m = dt.month;
  y -= m <= 2;
  const int era = (y >= 0 ? y : y - 399) / 400;
  const unsigned yoe = static_cast<unsigned>(y - era * 400);
  const unsigned doy = (153u * static_cast<unsigned>(m + (m > 2 ? -3 : 9)) + 2u) / 5u + dt.day - 1u;
  const unsigned doe = yoe * 365u + yoe / 4u - yoe / 100u + doy;
  const int32_t days = static_cast<int32_t>(static_cast<uint32_t>(era) * 146097u + doe - 719468u);
  return static_cast<int64_t>(days) * 86400 + dt.hour * 3600 + dt.minute * 60 + dt.second;
}

pw_rtc_time toPw(const Rtc::DateTime& dt) {
  return {dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second, dt.weekday};
}

struct Outcome {
  bool ok;
  Rtc::DateTime time;
  std::vector<Write> writes;
  int operations;
  unsigned long pausedMs;
};

Outcome traced(bool ok, const Rtc::DateTime& time = {}) {
  return {ok, time, device.writes, device.operations, hostMillis};
}

void expectSame(const Outcome& sdk, const Outcome& ours, const char* what, int i) {
  SCOPED_TRACE(testing::Message() << what << " case " << i);
  EXPECT_EQ(sdk.ok, ours.ok);
  EXPECT_EQ(sdk.time.year, ours.time.year);
  EXPECT_EQ(sdk.time.month, ours.time.month);
  EXPECT_EQ(sdk.time.day, ours.time.day);
  EXPECT_EQ(sdk.time.hour, ours.time.hour);
  EXPECT_EQ(sdk.time.minute, ours.time.minute);
  EXPECT_EQ(sdk.time.second, ours.time.second);
  EXPECT_EQ(sdk.time.weekday, ours.time.weekday);
  EXPECT_TRUE(sdk.writes == ours.writes);
  EXPECT_EQ(sdk.operations, ours.operations);
  EXPECT_EQ(sdk.pausedMs, ours.pausedMs);
}

// An SDK driver that has found the clock, with the trace cleared.
Rtc begunSdk() {
  const auto failAt = device.failAt;
  const auto dead = device.dead;
  device.failAt = 0;
  device.dead = false;
  Rtc rtc;
  EXPECT_TRUE(rtc.begin());
  device.failAt = failAt;
  device.dead = dead;
  return rtc;
}

class RtcPcf8563 : public testing::Test {
 protected:
  void SetUp() override {
    BoardConfig::ACTIVE.sensors.rtcType = BoardConfig::RtcType::Pcf8563;
    BoardConfig::ACTIVE.sensors.rtcAddr = ADDR;
    BoardConfig::ACTIVE.sensors.i2cSda = 39;
    BoardConfig::ACTIVE.sensors.i2cScl = 38;
    BoardConfig::ACTIVE.sensors.i2cHz = 400000;
    BoardConfig::ACTIVE.sensors.i2cBus = 0;
    device = {};
  }
  std::mt19937 rng{20261025};
  uint8_t byte() { return static_cast<uint8_t>(rng()); }
  pw_i2c bus = wireBus(Wire);
};

}  // namespace

bool hostI2cWrite(const uint8_t addr, const std::vector<uint8_t>& bytes) {
  if (failing() || addr != ADDR) return false;
  for (size_t i = 1; i < bytes.size(); ++i) {
    const auto reg = static_cast<uint8_t>(bytes[0] + i - 1);
    device.writes.push_back({addr, reg, bytes[i]});
    device.regs[reg] = bytes[i];
  }
  return true;
}

bool hostI2cRead(const uint8_t addr, const uint8_t reg, uint8_t* out, const size_t length) {
  if (failing() || addr != ADDR) return false;
  for (size_t i = 0; i < length; ++i) out[i] = device.regs[static_cast<uint8_t>(reg + i)];
  return true;
}

TEST_F(RtcPcf8563, BeginProbesAndRetriesAsTheSdk) {
  for (int failAt = 0; failAt <= 6; ++failAt) {
    for (const bool dead : {false, true}) {
      device = {};
      device.failAt = failAt;
      device.dead = dead;
      hostMillis = 0;
      Rtc rtc;
      const Outcome sdk = traced(rtc.begin());
      device = {};
      device.failAt = failAt;
      device.dead = dead;
      hostMillis = 0;
      const Outcome ours = traced(pw_rtc_begin(&bus, ADDR));
      expectSame(sdk, ours, "begin", failAt * 2 + dead);
    }
  }
}

TEST_F(RtcPcf8563, NowReadsAsTheSdk) {
  Rtc rtc = begunSdk();
  for (int i = 0; i < 20000; ++i) {
    std::array<uint8_t, 7> raw;
    for (auto& b : raw) b = byte();
    if (i % 3 == 0) raw[0] &= 0x7F;  // mostly a running oscillator
    const int failAt = i % 7 == 0 ? 1 : 0;

    std::copy(raw.begin(), raw.end(), device.regs.begin() + 2);
    device.failAt = failAt;
    resetTrace();
    Rtc::DateTime sdkTime{9, 9, 9, 9, 9, 9, 9};
    const bool sdkOk = rtc.now(sdkTime);
    const Outcome sdk = traced(sdkOk, sdkTime);

    std::copy(raw.begin(), raw.end(), device.regs.begin() + 2);
    device.failAt = failAt;
    resetTrace();
    pw_rtc_time t{9, 9, 9, 9, 9, 9, 9};
    const bool ok = pw_rtc_now(&bus, ADDR, &t);
    const Outcome ours = traced(ok, {t.year, t.month, t.day, t.hour, t.minute, t.second, t.weekday});
    expectSame(sdk, ours, "now", i);
    if (HasFailure()) return;
  }
}

TEST_F(RtcPcf8563, SetWritesAsTheSdk) {
  Rtc rtc = begunSdk();
  for (int i = 0; i < 20000; ++i) {
    Rtc::DateTime dt;
    dt.year = i % 4 == 0 ? static_cast<uint16_t>(rng()) : static_cast<uint16_t>(1900 + rng() % 200);
    const bool valid = i % 2 == 0;
    dt.month = valid ? static_cast<uint8_t>(1 + rng() % 12) : byte();
    dt.day = valid ? static_cast<uint8_t>(1 + rng() % 31) : byte();
    dt.hour = valid ? static_cast<uint8_t>(rng() % 24) : byte();
    dt.minute = valid ? static_cast<uint8_t>(rng() % 60) : byte();
    dt.second = valid ? static_cast<uint8_t>(rng() % 60) : byte();
    dt.weekday = valid ? static_cast<uint8_t>(rng() % 7) : byte();
    const int failAt = i % 5 == 0 ? 1 : 0;

    device.failAt = failAt;
    resetTrace();
    const Outcome sdk = traced(rtc.set(dt));

    device.failAt = failAt;
    resetTrace();
    const pw_rtc_time t = toPw(dt);
    const Outcome ours = traced(pw_rtc_set(&bus, ADDR, &t));
    expectSame(sdk, ours, "set", i);
    if (HasFailure()) return;
  }
}

TEST_F(RtcPcf8563, EpochIsTheOldHalClocks) {
  for (int i = 0; i < 200000; ++i) {
    Rtc::DateTime dt;
    const bool valid = i % 2 == 0;
    dt.year = valid ? static_cast<uint16_t>(1900 + rng() % 200) : static_cast<uint16_t>(rng());
    dt.month = valid ? static_cast<uint8_t>(1 + rng() % 12) : byte();
    dt.day = valid ? static_cast<uint8_t>(1 + rng() % 31) : byte();
    dt.hour = valid ? static_cast<uint8_t>(rng() % 24) : byte();
    dt.minute = valid ? static_cast<uint8_t>(rng() % 60) : byte();
    dt.second = valid ? static_cast<uint8_t>(rng() % 60) : byte();
    const pw_rtc_time t = toPw(dt);
    ASSERT_EQ(pw_rtc_epoch(&t), oldEpochFromUtc(dt)) << "case " << i;
  }
}

TEST_F(RtcPcf8563, NullArgumentsFail) {
  pw_rtc_time t{};
  EXPECT_FALSE(pw_rtc_begin(nullptr, ADDR));
  EXPECT_FALSE(pw_rtc_now(nullptr, ADDR, &t));
  EXPECT_FALSE(pw_rtc_now(&bus, ADDR, nullptr));
  EXPECT_FALSE(pw_rtc_set(&bus, ADDR, nullptr));
  EXPECT_EQ(pw_rtc_epoch(nullptr), 0);
}
