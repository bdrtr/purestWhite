// The model UC81xx controller that test/epd_panels runs freeink-sdk's drivers and ours against: its
// chip-select frames, reset pulses, BUSY reads and pauses, and the C ABI bus that drives it.
#pragma once

#include <Arduino.h>
#include <SPI.h>
#include <gtest/gtest.h>
#include <purestwhite/epd.h>

#include <random>
#include <string>
#include <utility>
#include <vector>

namespace epd_model {

constexpr int CS = 13, DC = 18, RST = 14, BUSY = 6;
constexpr size_t FRAME_BYTES = 100 * 480;

struct Event {
  enum Kind { Frame, Reset, BusyRead, Pause } kind;
  int value;
  std::vector<std::pair<bool, uint8_t>> bytes;
  bool operator==(const Event&) const = default;
};

inline std::string describe(const Event& e) {
  static const char* names[] = {"frame", "reset", "busy", "pause"};
  std::string s = std::string(names[e.kind]) + " " + std::to_string(e.value);
  for (size_t i = 0; i < e.bytes.size() && i < 4; ++i)
    s += std::string(" ") + (e.bytes[i].first ? "d" : "c") + std::to_string(e.bytes[i].second);
  if (e.bytes.size() > 4) s += " ... (" + std::to_string(e.bytes.size()) + " bytes)";
  return s;
}

// A UC81xx as its lines see it. BUSY (active low) drops after power-on, power-off and a
// refresh: a refresh may take a few milliseconds to start. All randomness is the panel's own.
// Writes take no model time, so the busy check catches a write issued while BUSY reads low;
// a missing wait also shows as missing BUSY reads and pauses in the event comparison. It cannot
// show a refresh that starts after the 50 ms start wait: both sides then write on, as the SDK
// does, before the model's busy window opens.
struct Panel {
  std::mt19937 rng;
  bool csLow = false;
  bool dc = true;
  std::vector<std::pair<bool, uint8_t>> frame;
  unsigned long busyFrom = 0, busyUntil = 0;
  bool wroteWhileBusy = false;
  bool failBursts = false;  // the bridge's write fails for more than one byte: a plane
  int lineTouches = 0;      // every pin write or read, SPI write and pause
  int lateStarts = 0;       // refreshes that took longer than the 50 ms start wait
  std::vector<Event> events;

  explicit Panel(uint32_t seed) : rng(seed) {}
  bool busy() const { return hostMillis >= busyFrom && hostMillis < busyUntil; }
};

inline Panel* active = nullptr;

inline void spiWrite(const uint8_t* bytes, size_t len) {
  Panel& p = *active;
  ++p.lineTouches;
  if (p.busy()) p.wroteWhileBusy = true;
  for (size_t i = 0; i < len; ++i) p.frame.emplace_back(p.dc, bytes[i]);
}

inline void hook() {
  onSpiWrite = spiWrite;
  onDigitalWrite = [](int pin, int level) {
    Panel& p = *active;
    ++p.lineTouches;
    if (pin == DC) p.dc = level == HIGH;
    if (pin == RST) {
      p.events.push_back({Event::Reset, level, {}});
      // A reset ends whatever the controller was doing.
      if (level == LOW) p.busyUntil = 0;
    }
    if (pin == CS) {
      if (level == LOW && !p.csLow) p.frame.clear();
      if (level == HIGH && p.csLow) {
        // The controller acts on a lone command byte as the frame ends.
        if (p.frame.size() == 1 && !p.frame[0].first) {
          const uint8_t c = p.frame[0].second;
          if (c == 0x02 || c == 0x04 || c == 0x12) {
            const unsigned long start = c == 0x12 ? p.rng() % 70 : 0;
            if (start > 50) ++p.lateStarts;
            p.busyFrom = hostMillis + start;
            p.busyUntil = p.busyFrom + 1 + p.rng() % 40;
          }
        }
        p.events.push_back({Event::Frame, 0, p.frame});
      }
      p.csLow = level == LOW;
    }
  };
  onDigitalRead = [](int pin) {
    Panel& p = *active;
    ++p.lineTouches;
    const int level = pin == BUSY && p.busy() ? LOW : HIGH;
    if (pin == BUSY) p.events.push_back({Event::BusyRead, level, {}});
    return level;
  };
  onDelay = [](unsigned long ms) {
    ++active->lineTouches;
    active->events.push_back({Event::Pause, static_cast<int>(ms), {}});
  };
}

// Each callback checks it was handed the panel it belongs to.
inline Panel& from(void* context) {
  EXPECT_EQ(context, active);
  return *static_cast<Panel*>(context);
}

inline pw_epd_bus ourBus(Panel& panel) {
  pw_epd_bus bus{};
  bus.context = &panel;
  bus.write = [](void* c, const uint8_t* bytes, size_t len) {
    if (from(c).failBursts && len > 1) return false;
    spiWrite(bytes, len);
    return true;
  };
  bus.set_cs = [](void* c, bool high) {
    from(c);
    digitalWrite(CS, high ? HIGH : LOW);
  };
  bus.set_dc = [](void* c, bool high) {
    from(c);
    digitalWrite(DC, high ? HIGH : LOW);
  };
  bus.set_rst = [](void* c, bool high) {
    from(c);
    digitalWrite(RST, high ? HIGH : LOW);
  };
  bus.busy_high = [](void* c) {
    from(c);
    return digitalRead(BUSY) == HIGH;
  };
  bus.pause_ms = [](void* c, uint32_t ms) {
    from(c);
    delay(ms);
  };
  return bus;
}

// Both sides' events for one step must match, and neither may write while busy.
inline void same(Panel& sdk, Panel& ours, const std::string& step) {
  ASSERT_FALSE(sdk.wroteWhileBusy) << step;
  ASSERT_FALSE(ours.wroteWhileBusy) << step;
  ASSERT_EQ(sdk.events.size(), ours.events.size()) << step;
  for (size_t i = 0; i < sdk.events.size(); ++i) {
    ASSERT_EQ(sdk.events[i], ours.events[i])
        << step << " event " << i << ": " << describe(sdk.events[i]) << " / " << describe(ours.events[i]);
  }
  sdk.events.clear();
  ours.events.clear();
}

}  // namespace epd_model
