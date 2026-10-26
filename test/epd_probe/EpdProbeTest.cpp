// purestwhite-epd's controller probe against freeink-sdk's XteinkDetect (the X4-family path),
// which the C++ firmware runs at every boot. Both drive the same model controller over the
// panel lines as GPIOs; they must switch, write, read and pause the same and reach the same
// verdict, VER, FLG, MTP and controller.
#include <Arduino.h>
#include <BoardConfig.h>
#include <XteinkDetect.h>
#include <driver/gpio.h>
#include <gtest/gtest.h>
#include <purestwhite/epd.h>

#include <array>
#include <cstring>
#include <map>
#include <random>
#include <string>
#include <vector>

// The layout epd.rs asserts on its side.
static_assert(sizeof(pw_epd_probe_result) == 58 && alignof(pw_epd_probe_result) == 1, "pw_epd_probe_result layout");
static_assert(sizeof(pw_epd_probe_lines) == 7 * sizeof(void*), "pw_epd_probe_lines layout");

namespace {

constexpr int SCLK = 12, SDA = 11, CS = 13, DC = 18, RST = 14, BUSY = 6;

struct Event {
  enum Kind { Mode, Write, Read, PauseMs, PauseUs, Hold } kind;
  int a, b;
  bool operator==(const Event&) const = default;
};

std::string describe(const Event& e) {
  static const char* names[] = {"mode", "write", "read", "ms", "us", "hold"};
  return std::string(names[e.kind]) + " " + std::to_string(e.a) + " " + std::to_string(e.b);
}

// A controller on the panel lines. It shifts in a command on rising clock edges while selected
// with DC low and the data line an output, and answers the nth read of a command with the nth
// of its replies (the last one repeating); beyond a reply, or for a command it does not answer,
// the line floats: high through the pull-up, random when `noisy`, or `held` when something holds
// it (overriding the pull-up: a bus that reads 0x00).
struct Panel {
  std::map<int, std::vector<std::vector<uint8_t>>> replies;
  std::map<int, int> answered;  // replies given per command
  int held = -1;                // LOW or HIGH when the line is held, whatever the pull
  int mtpReads = 0;
  bool noisy = false;
  std::mt19937 rng{1};
  std::array<int, 64> levels{}, modes{};
  int command = 0, commandBits = 0, readBits = 0;
  const std::vector<uint8_t>* reply = nullptr;
  std::vector<Event> events;

  Panel() {
    levels.fill(HIGH);
    modes.fill(INPUT);
  }
};

Panel* active = nullptr;

}  // namespace

// What freeink-sdk's XteinkDetect and our callbacks both call.
void pinMode(int pin, int mode) {
  active->modes.at(pin) = mode;
  active->events.push_back({Event::Mode, pin, mode});
}
void gpio_hold_dis(gpio_num_t pin) { active->events.push_back({Event::Hold, pin, 0}); }
void digitalWrite(int pin, int value) {
  Panel& p = *active;
  p.events.push_back({Event::Write, pin, value});
  if (pin == CS && value == LOW) {
    p.command = p.commandBits = p.readBits = 0;
    p.reply = nullptr;
  }
  if (pin == SCLK && value == HIGH && p.levels[SCLK] == LOW && p.levels[CS] == LOW && p.levels[DC] == LOW &&
      p.modes[SDA] == OUTPUT) {
    p.command = ((p.command << 1) | p.levels[SDA]) & 0xFF;
    if (++p.commandBits == 8) {
      if (p.command == 0xA2) ++p.mtpReads;
      const auto it = p.replies.find(p.command);
      if (it != p.replies.end() && !it->second.empty()) {
        int& n = p.answered[p.command];
        p.reply = &it->second[std::min<size_t>(n, it->second.size() - 1)];
        ++n;
      }
    }
  }
  p.levels.at(pin) = value;
}
int digitalRead(int pin) {
  Panel& p = *active;
  int value;
  if (pin == SDA && p.levels[CS] == LOW && p.levels[DC] == HIGH && p.modes[SDA] != OUTPUT) {
    const size_t byte = static_cast<size_t>(p.readBits) / 8;
    if (p.reply && byte < p.reply->size()) {
      value = ((*p.reply)[byte] >> (7 - p.readBits % 8)) & 1;
    } else if (p.held >= 0) {
      value = p.held;
    } else if (p.noisy) {
      value = static_cast<int>(p.rng() & 1);
    } else {
      value = p.modes[SDA] == INPUT_PULLUP ? HIGH : LOW;
    }
    ++p.readBits;
  } else {
    value = pin == BUSY ? HIGH : p.levels.at(pin);
  }
  p.events.push_back({Event::Read, pin, value});
  return value;
}
void delay(unsigned long ms) { active->events.push_back({Event::PauseMs, static_cast<int>(ms), 0}); }
void delayMicroseconds(unsigned int us) { active->events.push_back({Event::PauseUs, static_cast<int>(us), 0}); }
unsigned long millis() { return 0; }

namespace {

int pinOf(uint8_t line) {
  switch (line) {
    case PW_EPD_SCLK:
      return SCLK;
    case PW_EPD_SDA:
      return SDA;
    case PW_EPD_CS:
      return CS;
    case PW_EPD_DC:
      return DC;
    case PW_EPD_RST:
      return RST;
    default:
      return BUSY;
  }
}

pw_epd_probe_lines ourLines(Panel& panel) {
  pw_epd_probe_lines l{};
  l.context = &panel;
  l.mode = [](void* c, uint8_t line, uint8_t mode) {
    EXPECT_EQ(c, active);
    pinMode(pinOf(line), mode == PW_EPD_OUTPUT ? OUTPUT : mode == PW_EPD_INPUT ? INPUT : INPUT_PULLUP);
  };
  l.write = [](void* c, uint8_t line, bool high) {
    EXPECT_EQ(c, active);
    digitalWrite(pinOf(line), high ? HIGH : LOW);
  };
  l.read = [](void* c, uint8_t line) {
    EXPECT_EQ(c, active);
    return digitalRead(pinOf(line)) == HIGH;
  };
  l.release_reset_hold = [](void* c) {
    EXPECT_EQ(c, active);
    gpio_hold_dis(RST);
  };
  l.pause_us = [](void* c, uint32_t us) {
    EXPECT_EQ(c, active);
    delayMicroseconds(us);
  };
  l.pause_ms = [](void* c, uint32_t ms) {
    EXPECT_EQ(c, active);
    delay(ms);
  };
  return l;
}

// Runs both probes on twin panels; the SDK's controller choice and diagnostics, and ours, must
// agree. Returns ours.
int lastMtpReads = 0;  // MTP reads of the last both(), equal on both sides

pw_epd_probe_result both(const Panel& model, const std::string& name) {
  Panel sdkPanel = model, ourPanel = model;
  BoardConfig::ACTIVE = BoardConfig::XTEINK_X4_PRO;
  active = &sdkPanel;
  freeink::applyXteinkDisplayController();
  const auto& diag = freeink::getXteinkDisplayProbeDiag();

  active = &ourPanel;
  const pw_epd_probe_lines lines = ourLines(ourPanel);
  pw_epd_probe_result ours{};
  EXPECT_TRUE(pw_epd_probe(&lines, &ours)) << name;

  EXPECT_EQ(sdkPanel.events.size(), ourPanel.events.size()) << name;
  for (size_t i = 0; i < std::min(sdkPanel.events.size(), ourPanel.events.size()); ++i) {
    if (!(sdkPanel.events[i] == ourPanel.events[i])) {
      ADD_FAILURE() << name << " event " << i << ": " << describe(sdkPanel.events[i]) << " / "
                    << describe(ourPanel.events[i]);
      break;
    }
  }
  EXPECT_EQ(ours.verdict, diag.verdict) << name;
  EXPECT_EQ(0, std::memcmp(ours.ver, diag.ver, 5)) << name;
  EXPECT_EQ(ours.flg, diag.flg) << name;
  EXPECT_EQ(ours.mtp_valid, diag.mtpValid) << name;
  if (diag.mtpValid) EXPECT_EQ(0, std::memcmp(ours.mtp, diag.mtp, sizeof ours.mtp)) << name;
  using C = BoardConfig::DisplayController;
  const C sdkController = BoardConfig::ACTIVE.displayController;
  const uint8_t expected = sdkController == C::UC8279 ? 2 : sdkController == C::UC8179 ? 1 : 0;
  EXPECT_EQ(ours.controller, expected) << name;
  if (expected != 0) EXPECT_EQ(ours.lut_ver, BoardConfig::ACTIVE.displayControllerVariant) << name;
  EXPECT_EQ(sdkPanel.mtpReads, ourPanel.mtpReads) << name;
  lastMtpReads = ourPanel.mtpReads;
  // Every line left an input.
  for (int pin : {SCLK, SDA, CS, DC, RST}) EXPECT_NE(ourPanel.modes[pin], OUTPUT) << name << " pin " << pin;
  return ours;
}

std::vector<uint8_t> mtp(uint8_t first, uint8_t rest) {
  std::vector<uint8_t> m(49, rest);
  m[1] = first;  // after the dummy byte
  return m;
}

TEST(EpdProbeTest, KnownPanelsMatchTheSdk) {
  // Every LUT_VER: only the stock UC8279 ids draw; the rest are taken as UC8179.
  for (int value = 0; value < 256; ++value) {
    const auto id = static_cast<uint8_t>(value);
    Panel p;
    p.replies[0x70] = {{0x00, 0x00, id, 0xFF, 0xFF}};
    p.replies[0x71] = {{0x13}};
    p.replies[0xA2] = {mtp(0xA5, 0x11)};
    const auto r = both(p, "lut " + std::to_string(id));
    EXPECT_EQ(r.verdict, 1);
    const bool uc8279 = id == 0x02 || id == 0x03 || id == 0x41 || id == 0x42 || id == 0x67 || id == 0x68 || id == 0x69;
    EXPECT_EQ(r.controller, uc8279 ? 2 : 1) << int(id);
  }
}

TEST(EpdProbeTest, SilentBusesStayOnTheSsd1677) {
  for (const int level : {-1, HIGH, LOW}) {
    Panel p;
    p.held = level;
    const auto r = both(p, "floating " + std::to_string(level));
    EXPECT_EQ(r.verdict, 0);
    EXPECT_EQ(r.controller, 0);
    EXPECT_FALSE(r.mtp_valid);
  }
}

TEST(EpdProbeTest, BlankVersionsNeedTheMtp) {
  // VER all 0xFF with a driven FLG: a programmed MTP confirms; a block that repeats confirms; a
  // uniform or changing one does not, and with both passes missing that is the default.
  struct Case {
    std::vector<std::vector<uint8_t>> mtp;
    uint8_t verdict;
  } cases[] = {
      {{mtp(0xA5, 0x00)}, 1},
      {{mtp(0x00, 0x00), mtp(0x00, 0x00)}, 0},
      {{[] {
         auto m = mtp(0x00, 0x00);
         m[27] = 0x68;
         return m;
       }()},
       1},
      {{[] {
          auto m = mtp(0x00, 0x00);
          m[27] = 0x68;
          return m;
        }(),
        [] {
          auto m = mtp(0x00, 0x00);
          m[27] = 0x69;
          return m;
        }()},
       0},
  };
  int n = 0;
  for (const auto& c : cases) {
    Panel p;
    p.replies[0x70] = {{0xFF, 0xFF, 0xFF, 0xFF, 0xFF}};
    p.replies[0x71] = {{0x13}};
    p.replies[0xA2] = c.mtp;
    const auto r = both(p, "blank " + std::to_string(n++));
    EXPECT_EQ(r.verdict, c.verdict);
  }
  // The fallback is for VER reading 0xFF only: a VER held at 0x00 is not rescued by the MTP.
  for (const auto& block : {mtp(0xA5, 0x00), [] {
                              auto m = mtp(0x00, 0x00);
                              m[27] = 0x68;
                              return m;
                            }()}) {
    Panel p;
    p.replies[0x70] = {{0x00, 0x00, 0x00, 0x00, 0x00}};
    p.replies[0x71] = {{0x13}};
    p.replies[0xA2] = {block};
    EXPECT_EQ(both(p, "zero ver " + std::to_string(n++)).verdict, 0);
  }
  // A repeat is the whole 48-byte block, not the dummy byte: two reads differing only there
  // confirm; two differing only in the last byte do not; nor does a block uniform but for it.
  const auto with = [](std::vector<uint8_t> m, size_t at, uint8_t v) {
    m[at] = v;
    return m;
  };
  struct Edge {
    std::vector<std::vector<uint8_t>> mtp;
    uint8_t verdict;
  } edges[] = {
      {{with(with(mtp(0, 0), 27, 0x68), 0, 0x11), with(with(mtp(0, 0), 27, 0x68), 0, 0x22)}, 1},
      {{with(with(mtp(0, 0), 27, 0x68), 48, 0x01), with(with(mtp(0, 0), 27, 0x68), 48, 0x02)}, 0},
      {{with(mtp(0, 0), 48, 0x01)}, 1},
      {{std::vector<uint8_t>(20, 0x5A)}, 1},  // a short reply: the rest reads 0xFF, repeatably
  };
  for (const auto& e : edges) {
    Panel p;
    p.replies[0x70] = {{0xFF, 0xFF, 0xFF, 0xFF, 0xFF}};
    p.replies[0x71] = {{0x13}};
    p.replies[0xA2] = e.mtp;
    EXPECT_EQ(both(p, "edge " + std::to_string(n++)).verdict, e.verdict);
  }
}

// FLG is taken from the first pass: whether the line counts as driven, and the reported value.
TEST(EpdProbeTest, StatusComesFromTheFirstPass) {
  {
    Panel p;
    p.replies[0x70] = {{0x00, 0x00, 0x68, 0xFF, 0xFF}};
    p.replies[0x71] = {{0x13}, {0x12}};
    const auto r = both(p, "flg lost");
    EXPECT_EQ(r.verdict, 2);
    EXPECT_EQ(r.flg, 0x13);
    EXPECT_EQ(lastMtpReads, 1);
  }
  {
    Panel p;
    p.replies[0x70] = {{0xFF, 0xFF, 0xFF, 0xFF, 0xFF}};
    p.replies[0x71] = {{0x12}, {0x13}};
    p.replies[0xA2] = {mtp(0xA5, 0x00)};
    const auto r = both(p, "flg late");
    EXPECT_EQ(r.verdict, 0);
    EXPECT_EQ(lastMtpReads, 0);
  }
}

TEST(EpdProbeTest, DisagreeingPassesAreInconclusive) {
  Panel p;
  p.replies[0x70] = {{0x00, 0x00, 0x68, 0xFF, 0xFF}, {0x00, 0x00, 0x02, 0xFF, 0xFF}};
  p.replies[0x71] = {{0x13}};
  const auto r = both(p, "disagree");
  EXPECT_EQ(r.verdict, 2);
  EXPECT_EQ(r.controller, 0);
  // A first pass that misses and a second that matches: inconclusive too.
  Panel q;
  q.replies[0x70] = {{0x00, 0x00, 0x00, 0x00, 0x00}, {0x00, 0x00, 0x68, 0xFF, 0xFF}};
  q.replies[0x71] = {{0x13}};
  EXPECT_EQ(both(q, "late").verdict, 2);
}

TEST(EpdProbeTest, RandomBusesMatchTheSdk) {
  std::mt19937 rng(42);
  int verdicts[3] = {};
  int mtpNotConfirming = 0, mtpRepeats = 0;
  for (int n = 0; n < 400; ++n) {
    Panel p;
    p.noisy = rng() % 4 == 0;
    p.rng.seed(rng());
    if (rng() % 8 == 0) p.held = rng() % 2 ? HIGH : LOW;
    const auto bytes = [&](size_t len) {
      std::vector<uint8_t> v(len);
      for (auto& b : v) b = rng() % 3 == 0 ? 0xFF : static_cast<uint8_t>(rng());
      return v;
    };
    if (rng() % 5) {
      p.replies[0x71] = {{static_cast<uint8_t>(rng() % 3 ? 0x13 : rng())}};
      if (rng() % 3 == 0) p.replies[0x71].push_back({static_cast<uint8_t>(rng() % 2 ? 0x13 : rng())});
    }
    if (rng() % 5) {
      p.replies[0x70] = {bytes(rng() % 6)};
      if (rng() % 3 == 0) p.replies[0x70].push_back(bytes(5));
      if (rng() % 3 == 0) p.replies[0x70][0] = {0xFF, 0xFF, 0xFF, 0xFF, 0xFF};
    }
    if (rng() % 2) {
      p.replies[0xA2] = {rng() % 3 == 0 ? mtp(0xA5, 0) : bytes(49)};
      if (rng() % 2) p.replies[0xA2].push_back(bytes(49));
    }
    const auto r = both(p, "random " + std::to_string(n));
    if (HasFailure()) return;
    ++verdicts[r.verdict % 3];
    if (lastMtpReads > 0 && r.verdict != 1) ++mtpNotConfirming;
    if (lastMtpReads == 2) ++mtpRepeats;
  }
  for (int v = 0; v < 3; ++v) EXPECT_GT(verdicts[v], 10) << "verdict " << v;
  EXPECT_GT(mtpNotConfirming, 5);
  EXPECT_GT(mtpRepeats, 5);
}

TEST(EpdProbeTest, MissingCallbacksTouchNothing) {
  Panel p;
  active = &p;
  const pw_epd_probe_lines lines = ourLines(p);
  pw_epd_probe_result r{};
  EXPECT_FALSE(pw_epd_probe(nullptr, &r));
  EXPECT_FALSE(pw_epd_probe(&lines, nullptr));
  for (int missing = 0; missing < 6; ++missing) {
    pw_epd_probe_lines l = lines;
    switch (missing) {
      case 0:
        l.mode = nullptr;
        break;
      case 1:
        l.write = nullptr;
        break;
      case 2:
        l.read = nullptr;
        break;
      case 3:
        l.release_reset_hold = nullptr;
        break;
      case 4:
        l.pause_us = nullptr;
        break;
      default:
        l.pause_ms = nullptr;
    }
    EXPECT_FALSE(pw_epd_probe(&l, &r)) << missing;
  }
  EXPECT_TRUE(p.events.empty());
}

}  // namespace
