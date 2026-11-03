// Oracle for purestwhite-ble's button bindings: runs one scenario through the app's binding rules
// (lib/BlePageTurner/include/BleKeyBinding.h with the SDK's BleRemoteProfiles.h, pageActionFor
// from BlePageTurner.h, and the saved tables' load in BlePageTurnerJson.h's readJson) and prints what each step gives. make_corpus.py builds it,
// feeds it scenarios and keeps the transcripts as the Rust tests' golden corpus
// (rust/crates/purestwhite-ble/testdata/binding-corpus.txt).
//
// State: up to kMaxRemotes saved tables, the table picked for the link (none at first) and its
// HoldWait. A step that changes the saved tables forgets the pick and the wait, as the runtime's
// Router is reset when the settings screen may have changed them.
//
// Input, one line each; <addr> and <name> are hex bytes, or - for the empty string:
//   load <n> {<addr> <slots>}*       readJson of n saved tables, <slots> hex joined by commas
//                                    (- for none), replacing the tables = <count>
//   edit <addr> <name>               editableTable          = <index or -1> <count>
//   forget <addr>                    forgetRemote           = <0|1> <count>
//   learn <i> <action> <code> <hold> learn on table i       = <0|1>
//   clear <i> <action>               clearAction on table i = <0|1>
//   find <i> <action>                find on table i        = <0|1> <binding>
//   dump <i>                         table i                = <addr hex> <count> <bindings...>
//   pick <addr> <name>               tableFor, kept         = saved <i> | default <j> | none, <routes>
//   lookup <code> <hold>             lookup on the pick     = <action>
//   edge <code> <pressed> <at> <legacy>  onRawEdge          = <action>
//   hold <now>                       pollHold               = <action>
//   valid <bits>                     valid                  = <0|1>
//   format <bits>                    formatCode             = <text>
//   name <action>                    actionName             = <text>
//   page <enabled> <prev> <next> <usage> <mods>  pageActionFor = <action>
// Codes, bindings, bits, and page's usages and mods are hex; the rest decimal. Each line is echoed, its answer after it.

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>

#include "BlePageTurnerJson.h"

namespace {

using namespace bleturner;

RemoteTable tables[kMaxRemotes] = {};
uint8_t count = 0;
const RemoteTable* pick = nullptr;
HoldWait wait;

std::string text(const char* hex) {
  std::string out;
  if (std::strcmp(hex, "-") == 0) return out;
  for (size_t i = 0; hex[i] && hex[i + 1]; i += 2) {
    const char byte[3] = {hex[i], hex[i + 1], 0};
    out.push_back(static_cast<char>(std::strtoul(byte, nullptr, 16)));
  }
  return out;
}

unsigned long num(const char* s, int base) { return std::strtoul(s, nullptr, base); }

void changed() {
  pick = nullptr;
  wait = HoldWait{};
}

RemoteTable* table(const char* index) {
  const unsigned long i = num(index, 10);
  return i < count ? &tables[i] : nullptr;
}

void pickTable(const std::string& addr, const std::string& name) {
  pick = tableFor(tables, count, addr.c_str(), name.c_str());
  wait = HoldWait{};
  if (pick == nullptr) {
    std::printf("  = none 0\n");
  } else if (pick >= tables && pick < tables + kMaxRemotes) {
    std::printf("  = saved %d %d\n", static_cast<int>(pick - tables), routes(pick) ? 1 : 0);
  } else {
    std::printf("  = default %d %d\n", freeink::remoteButtonProfileIndex(name.c_str()), routes(pick) ? 1 : 0);
  }
}

// The saved tables as settings.json holds them, read back by the firmware's own loader.
void load(char** a) {
  JsonDocument doc;
  JsonArray remotes = doc["bleRemotes"].to<JsonArray>();
  const unsigned long n = num(a[0], 10);
  for (unsigned long i = 0; i < n; ++i) {
    JsonObject r = remotes.add<JsonObject>();
    r["addr"] = text(a[1 + 2 * i]);
    JsonArray binds = r["binds"].to<JsonArray>();
    std::string slots = a[2 + 2 * i];
    for (char* s = std::strtok(slots.data(), ","); s && std::strcmp(s, "-") != 0; s = std::strtok(nullptr, ",")) {
      binds.add(static_cast<uint32_t>(num(s, 16)));
    }
  }
  Config config;
  readJson(config, doc);
  count = config.remoteCount;
  for (uint8_t i = 0; i < kMaxRemotes; ++i) tables[i] = config.remotes[i];
  std::printf("  = %u\n", count);
  changed();
}

void dump(const RemoteTable& t) {
  std::printf("  = ");
  for (size_t i = 0; i < sizeof(t.addr) && t.addr[i]; ++i) std::printf("%02x", static_cast<uint8_t>(t.addr[i]));
  std::printf("%s %u", t.addr[0] ? "" : "-", t.count);
  for (uint8_t i = 0; i < t.count; ++i) std::printf(" %08x", t.bindings[i]);
  std::printf("\n");
}

Action action(const char* s) { return static_cast<Action>(num(s, 10)); }

// The steps on one table; false when `cmd` is none of them.
bool tableStep(const char* cmd, char** a) {
  RemoteTable* t = table(a[0]);
  if (t == nullptr) {
    std::printf("  = no table\n");
    return true;
  }
  if (std::strcmp(cmd, "learn") == 0) {
    std::printf("  = %d\n", learn(*t, action(a[1]), num(a[2], 16), num(a[3], 10) != 0) ? 1 : 0);
    changed();
  } else if (std::strcmp(cmd, "clear") == 0) {
    std::printf("  = %d\n", clearAction(*t, action(a[1])) ? 1 : 0);
    changed();
  } else if (std::strcmp(cmd, "find") == 0) {
    Binding b = 0;
    const bool found = find(*t, action(a[1]), b);
    std::printf("  = %d %08x\n", found ? 1 : 0, found ? b : 0u);
  } else if (std::strcmp(cmd, "dump") == 0) {
    dump(*t);
  } else {
    return false;
  }
  return true;
}

void step(const char* cmd, char** a) {
  if (std::strcmp(cmd, "load") == 0) {
    load(a);
  } else if (std::strcmp(cmd, "edit") == 0) {
    const RemoteTable* t = editableTable(tables, count, text(a[0]).c_str(), text(a[1]).c_str());
    std::printf("  = %d %u\n", t == nullptr ? -1 : static_cast<int>(t - tables), count);
    changed();
  } else if (std::strcmp(cmd, "forget") == 0) {
    const bool gone = forgetRemote(tables, count, text(a[0]).c_str());
    std::printf("  = %d %u\n", gone ? 1 : 0, count);
    changed();
  } else if (std::strcmp(cmd, "pick") == 0) {
    pickTable(text(a[0]), text(a[1]));
  } else if (std::strcmp(cmd, "lookup") == 0) {
    std::printf("  = %u\n", pick ? static_cast<unsigned>(lookup(*pick, num(a[0], 16), num(a[1], 10) != 0)) : 0u);
  } else if (std::strcmp(cmd, "edge") == 0) {
    const Action got = pick ? onRawEdge(*pick, num(a[0], 16), num(a[1], 10) != 0,
                                        static_cast<uint32_t>(num(a[2], 10)), action(a[3]), wait)
                            : Action::None;
    std::printf("  = %u\n", static_cast<unsigned>(got));
  } else if (std::strcmp(cmd, "hold") == 0) {
    std::printf("  = %u\n", static_cast<unsigned>(pollHold(wait, static_cast<uint32_t>(num(a[0], 10)))));
  } else if (std::strcmp(cmd, "valid") == 0) {
    std::printf("  = %d\n", valid(static_cast<Binding>(num(a[0], 16))) ? 1 : 0);
  } else if (std::strcmp(cmd, "format") == 0) {
    char out[16];
    formatCode(out, sizeof out, static_cast<Binding>(num(a[0], 16)));
    std::printf("  = %s\n", out);
  } else if (std::strcmp(cmd, "name") == 0) {
    std::printf("  = %s\n", actionName(action(a[0])));
  } else if (std::strcmp(cmd, "page") == 0) {
    Config c;
    c.enabled = static_cast<uint8_t>(num(a[0], 10));
    c.prevKeyUsage = static_cast<uint8_t>(num(a[1], 16));
    c.nextKeyUsage = static_cast<uint8_t>(num(a[2], 16));
    const Action got = pageActionFor(c, static_cast<uint8_t>(num(a[3], 16)), static_cast<uint8_t>(num(a[4], 16)));
    std::printf("  = %u\n", static_cast<unsigned>(got));
  } else if (!tableStep(cmd, a)) {
    std::printf("  = ?\n");
  }
}

}  // namespace

int main() {
  char line[2048];
  while (std::fgets(line, sizeof line, stdin)) {
    line[std::strcspn(line, "\n")] = 0;
    std::printf("%s\n", line);
    char* words[16] = {};
    int n = 0;
    for (char* w = std::strtok(line, " "); w && n < 16; w = std::strtok(nullptr, " ")) words[n++] = w;
    static char empty[] = "0";
    for (int i = n; i < 16; ++i) words[i] = empty;
    if (n > 0) step(words[0], words + 1);
  }
  return 0;
}
