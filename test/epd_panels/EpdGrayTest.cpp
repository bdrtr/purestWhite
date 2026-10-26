// purestwhite-epd's UC8179 grayscale passes against freeink-sdk's Uc8179Driver, on the model controller of
// PanelModel.h: the EPUB reader's Overlay pages, the image viewer's Absolute ones and the sleep screen's Direct ones,
// mixed with black-and-white refreshes, sleeps, resyncs and calls out of order. Built twice: with BOARD_HAS_PSRAM, as
// on the X4 Pro, where the SDK keeps the black-and-white base and ours gets a base buffer, and without, where neither
// has one.
#include <BoardConfig.h>
#include <gtest/gtest.h>
#include <purestwhite/epd.h>

#include <random>
#include <string>
#include <vector>

#include "PanelModel.h"
#include "bus/EpdBus.h"
#include "driver/Uc8179Driver.h"

namespace {

using namespace epd_model;
using freeink::GrayscaleMode;
using freeink::RefreshMode;

#if defined(BOARD_HAS_PSRAM)
constexpr bool kBase = true;
#else
constexpr bool kBase = false;
#endif

// A 4-level page (per pixel 0 black, 1 dark, 2 light, 3 white) as the callers encode it: the black-and-white base,
// Overlay's masks and the absolute planes.
struct Page {
  std::vector<uint8_t> bw, overlayLsb, overlayMsb, absLsb, absMsb;
};

Page page(std::mt19937& rng) {
  Page p;
  for (auto* v : {&p.bw, &p.overlayLsb, &p.overlayMsb, &p.absLsb, &p.absMsb}) v->assign(FRAME_BYTES, 0);
  const unsigned kind = rng() % 4;
  for (size_t i = 0; i < FRAME_BYTES; ++i) {
    for (int bit = 7; bit >= 0; --bit) {
      unsigned level = 3;
      if (kind == 0) level = rng() % 4;
      if (kind == 1) level = rng() % 8 < 6 ? 3 : rng() % 3;  // text: mostly white
      if (kind == 2) level = 0;
      const uint8_t m = static_cast<uint8_t>(1u << bit);
      if (level == 3) p.bw[i] |= m;
      if (level == 1) p.overlayLsb[i] |= m;
      if (level == 1 || level == 2) p.overlayMsb[i] |= m;
      if (level == 1 || level == 3) p.absLsb[i] |= m;
      if (level == 2 || level == 3) p.absMsb[i] |= m;
    }
  }
  return p;
}

// Whether the panel got a frame starting with command c.
bool sent(const Panel& p, uint8_t c) {
  for (const auto& e : p.events)
    if (e.kind == Event::Frame && !e.bytes.empty() && e.bytes[0] == std::pair<bool, uint8_t>{false, c}) return true;
  return false;
}

// The SDK's driver and ours, each on its own model panel and clock.
struct Session {
  Panel sdkPanel, ourPanel;
  freeink::EpdBus sdkBus;
  freeink::Uc8179Driver sdk;
  pw_epd_bus bus{};
  pw_uc8179 ours{};
  std::vector<uint8_t> base;
  // Paths reached, to check the random steps reach them.
  int transitions = 0, directConfigs = 0, slowerImages = 0, fromDirect = 0, plainAfterGray = 0;

  explicit Session(uint32_t seed) : sdkPanel(seed), ourPanel(seed) {
    hostMillis = 0;
    active = &sdkPanel;
    sdkBus.begin({12, 11, CS, DC, RST, BUSY, -1}, 10000000, freeink::BusyPolarity::UcIdleHigh);
    sdkPanel.events.clear();
    hostMillis = 0;
    active = &ourPanel;
    bus = ourBus(ourPanel);
    if (kBase) {
      base.assign(FRAME_BYTES, 0x5A);
      EXPECT_TRUE(pw_uc8179_init_with_gray_base(&ours, base.data(), base.size()));
    } else {
      pw_uc8179_init(&ours);
    }
  }

  // Runs one step on each side from the same clock; both must end at the same time with the same events.
  template <typename S, typename O>
  void step(const std::string& name, S&& sdkStep, O&& ourStep) {
    const unsigned long t = hostMillis;
    active = &sdkPanel;
    sdkStep();
    const unsigned long sdkEnd = hostMillis;
    if (sent(sdkPanel, 0x90)) ++transitions;
    if (sent(sdkPanel, 0x52)) ++directConfigs;
    for (size_t i = 0; i + 1 < sdkPanel.events.size(); ++i)
      if (sdkPanel.events[i].bytes == std::vector<std::pair<bool, uint8_t>>{{false, 0x30}} &&
          sdkPanel.events[i + 1].bytes == std::vector<std::pair<bool, uint8_t>>{{true, 0x05}})
        ++slowerImages;
    hostMillis = t;
    active = &ourPanel;
    ourStep();
    ASSERT_EQ(hostMillis, sdkEnd) << name;
    same(sdkPanel, ourPanel, name);
  }
};

RefreshMode sdkRefresh(uint8_t mode) {
  return mode == PW_EPD_FULL ? RefreshMode::Full : mode == PW_EPD_HALF ? RefreshMode::Half : RefreshMode::Fast;
}

class EpdGrayTest : public testing::Test {
 protected:
  void SetUp() override { hook(); }
};

TEST_F(EpdGrayTest, PassesMatchTheSdk) {
  // The rarer paths, over all seeds.
  int fromDirect = 0, plainAfterGray = 0;
  for (const uint32_t seed : {1u, 2u, 3u, 4u}) {
    std::mt19937 rng(seed * 104729);
    Session s(seed);
    auto& sdk = s.sdk;
    auto& sdkBus = s.sdkBus;
    auto* ours = &s.ours;
    const pw_epd_bus* bus = &s.bus;
    const auto begin = [&](const std::string& name) {
      s.step(name + " begin", [&] { sdk.begin(sdkBus); }, [&] { ASSERT_TRUE(pw_uc8179_begin(ours, bus)); });
    };
    const auto bwDisplay = [&](const std::string& name, const std::vector<uint8_t>& fb, uint8_t mode, bool off) {
      s.step(
          name + " display " + std::to_string(mode),
          [&] { sdk.display(sdkBus, fb.data(), nullptr, sdkRefresh(mode), off); },
          [&] { ASSERT_TRUE(pw_uc8179_display(ours, bus, fb.data(), fb.size(), mode, off)); });
    };
    const auto planes = [&](const std::string& name, const std::vector<uint8_t>& lsb, const std::vector<uint8_t>& msb) {
      s.step(
          name + " lsb", [&] { sdk.copyGrayscaleLsb(sdkBus, lsb.data()); },
          [&] { ASSERT_TRUE(pw_uc8179_copy_grayscale_lsb(ours, bus, lsb.data(), lsb.size())); });
      s.step(
          name + " msb", [&] { sdk.copyGrayscaleMsb(sdkBus, msb.data()); },
          [&] { ASSERT_TRUE(pw_uc8179_copy_grayscale_msb(ours, bus, msb.data(), msb.size())); });
    };
    const auto gray = [&](const std::string& name, const std::vector<uint8_t>& fb, bool off, bool factory) {
      s.step(
          name + " gray", [&] { sdk.displayGray(sdkBus, fb.data(), off, nullptr, factory); },
          [&] { ASSERT_TRUE(pw_uc8179_display_gray(ours, bus, off, factory)); });
    };
    const auto cleanup = [&](const std::string& name, const std::vector<uint8_t>* bw) {
      s.step(
          name + " cleanup", [&] { sdk.cleanupGrayscaleBuffers(sdkBus, bw ? bw->data() : nullptr); },
          [&] { ASSERT_TRUE(pw_uc8179_cleanup_grayscale(ours, bus, bw ? bw->data() : nullptr, bw ? bw->size() : 0)); });
    };
    const auto beginGray = [&](const std::string& name, const std::vector<uint8_t>& fb, uint8_t mode, uint8_t fallback,
                               bool off) {
      const GrayscaleMode sdkMode = mode == PW_EPD_OVERLAY    ? GrayscaleMode::Overlay
                                    : mode == PW_EPD_ABSOLUTE ? GrayscaleMode::Absolute
                                                              : GrayscaleMode::Direct;
      s.step(
          name + " begin gray " + std::to_string(mode),
          [&] { sdk.beginGrayscale(sdkBus, fb.data(), sdkMode, sdkRefresh(fallback), off); },
          [&] { ASSERT_TRUE(pw_uc8179_begin_grayscale(ours, bus, fb.data(), fb.size(), mode, fallback, off)); });
    };
    const auto resync = [&](const std::string& name) {
      s.step(name + " resync", [&] { sdk.requestResync(1); }, [&] { pw_uc8179_request_resync(ours); });
    };

    begin("start");
    if (this->HasFatalFailure()) return;
    int readerPages = 0, imagePages = 0, sleepScreens = 0, strays = 0;
    bool directShown = false, afterReader = false;
    for (int n = 0; n < 160; ++n) {
      const std::string name = "seed " + std::to_string(seed) + " step " + std::to_string(n);
      const Page p = page(rng);
      const bool off = rng() % 4 == 0;
      const unsigned r = rng() % 40;
      const bool readerBefore = afterReader;
      afterReader = false;
      if (r < 14) {
        // The reader's text page: the base (mostly a Fast one), the masks, the gray refresh, the planes back.
        const uint8_t fallback = rng() % 5 == 0 ? static_cast<uint8_t>(rng() % 2) : static_cast<uint8_t>(PW_EPD_FAST);
        if (rng() % 6 == 0) {
          bwDisplay(name, p.bw, PW_EPD_HALF, off);  // the periodic cleanup page: an explicit Half
        } else {
          beginGray(name, p.bw, PW_EPD_OVERLAY, fallback, off);
        }
        planes(name, p.overlayLsb, p.overlayMsb);
        gray(name, p.overlayMsb, off, false);
        cleanup(name, &p.bw);
        ++readerPages;
        directShown = false;
        afterReader = true;
      } else if (r < 18) {
        // The image viewer: absolute planes after a Half base, a resync, the page redrawn, the planes back.
        beginGray(name, p.bw, PW_EPD_ABSOLUTE, PW_EPD_HALF, off);
        planes(name, p.absLsb, p.absMsb);
        gray(name, p.absMsb, off, true);
        resync(name);
        bwDisplay(name, p.bw, rng() % 2 ? PW_EPD_FAST : PW_EPD_HALF, off);
        cleanup(name, rng() % 3 ? &p.bw : nullptr);
        ++imagePages;
        directShown = false;
      } else if (r < 21) {
        // The sleep screen: Direct, then a resync; sometimes it sleeps and wakes.
        beginGray(name, p.bw, PW_EPD_DIRECT, PW_EPD_HALF, off);
        planes(name, p.absLsb, p.absMsb);
        gray(name, p.absMsb, off, true);
        resync(name);
        ++sleepScreens;
        directShown = true;
        if (rng() % 2) {
          s.step(
              name + " sleep", [&] { sdk.deepSleep(sdkBus); }, [&] { ASSERT_TRUE(pw_uc8179_deep_sleep(ours, bus)); });
          begin(name + " wake");
          directShown = false;
        }
      } else if (r < 28) {
        if (directShown) ++s.fromDirect;
        bwDisplay(name, p.bw, static_cast<uint8_t>(rng() % 3), off);
        directShown = false;
      } else if (r < 31) {
        // The SDK's asynchronous refresh, as the reader's overlays use it: never the transition.
        if (readerBefore) ++s.plainAfterGray;
        if (directShown) ++s.fromDirect;
        const uint8_t mode = static_cast<uint8_t>(rng() % 3);
        s.step(
            name + " plain " + std::to_string(mode),
            [&] {
              sdk.displayStart(sdkBus, p.bw.data(), nullptr, sdkRefresh(mode), off);
              sdk.displayFinish(sdkBus, p.bw.data());
            },
            [&] { ASSERT_TRUE(pw_uc8179_display_plain(ours, bus, p.bw.data(), FRAME_BYTES, mode, off)); });
        directShown = false;
      } else if (r < 32) {
        begin(name);
        directShown = false;
      } else if (r < 33) {
        resync(name);
      } else if (r < 34) {
        s.step(name + " skip", [&] { sdk.skipInitialResync(); }, [&] { pw_uc8179_skip_initial_resync(ours); });
      } else {
        // Calls out of order: one step of a pass alone.
        ++strays;
        switch (rng() % 6) {
          case 0:
            s.step(
                name + " stray lsb", [&] { sdk.copyGrayscaleLsb(sdkBus, p.overlayLsb.data()); },
                [&] { ASSERT_TRUE(pw_uc8179_copy_grayscale_lsb(ours, bus, p.overlayLsb.data(), FRAME_BYTES)); });
            break;
          case 1:
            s.step(
                name + " stray msb", [&] { sdk.copyGrayscaleMsb(sdkBus, p.overlayMsb.data()); },
                [&] { ASSERT_TRUE(pw_uc8179_copy_grayscale_msb(ours, bus, p.overlayMsb.data(), FRAME_BYTES)); });
            break;
          case 2:
            gray(name + " stray", p.overlayMsb, off, rng() % 2);
            break;
          case 3:
            cleanup(name + " stray", rng() % 2 ? &p.bw : nullptr);
            break;
          case 4: {
            const uint8_t fallback = static_cast<uint8_t>(rng() % 3);
            s.step(
                name + " stray base", [&] { sdk.displayGrayscaleBase(sdkBus, p.bw.data(), sdkRefresh(fallback), off); },
                [&] {
                  ASSERT_TRUE(pw_uc8179_display_grayscale_base(ours, bus, p.bw.data(), FRAME_BYTES, fallback, off));
                });
            break;
          }
          default:
            beginGray(name + " stray", p.bw, static_cast<uint8_t>(rng() % 3), static_cast<uint8_t>(rng() % 3), off);
        }
        directShown = false;
      }
      if (this->HasFatalFailure()) return;
    }
    // The paths the random steps are meant to reach, reached.
    EXPECT_GT(readerPages, 30);
    EXPECT_GT(imagePages, 5);
    EXPECT_GT(sleepScreens, 3);
    EXPECT_GT(strays, 10);
    EXPECT_GT(s.transitions, 10);
    EXPECT_GT(s.directConfigs, 2);
    EXPECT_GT(s.slowerImages, 3);
    fromDirect += s.fromDirect;
    plainAfterGray += s.plainAfterGray;
  }
  EXPECT_GT(fromDirect, 1);
  EXPECT_GT(plainAfterGray, 3);
}

// A reader's run of text pages: after the first gray page every Fast base is the non-flashing transition, and with
// a base buffer the Overlay masks reach the controller as the absolute planes.
TEST_F(EpdGrayTest, ReaderPagesMatchTheSdk) {
  Session s(7);
  std::mt19937 rng(7);
  const pw_epd_bus* bus = &s.bus;
  s.step("begin", [&] { s.sdk.begin(s.sdkBus); }, [&] { ASSERT_TRUE(pw_uc8179_begin(&s.ours, bus)); });
  for (int n = 0; n < 12; ++n) {
    const Page p = page(rng);
    const std::string name = "page " + std::to_string(n);
    s.step(
        name + " base",
        [&] { s.sdk.beginGrayscale(s.sdkBus, p.bw.data(), GrayscaleMode::Overlay, RefreshMode::Fast, false); },
        [&] {
          ASSERT_TRUE(
              pw_uc8179_begin_grayscale(&s.ours, bus, p.bw.data(), FRAME_BYTES, PW_EPD_OVERLAY, PW_EPD_FAST, false));
        });
    s.step(
        name + " lsb", [&] { s.sdk.copyGrayscaleLsb(s.sdkBus, p.overlayLsb.data()); },
        [&] { ASSERT_TRUE(pw_uc8179_copy_grayscale_lsb(&s.ours, bus, p.overlayLsb.data(), FRAME_BYTES)); });
    s.step(
        name + " msb", [&] { s.sdk.copyGrayscaleMsb(s.sdkBus, p.overlayMsb.data()); },
        [&] { ASSERT_TRUE(pw_uc8179_copy_grayscale_msb(&s.ours, bus, p.overlayMsb.data(), FRAME_BYTES)); });
    s.step(
        name + " gray", [&] { s.sdk.displayGray(s.sdkBus, p.overlayMsb.data(), false, nullptr, false); },
        [&] { ASSERT_TRUE(pw_uc8179_display_gray(&s.ours, bus, false, false)); });
    s.step(
        name + " cleanup", [&] { s.sdk.cleanupGrayscaleBuffers(s.sdkBus, p.bw.data()); },
        [&] { ASSERT_TRUE(pw_uc8179_cleanup_grayscale(&s.ours, bus, p.bw.data(), FRAME_BYTES)); });
    if (HasFatalFailure()) return;
  }
  // Every page after the first: the base's transition ran (the first page's base flashes; each later one follows a
  // gray page).
  EXPECT_EQ(s.transitions, 11);
}

// Passes broken off or repeated, which the random sessions reach only rarely: a Direct pass with one plane, planes
// sent again after the gray refresh, a gray refresh without planes.
TEST_F(EpdGrayTest, OutOfOrderPassesMatchTheSdk) {
  Session s(11);
  std::mt19937 rng(11);
  const pw_epd_bus* bus = &s.bus;
  auto* ours = &s.ours;
  const Page p = page(rng), q = page(rng);
  const auto begin = [&] {
    s.step("begin", [&] { s.sdk.begin(s.sdkBus); }, [&] { ASSERT_TRUE(pw_uc8179_begin(ours, bus)); });
  };
  const auto beginGray = [&](const char* name, GrayscaleMode mode, uint8_t ourMode, RefreshMode fallback,
                             uint8_t ourFallback) {
    s.step(
        name, [&] { s.sdk.beginGrayscale(s.sdkBus, p.bw.data(), mode, fallback, false); },
        [&] {
          ASSERT_TRUE(pw_uc8179_begin_grayscale(ours, bus, p.bw.data(), FRAME_BYTES, ourMode, ourFallback, false));
        });
  };
  const auto lsb = [&](const char* name, const std::vector<uint8_t>& plane) {
    s.step(
        name, [&] { s.sdk.copyGrayscaleLsb(s.sdkBus, plane.data()); },
        [&] { ASSERT_TRUE(pw_uc8179_copy_grayscale_lsb(ours, bus, plane.data(), FRAME_BYTES)); });
  };
  const auto msb = [&](const char* name, const std::vector<uint8_t>& plane) {
    s.step(
        name, [&] { s.sdk.copyGrayscaleMsb(s.sdkBus, plane.data()); },
        [&] { ASSERT_TRUE(pw_uc8179_copy_grayscale_msb(ours, bus, plane.data(), FRAME_BYTES)); });
  };
  const auto gray = [&](const char* name, bool factory) {
    s.step(
        name, [&] { s.sdk.displayGray(s.sdkBus, p.overlayMsb.data(), false, nullptr, factory); },
        [&] { ASSERT_TRUE(pw_uc8179_display_gray(ours, bus, false, factory)); });
  };
  begin();
  // Direct with one plane: the gray refresh sends nothing until the other one has come.
  beginGray("direct", GrayscaleMode::Direct, PW_EPD_DIRECT, RefreshMode::Half, PW_EPD_HALF);
  lsb("direct lsb", p.absLsb);
  gray("direct gray, one plane", true);
  EXPECT_TRUE(s.sdkPanel.events.empty() && s.ourPanel.events.empty());
  msb("direct msb", p.absMsb);
  gray("direct gray", true);
  // A begin refused for its frame still ends the pass under way: the next planes and gray refresh are Overlay's.
  beginGray("direct again", GrayscaleMode::Direct, PW_EPD_DIRECT, RefreshMode::Half, PW_EPD_HALF);
  lsb("direct lsb again", p.absLsb);
  s.step(
      "refused begin",
      [&] { s.sdk.beginGrayscale(s.sdkBus, nullptr, GrayscaleMode::Overlay, RefreshMode::Fast, false); },
      [&] { ASSERT_FALSE(pw_uc8179_begin_grayscale(ours, bus, nullptr, 0, PW_EPD_OVERLAY, PW_EPD_FAST, false)); });
  msb("msb after refused begin", p.absMsb);
  gray("gray after refused begin", true);
  // Overlay, then each plane again after the gray refresh, and another gray refresh.
  beginGray("overlay", GrayscaleMode::Overlay, PW_EPD_OVERLAY, RefreshMode::Full, PW_EPD_FULL);
  lsb("overlay lsb", p.overlayLsb);
  msb("overlay msb", p.overlayMsb);
  gray("overlay gray", false);
  msb("msb again", q.overlayMsb);
  lsb("lsb again", q.overlayLsb);
  msb("msb after lsb again", q.overlayMsb);
  gray("gray again", false);
  // Absolute: a gray refresh before the planes, then the pass, then no base given back.
  beginGray("absolute", GrayscaleMode::Absolute, PW_EPD_ABSOLUTE, RefreshMode::Half, PW_EPD_HALF);
  gray("absolute gray, no planes", true);
  lsb("absolute lsb", p.absLsb);
  msb("absolute msb", p.absMsb);
  gray("absolute gray", true);
  s.step(
      "cleanup without base", [&] { s.sdk.cleanupGrayscaleBuffers(s.sdkBus, nullptr); },
      [&] { ASSERT_TRUE(pw_uc8179_cleanup_grayscale(ours, bus, nullptr, 0)); });
  s.step(
      "fast after", [&] { s.sdk.display(s.sdkBus, q.bw.data(), nullptr, RefreshMode::Fast, true); },
      [&] { ASSERT_TRUE(pw_uc8179_display(ours, bus, q.bw.data(), FRAME_BYTES, PW_EPD_FAST, true)); });
}

TEST_F(EpdGrayTest, BadCallsFailWithoutTouchingTheBus) {
  Panel panel(1);
  active = &panel;
  const pw_epd_bus bus = ourBus(panel);
  pw_uc8179 ours;
  std::vector<uint8_t> base(FRAME_BYTES);
  EXPECT_FALSE(pw_uc8179_init_with_gray_base(&ours, base.data(), FRAME_BYTES - 1));
  EXPECT_FALSE(pw_uc8179_init_with_gray_base(&ours, nullptr, FRAME_BYTES));
  ASSERT_TRUE(pw_uc8179_init_with_gray_base(&ours, base.data(), base.size()));
  ASSERT_TRUE(pw_uc8179_begin(&ours, &bus));
  const std::vector<uint8_t> fb(FRAME_BYTES, 0xFF);
  panel.events.clear();
  panel.lineTouches = 0;
  const size_t shortLen = FRAME_BYTES - 1;
  EXPECT_FALSE(pw_uc8179_begin_grayscale(&ours, &bus, fb.data(), shortLen, PW_EPD_OVERLAY, PW_EPD_FAST, false));
  EXPECT_FALSE(pw_uc8179_begin_grayscale(&ours, &bus, nullptr, FRAME_BYTES, PW_EPD_ABSOLUTE, PW_EPD_FAST, false));
  EXPECT_FALSE(pw_uc8179_begin_grayscale(&ours, &bus, fb.data(), FRAME_BYTES, 3, PW_EPD_FAST, false));
  EXPECT_FALSE(pw_uc8179_begin_grayscale(&ours, &bus, fb.data(), FRAME_BYTES, PW_EPD_OVERLAY, 3, false));
  EXPECT_FALSE(pw_uc8179_display_grayscale_base(&ours, &bus, fb.data(), shortLen, PW_EPD_FAST, false));
  EXPECT_FALSE(pw_uc8179_display_grayscale_base(&ours, &bus, fb.data(), FRAME_BYTES, 3, false));
  EXPECT_FALSE(pw_uc8179_copy_grayscale_lsb(&ours, &bus, fb.data(), shortLen));
  EXPECT_FALSE(pw_uc8179_copy_grayscale_lsb(&ours, &bus, nullptr, FRAME_BYTES));
  EXPECT_FALSE(pw_uc8179_copy_grayscale_msb(&ours, &bus, fb.data(), shortLen));
  EXPECT_FALSE(pw_uc8179_cleanup_grayscale(&ours, &bus, fb.data(), shortLen));
  EXPECT_FALSE(pw_uc8179_display_gray(nullptr, &bus, false, false));
  EXPECT_FALSE(pw_uc8179_display_gray(&ours, nullptr, false, false));
  EXPECT_FALSE(pw_uc8179_copy_grayscale_lsb(nullptr, &bus, fb.data(), FRAME_BYTES));
  EXPECT_FALSE(pw_uc8179_cleanup_grayscale(&ours, nullptr, fb.data(), FRAME_BYTES));
  pw_epd_bus partialBus = bus;
  partialBus.busy_high = nullptr;
  EXPECT_FALSE(
      pw_uc8179_begin_grayscale(&ours, &partialBus, fb.data(), FRAME_BYTES, PW_EPD_DIRECT, PW_EPD_HALF, false));
  EXPECT_FALSE(pw_uc8179_display_gray(&ours, &partialBus, false, false));
  EXPECT_FALSE(pw_uc8179_init_with_gray_base(nullptr, base.data(), base.size()));
  EXPECT_EQ(panel.lineTouches, 0);
  EXPECT_TRUE(panel.events.empty());
  // Direct does not use the frame: NULL is accepted.
  EXPECT_TRUE(pw_uc8179_begin_grayscale(&ours, &bus, nullptr, 0, PW_EPD_DIRECT, PW_EPD_HALF, false));
  EXPECT_FALSE(panel.events.empty());
}

}  // namespace
