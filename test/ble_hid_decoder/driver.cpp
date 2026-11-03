// Oracle for purestwhite-ble's HID report decoder: runs one scenario through the SDK's real
// decoder (freeink-sdk/libs/network/BleKeyboardHost/src/BleKeyboardHost.cpp, against the SDK's
// fake NimBLE stack in test/host) and prints every key and raw button event each step gives.
// make_corpus.py builds it, feeds it scenarios and keeps the transcripts as the Rust tests'
// golden corpus (rust/crates/purestwhite-ble/testdata/hid-corpus.txt).
//
// Input, one line each:
//   map <hex>|map -      the Report Map the remote serves (none with -)
//   report <id>          an Input Report characteristic whose Report Reference id is <id> (0: none)
//   frame <dt> <i> <hex> after <dt> ms, report characteristic <i> notifies <hex>
//   poll <dt>            after <dt> ms, the host's poll()
//   drain                the events queued so far are taken (the queues fill, and drop, between)
// Output: the input lines echoed (frame and poll with the time since the link came up instead of
// <dt>); `connected <0|1>` before the first frame or poll, and `end` last; each drain followed by
// its events, indented (<at> also in ms since the link came up):
//   key <keycode> <mods> <special> <ch>
//   raw <code hex> <pressed> <keycode> <mods> <wasRest> <at>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include "fake_ble.h"

namespace {

constexpr const char* kRemote = "AA:BB:CC:DD:EE:01";

std::vector<uint8_t> hex(const char* text) {
  std::vector<uint8_t> out;
  for (size_t i = 0; text[i] && text[i + 1]; i += 2) {
    char byte[3] = {text[i], text[i + 1], 0};
    out.push_back(static_cast<uint8_t>(std::strtoul(byte, nullptr, 16)));
  }
  return out;
}

void drain(unsigned long base) {
  freeink::KeyEvent key;
  while (fakeble::host().popKey(key)) {
    std::printf("  key %u %u %u %u\n", key.keycode, key.mods, static_cast<unsigned>(key.special),
                static_cast<unsigned>(static_cast<uint8_t>(key.ch)));
  }
  freeink::RawButtonEvent raw;
  while (fakeble::host().popRawButton(raw)) {
    std::printf("  raw %06x %u %u %u %u %lu\n", static_cast<unsigned>(raw.code()), raw.pressed ? 1 : 0,
                raw.keycode, raw.mods, raw.wasRest ? 1 : 0, static_cast<unsigned long>(raw.atMs) - base);
  }
}

}  // namespace

int main() {
  fakeble::resetWorld();
  std::vector<int> reports;
  char line[1024];
  bool connected = false;
  unsigned long base = 0;
  while (std::fgets(line, sizeof line, stdin)) {
    line[std::strcspn(line, "\r\n")] = 0;
    char word[16] = {0};
    std::sscanf(line, "%15s", word);
    if (std::strcmp(word, "map") == 0) {
      std::printf("%s\n", line);
      const char* body = line + 4;
      if (std::strcmp(body, "-") != 0) {
        const std::vector<uint8_t> map = hex(body);
        const int index = fakeble::addCharacteristic(0x2A4B, true, false, false);
        fakeble::setValue(index, map.data(), map.size());
      }
    } else if (std::strcmp(word, "report") == 0) {
      std::printf("%s\n", line);
      reports.push_back(fakeble::addInputReport(static_cast<uint8_t>(std::atoi(line + 7))));
    } else if (std::strcmp(word, "frame") == 0 || std::strcmp(word, "poll") == 0) {
      if (!connected) {
        connected = fakeble::beginHost() && fakeble::connectTo(kRemote) && fakeble::waitForWorkerIdle();
        std::printf("connected %d\n", connected ? 1 : 0);
        if (!connected) return 1;
        base = fakeble::clockMs();
      }
      unsigned dt = 0;
      if (std::strcmp(word, "frame") == 0) {
        unsigned index = 0;
        char body[900] = {0};
        std::sscanf(line, "frame %u %u %899s", &dt, &index, body);
        fakeble::advanceMillis(dt);
        const std::vector<uint8_t> data = hex(body);
        std::printf("frame %lu %u %s\n", fakeble::clockMs() - base, index, body);
        // A frame the fake does not deliver would be one the Rust replay still reads: stop.
        if (!fakeble::notify(reports.at(index), data.data(), data.size())) return 2;
      } else {
        std::sscanf(line, "poll %u", &dt);
        fakeble::advanceMillis(dt);
        std::printf("poll %lu\n", fakeble::clockMs() - base);
        fakeble::host().poll();
      }
    } else if (std::strcmp(word, "drain") == 0) {
      std::printf("drain\n");
      drain(base);
    }
  }
  std::printf("end\n");
  return 0;
}
