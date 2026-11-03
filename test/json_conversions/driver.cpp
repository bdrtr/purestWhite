// Oracle for bembeyaz-jsondoc's conversions: ArduinoJson 7.4.2's is<T>(), as<T>() and
// `variant | fallback` for the types the settings stores read (lib/Serialization/PersistableStore.h),
// on the member "v" of each document. make_corpus.py builds it and keeps the transcript as the Rust
// tests' golden corpus (rust/crates/bembeyaz-jsondoc/testdata/conversions-corpus.txt).
//
// Each line of input is `v <hex of a JSON document>`; it prints
//   = err <name>    when the document is not read, else
//   = is <u8 u16 u32 int i64 bool float str, each 0|1>
//     or <u8|7> <u16|7> <u32|7> <int|-7> <float|7.5 bits> <bool|true> <bool|false> <str|"d" hex>
//     as <u8> <u16> <u32> <int> <float bits>
//     array <size> <[0]|9 as u16> <[5]|9 as u16> object <size>
#define ARDUINOJSON_ENABLE_ARDUINO_STRING 0
#include <ArduinoJson.h>

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <string>

namespace {
std::string text(const std::string& hex) {
  std::string out;
  if (hex == "-") return out;
  for (size_t i = 0; i + 1 < hex.size(); i += 2)
    out.push_back(static_cast<char>(std::stoi(hex.substr(i, 2), nullptr, 16)));
  return out;
}

std::string hexOf(const char* s) {
  std::string out;
  char b[3];
  for (const char* p = s; *p; ++p) {
    std::snprintf(b, sizeof(b), "%02x", static_cast<unsigned char>(*p));
    out += b;
  }
  return out.empty() ? "-" : out;
}

uint32_t bitsOf(const float f) {
  uint32_t b;
  std::memcpy(&b, &f, sizeof(b));
  return b;
}

void step(const std::string& hex) {
  JsonDocument doc;
  const std::string input = text(hex);
  const DeserializationError error = deserializeJson(doc, input.c_str());
  if (error) {
    std::printf("  = err %s\n", error.c_str());
    return;
  }
  const JsonVariantConst v = doc["v"];
  std::printf("  = is %d %d %d %d %d %d %d %d", v.is<uint8_t>(), v.is<uint16_t>(), v.is<uint32_t>(), v.is<int>(),
              v.is<long long>(), v.is<bool>(), v.is<float>(), v.is<const char*>());
  std::printf(" or %u %u %lu %d %08lx %d %d %s", static_cast<unsigned>(v | static_cast<uint8_t>(7)),
              static_cast<unsigned>(v | static_cast<uint16_t>(7)),
              static_cast<unsigned long>(v | static_cast<uint32_t>(7)), v | -7,
              static_cast<unsigned long>(bitsOf(v | 7.5f)), v | true, v | false, hexOf(v | "d").c_str());
  std::printf(" as %u %u %lu %d %08lx", static_cast<unsigned>(v.as<uint8_t>()), static_cast<unsigned>(v.as<uint16_t>()),
              static_cast<unsigned long>(v.as<uint32_t>()), v.as<int>(),
              static_cast<unsigned long>(bitsOf(v.as<float>())));
  const JsonArrayConst array = v.as<JsonArrayConst>();
  std::printf(" array %u %u %u object %u\n", static_cast<unsigned>(array.size()),
              static_cast<unsigned>(array[0] | static_cast<uint16_t>(9)),
              static_cast<unsigned>(array[5] | static_cast<uint16_t>(9)),
              static_cast<unsigned>(v.as<JsonObjectConst>().size()));
}
}  // namespace

int main() {
  std::string line;
  while (std::getline(std::cin, line)) {
    if (line.rfind("v ", 0) != 0) continue;
    std::printf("%s\n", line.c_str());
    step(line.substr(2));
  }
  return 0;
}
