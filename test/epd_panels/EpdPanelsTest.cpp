// purestwhite-epd's UC8279 and UC8179 (black and white) against freeink-sdk's EpdBus and its
// Uc8279X4Driver and Uc8179Driver, which they replace in the bare-metal firmware. Each side drives its own model
// controller; both must send the same chip-select frames (each byte with its data/command level), pulse reset the same
// way, read BUSY and pause the same, and never write while the controller is busy.
#include <BoardConfig.h>
#include <gtest/gtest.h>
#include <purestwhite/epd.h>

#include <random>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

#include "PanelModel.h"
#include "bus/EpdBus.h"
#include "driver/Uc8179Driver.h"
#include "driver/Uc8279X4Driver.h"

namespace {

using namespace epd_model;

// As bembeyaz-ffi's CPanel, which the states are cast to.
static_assert(sizeof(pw_uc8279x4) == 64 && alignof(pw_uc8279x4) == 8);
static_assert(sizeof(pw_uc8179) == 64 && alignof(pw_uc8179) == 8);

// One driver on each side: the SDK's and ours through the C ABI.
struct Uc8279 {
  using Sdk = freeink::Uc8279X4Driver;
  using State = pw_uc8279x4;
  static constexpr auto init = pw_uc8279x4_init;
  static constexpr auto begin = pw_uc8279x4_begin;
  static constexpr auto display = pw_uc8279x4_display;
  static constexpr auto deep_sleep = pw_uc8279x4_deep_sleep;
  static constexpr auto request_resync = pw_uc8279x4_request_resync;
  static constexpr auto skip_initial_resync = pw_uc8279x4_skip_initial_resync;
};

struct Uc8179 {
  using Sdk = freeink::Uc8179Driver;
  using State = pw_uc8179;
  static constexpr auto init = pw_uc8179_init;
  static constexpr auto begin = pw_uc8179_begin;
  static constexpr auto display = pw_uc8179_display;
  static constexpr auto deep_sleep = pw_uc8179_deep_sleep;
  static constexpr auto request_resync = pw_uc8179_request_resync;
  static constexpr auto skip_initial_resync = pw_uc8179_skip_initial_resync;
};

template <typename D>
class EpdPanelsTest : public testing::Test {
 protected:
  void SetUp() override { hook(); }
};

using Drivers = testing::Types<Uc8279, Uc8179>;
// A failure names the controller.
struct DriverNames {
  template <typename T>
  static std::string GetName(int) {
    return std::is_same_v<T, Uc8279> ? "Uc8279" : "Uc8179";
  }
};
TYPED_TEST_SUITE(EpdPanelsTest, Drivers, DriverNames);

std::vector<uint8_t> image(std::mt19937& rng) {
  std::vector<uint8_t> fb(FRAME_BYTES);
  switch (rng() % 4) {
    case 0:
      for (auto& b : fb) b = static_cast<uint8_t>(rng());
      break;
    case 1:
      std::fill(fb.begin(), fb.end(), 0xFF);
      break;
    case 2:
      std::fill(fb.begin(), fb.end(), 0x00);
      break;
    default:
      for (size_t i = 0; i < fb.size(); ++i) fb[i] = (i / 100) % 2 ? 0xAA : 0x55;
  }
  return fb;
}

TYPED_TEST(EpdPanelsTest, SessionsMatchTheSdk) {
  for (const uint32_t seed : {1u, 2u, 3u}) {
    std::mt19937 rng(seed * 7919);
    Panel sdkPanel(seed), ourPanel(seed);
    hostMillis = 0;

    active = &sdkPanel;
    freeink::EpdBus sdkBus;
    sdkBus.begin({12, 11, CS, DC, RST, BUSY, -1}, 10000000, freeink::BusyPolarity::UcIdleHigh);
    typename TypeParam::Sdk sdk;
    sdkPanel.events.clear();

    active = &ourPanel;
    hostMillis = 0;
    const pw_epd_bus bus = ourBus(ourPanel);
    typename TypeParam::State ours;
    TypeParam::init(&ours);

    // Each side keeps its own clock: run one, then the other, from the same start.
    const auto step = [&](const std::string& name, auto&& sdkStep, auto&& ourStep) {
      const unsigned long t = hostMillis;
      active = &sdkPanel;
      sdkStep();
      const unsigned long sdkEnd = hostMillis;
      hostMillis = t;
      active = &ourPanel;
      ourStep();
      ASSERT_EQ(hostMillis, sdkEnd) << name;
      same(sdkPanel, ourPanel, name);
    };

    step("begin", [&] { sdk.begin(sdkBus); }, [&] { ASSERT_TRUE(TypeParam::begin(&ours, &bus)); });
    int modes[3] = {};
    int partials = 0, sleepsPowered = 0;
    for (int n = 0; n < 120; ++n) {
      const std::string name = "seed " + std::to_string(seed) + " step " + std::to_string(n);
      const unsigned r = rng() % 20;
      if (r == 0) {
        step(name + " resync", [&] { sdk.requestResync(0); }, [&] { TypeParam::request_resync(&ours); });
      } else if (r == 1) {
        step(name + " skip", [&] { sdk.skipInitialResync(); }, [&] { TypeParam::skip_initial_resync(&ours); });
      } else if (r == 3) {
        // Programming again while the panel may be powered.
        step(name + " again", [&] { sdk.begin(sdkBus); }, [&] { ASSERT_TRUE(TypeParam::begin(&ours, &bus)); });
      } else if (r == 2) {
        step(
            name + " sleep",
            [&] {
              sdk.deepSleep(sdkBus);
              for (const auto& e : sdkPanel.events)
                if (e.kind == Event::Frame && e.bytes == std::vector<std::pair<bool, uint8_t>>{{false, 0x02}})
                  ++sleepsPowered;
            },
            [&] { ASSERT_TRUE(TypeParam::deep_sleep(&ours, &bus)); });
        step(name + " wake", [&] { sdk.begin(sdkBus); }, [&] { ASSERT_TRUE(TypeParam::begin(&ours, &bus)); });
      } else {
        const auto fb = image(rng);
        const uint8_t mode = rng() % 3;
        const bool off = rng() % 3 == 0;
        ++modes[mode];
        const freeink::RefreshMode sdkMode = mode == PW_EPD_FULL   ? freeink::RefreshMode::Full
                                             : mode == PW_EPD_HALF ? freeink::RefreshMode::Half
                                                                   : freeink::RefreshMode::Fast;
        step(
            name + " display " + std::to_string(mode) + (off ? " off" : ""),
            [&] {
              sdk.display(sdkBus, fb.data(), nullptr, sdkMode, off);
              for (const auto& e : sdkPanel.events)
                if (e.kind == Event::Frame && e.bytes == std::vector<std::pair<bool, uint8_t>>{{false, 0x91}})
                  ++partials;
            },
            [&] { ASSERT_TRUE(TypeParam::display(&ours, &bus, fb.data(), fb.size(), mode, off)); });
      }
      if (this->HasFatalFailure()) return;
    }
    for (int m = 0; m < 3; ++m) EXPECT_GT(modes[m], 10) << "mode " << m;
    // The paths the random steps are meant to reach, reached.
    EXPECT_GT(partials, 5);
    EXPECT_GT(sleepsPowered, 0);
    EXPECT_GT(sdkPanel.lateStarts, 2);
  }
}

// Skip and resync only matter after a display: the first display after begin flashes.
TYPED_TEST(EpdPanelsTest, SkipAndResyncMatchTheSdk) {
  Panel sdkPanel(9), ourPanel(9);
  hostMillis = 0;
  active = &sdkPanel;
  freeink::EpdBus sdkBus;
  sdkBus.begin({12, 11, CS, DC, RST, BUSY, -1}, 10000000, freeink::BusyPolarity::UcIdleHigh);
  typename TypeParam::Sdk sdk;
  sdkPanel.events.clear();
  hostMillis = 0;
  active = &ourPanel;
  const pw_epd_bus bus = ourBus(ourPanel);
  typename TypeParam::State ours;
  TypeParam::init(&ours);
  const std::vector<uint8_t> white(FRAME_BYTES, 0xFF), black(FRAME_BYTES, 0x00);
  const auto partial = [](const Panel& p) {
    for (const auto& e : p.events)
      if (e.kind == Event::Frame && e.bytes == std::vector<std::pair<bool, uint8_t>>{{false, 0x91}}) return true;
    return false;
  };
  // Each step on both sides from the same clock; whether it was partial, which must agree.
  const auto step = [&](auto&& sdkStep, auto&& ourStep) {
    const unsigned long t = hostMillis;
    active = &sdkPanel;
    sdkStep();
    const unsigned long end = hostMillis;
    hostMillis = t;
    active = &ourPanel;
    ourStep();
    EXPECT_EQ(hostMillis, end);
    const bool wasPartial = partial(sdkPanel);
    EXPECT_EQ(wasPartial, partial(ourPanel));
    same(sdkPanel, ourPanel, "skip/resync");
    return wasPartial;
  };
  const auto show = [&](const std::vector<uint8_t>& fb) {
    return step([&] { sdk.display(sdkBus, fb.data(), nullptr, freeink::RefreshMode::Fast, false); },
                [&] { EXPECT_TRUE(TypeParam::display(&ours, &bus, fb.data(), fb.size(), PW_EPD_FAST, false)); });
  };
  const auto begin = [&] { step([&] { sdk.begin(sdkBus); }, [&] { EXPECT_TRUE(TypeParam::begin(&ours, &bus)); }); };
  const auto skip = [&] { step([&] { sdk.skipInitialResync(); }, [&] { TypeParam::skip_initial_resync(&ours); }); };
  const auto resync = [&] { step([&] { sdk.requestResync(0); }, [&] { TypeParam::request_resync(&ours); }); };
  begin();
  skip();
  EXPECT_FALSE(show(white));  // first after begin: a flash despite the skip
  EXPECT_TRUE(show(black));
  resync();
  EXPECT_FALSE(show(white));
  resync();
  skip();
  EXPECT_TRUE(show(black));
  begin();
  EXPECT_FALSE(show(white));  // begin again: a flash
}

TYPED_TEST(EpdPanelsTest, BadCallsFailWithoutTouchingTheBus) {
  Panel panel(1);
  active = &panel;
  const pw_epd_bus bus = ourBus(panel);
  typename TypeParam::State ours;
  TypeParam::init(&ours);
  ASSERT_TRUE(TypeParam::begin(&ours, &bus));
  const std::vector<uint8_t> fb(FRAME_BYTES, 0xFF);
  ASSERT_TRUE(TypeParam::display(&ours, &bus, fb.data(), fb.size(), PW_EPD_FULL, false));
  panel.events.clear();
  panel.lineTouches = 0;
  EXPECT_FALSE(TypeParam::display(&ours, &bus, fb.data(), fb.size() - 1, PW_EPD_FULL, false));
  EXPECT_FALSE(TypeParam::display(&ours, &bus, fb.data(), fb.size(), 3, false));
  EXPECT_FALSE(TypeParam::display(&ours, &bus, nullptr, fb.size(), PW_EPD_FULL, false));
  EXPECT_FALSE(TypeParam::display(nullptr, &bus, fb.data(), fb.size(), PW_EPD_FULL, false));
  EXPECT_FALSE(TypeParam::display(&ours, nullptr, fb.data(), fb.size(), PW_EPD_FULL, false));
  EXPECT_FALSE(TypeParam::begin(&ours, nullptr));
  EXPECT_FALSE(TypeParam::begin(nullptr, &bus));
  EXPECT_FALSE(TypeParam::deep_sleep(&ours, nullptr));
  TypeParam::request_resync(nullptr);
  TypeParam::skip_initial_resync(nullptr);
  TypeParam::init(nullptr);
  // A bus missing any callback is refused.
  for (int missing = 0; missing < 6; ++missing) {
    pw_epd_bus partialBus = bus;
    switch (missing) {
      case 0:
        partialBus.write = nullptr;
        break;
      case 1:
        partialBus.set_cs = nullptr;
        break;
      case 2:
        partialBus.set_dc = nullptr;
        break;
      case 3:
        partialBus.set_rst = nullptr;
        break;
      case 4:
        partialBus.busy_high = nullptr;
        break;
      default:
        partialBus.pause_ms = nullptr;
    }
    EXPECT_FALSE(TypeParam::begin(&ours, &partialBus)) << missing;
    EXPECT_FALSE(TypeParam::display(&ours, &partialBus, fb.data(), fb.size(), PW_EPD_FULL, false)) << missing;
    EXPECT_FALSE(TypeParam::deep_sleep(&ours, &partialBus)) << missing;
  }
  EXPECT_EQ(panel.lineTouches, 0);
  EXPECT_TRUE(panel.events.empty());
  EXPECT_FALSE(panel.csLow);
  // A failed plane write mid-burst fails the call and still ends its frame; the next Fast flashes.
  panel.failBursts = true;
  EXPECT_FALSE(TypeParam::display(&ours, &bus, fb.data(), fb.size(), PW_EPD_FAST, false));
  EXPECT_FALSE(panel.csLow);
  EXPECT_GT(panel.lineTouches, 0);
  panel.failBursts = false;
  panel.events.clear();
  ASSERT_TRUE(TypeParam::display(&ours, &bus, fb.data(), fb.size(), PW_EPD_FAST, false));
  ASSERT_FALSE(panel.events.empty());
  for (const auto& e : panel.events) EXPECT_NE(e.bytes, (std::vector<std::pair<bool, uint8_t>>{{false, 0x91}}));
}

}  // namespace
