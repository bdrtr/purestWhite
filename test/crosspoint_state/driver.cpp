// Oracle for the Rust port of src/CrossPointState: runs one scenario through the store's
// fromJson/toJson (ArduinoJson 7.4.2, the firmware's) and its recent-image ring, and prints each
// answer. make_corpus.py builds it and keeps the transcript as the Rust tests' golden corpus
// (rust/crates/bembeyaz-store/testdata/state-corpus.txt).
//
//   from <json> <idx> <count> <push>   deserializeJson then fromJson; = err <name> |
//                                      = <state> | <toJson> | <recent> <overlay recent> | <state>
//                                      where recent is isRecent*(idx, count) before push, and the
//                                      last state is after pushRecent*(push)
//   to <state>                         the state set directly, then toJson  = <toJson>
//
// <state> is: path imgs pos fill overlay_imgs overlay_pos overlay_fill load last_sleep boot,
// path in hex ("-" when empty), the image lists comma-separated; <toJson> is serializeJson's text
// (one line: it escapes control characters).
#define ARDUINOJSON_ENABLE_ARDUINO_STRING 0
#include <ArduinoJson.h>

#include <cstdio>
#include <cstdlib>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "CrossPointState.h"

namespace {
std::string text(const std::string& hex) {
  std::string out;
  if (hex == "-") return out;
  for (size_t i = 0; i + 1 < hex.size(); i += 2)
    out.push_back(static_cast<char>(std::stoi(hex.substr(i, 2), nullptr, 16)));
  return out;
}

std::string hexOf(const std::string& s) {
  std::string out;
  char b[3];
  for (const unsigned char c : s) {
    std::snprintf(b, sizeof(b), "%02x", c);
    out += b;
  }
  return out.empty() ? "-" : out;
}

std::string list(const uint16_t* v) {
  std::string out;
  for (int i = 0; i < CrossPointState::SLEEP_RECENT_COUNT; i++) out += (i ? "," : "") + std::to_string(v[i]);
  return out;
}

void parseList(const std::string& csv, uint16_t* v) {
  std::stringstream in(csv);
  std::string item;
  for (int i = 0; i < CrossPointState::SLEEP_RECENT_COUNT && std::getline(in, item, ','); i++) {
    v[i] = static_cast<uint16_t>(std::stoul(item));
  }
}

std::string stateOf(const CrossPointState& s) {
  std::ostringstream out;
  out << hexOf(s.openEpubPath) << ' ' << list(s.recentSleepImages) << ' ' << +s.recentSleepPos << ' '
      << +s.recentSleepFill << ' ' << list(s.recentOverlaySleepImages) << ' ' << +s.recentOverlaySleepPos << ' '
      << +s.recentOverlaySleepFill << ' ' << +s.readerActivityLoadCount << ' ' << s.lastSleepFromReader << ' '
      << s.showBootScreen;
  return out.str();
}

std::string jsonOf(const CrossPointState& s) {
  JsonDocument doc;
  s.toJson(doc);
  std::string out;
  serializeJson(doc, out);
  return out;
}

void from(const std::vector<std::string>& a, CrossPointState& s) {
  JsonDocument doc;
  const std::string input = text(a.at(1));
  const DeserializationError error = deserializeJson(doc, input.c_str());
  if (error) {
    std::printf("  = err %s\n", error.c_str());
    return;
  }
  s.fromJson(doc.as<JsonVariantConst>());
  const auto idx = static_cast<uint16_t>(std::stoul(a.at(2)));
  const auto count = static_cast<uint8_t>(std::stoul(a.at(3)));
  const auto push = static_cast<uint16_t>(std::stoul(a.at(4)));
  const std::string before = stateOf(s);
  const std::string json = jsonOf(s);
  const bool recent = s.isRecentSleep(idx, count);
  const bool overlay = s.isRecentOverlaySleep(idx, count);
  s.pushRecentSleep(push);
  s.pushRecentOverlaySleep(push);
  std::printf("  = %s | %s | %d %d | %s\n", before.c_str(), json.c_str(), recent, overlay, stateOf(s).c_str());
}

void to(const std::vector<std::string>& a, CrossPointState& s) {
  s.openEpubPath = text(a.at(1));
  parseList(a.at(2), s.recentSleepImages);
  s.recentSleepPos = static_cast<uint8_t>(std::stoul(a.at(3)));
  s.recentSleepFill = static_cast<uint8_t>(std::stoul(a.at(4)));
  parseList(a.at(5), s.recentOverlaySleepImages);
  s.recentOverlaySleepPos = static_cast<uint8_t>(std::stoul(a.at(6)));
  s.recentOverlaySleepFill = static_cast<uint8_t>(std::stoul(a.at(7)));
  s.readerActivityLoadCount = static_cast<uint8_t>(std::stoul(a.at(8)));
  s.lastSleepFromReader = a.at(9) == "1";
  s.showBootScreen = a.at(10) == "1";
  std::printf("  = %s\n", jsonOf(s).c_str());
}
}  // namespace

int main() {
  CrossPointState& state = APP_STATE;
  std::string line;
  while (std::getline(std::cin, line)) {
    std::vector<std::string> a;
    std::stringstream in(line);
    for (std::string w; in >> w;) a.push_back(w);
    if (a.empty()) continue;
    std::printf("%s\n", line.c_str());
    if (a[0] == "from") {
      from(a, state);
    } else if (a[0] == "to") {
      to(a, state);
    }
  }
  return 0;
}
