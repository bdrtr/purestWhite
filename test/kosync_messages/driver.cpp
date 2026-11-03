// Oracle for the Rust port of lib/KOReaderSync/KOReaderSyncMessages and of the ArduinoJson it
// uses: runs one scenario through the C++ functions with ArduinoJson 7.4.2 (the firmware's) and
// prints each answer. make_corpus.py builds it, feeds it scenarios and keeps the transcripts as the
// Rust tests' golden corpus (rust/crates/bembeyaz-jsondoc/testdata/kosync-corpus.txt).
//
// Input, one line each; text is hex bytes (- for none), a float is its bits in hex:
//   json <text>                 deserializeJson, then serializeJson   = ok <text> | err <name>
//   user <name> <md5>           kosync::createUserBody                = <text>
//   body <ext> <rich> <document> <progress> <percentage> <meta> <pos>
//        meta: - | <filename>,<title>,<authors>,<extra>,<isbn>,<asin>,<series>,<seriesIndex|->
//        pos:  - | <pctQ>,<spine>,<page>,<pages>,<para|->,<xpath>
//                               kosync::progressBody                  = <text>
//   parse <rich> <hash> <text>  kosync::parseProgress                 = err <name> |
//        ok <document> <progress> <percentage> <device> <deviceId> <timestamp> <pos>
//        pos: - | <pctQ>,<spine>,<page>,<pages>,<para|->,<xpath>
// Each line is echoed, its answer after it.

#include <ArduinoJson.h>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include "KOReaderSyncMessages.h"

namespace {

std::string text(const std::string& hex) {
  std::string out;
  if (hex == "-") return out;
  for (size_t i = 0; i + 1 < hex.size(); i += 2) {
    out.push_back(static_cast<char>(std::strtoul(hex.substr(i, 2).c_str(), nullptr, 16)));
  }
  return out;
}

std::string hexOf(const std::string& s) {
  if (s.empty()) return "-";
  std::string out;
  char b[3];
  for (const unsigned char c : s) {
    std::snprintf(b, sizeof b, "%02x", c);
    out += b;
  }
  return out;
}

float floatOf(const std::string& hex) {
  const uint32_t bits = static_cast<uint32_t>(std::strtoul(hex.c_str(), nullptr, 16));
  float f;
  std::memcpy(&f, &bits, 4);
  return f;
}

std::string bitsOf(const float f) {
  uint32_t bits;
  std::memcpy(&bits, &f, 4);
  char b[9];
  std::snprintf(b, sizeof b, "%08x", bits);
  return b;
}

std::vector<std::string> split(const std::string& s, const char sep) {
  std::vector<std::string> out;
  size_t at = 0;
  for (;;) {
    const size_t end = s.find(sep, at);
    out.push_back(s.substr(at, end == std::string::npos ? std::string::npos : end - at));
    if (end == std::string::npos) return out;
    at = end + 1;
  }
}

std::string posOf(const KOReaderRichPosition& p) {
  return std::to_string(p.pctQ) + "," + std::to_string(p.spineIndex) + "," + std::to_string(p.pageNumber) + "," +
         std::to_string(p.totalPages) + "," + (p.paragraphIndex ? std::to_string(*p.paragraphIndex) : "-") + "," +
         hexOf(p.xpath);
}

void json(const std::vector<std::string>& a) {
  JsonDocument doc;
  const std::string input = text(a.at(1));
  const DeserializationError error = deserializeJson(doc, input.c_str());
  if (error) {
    std::printf("  = err %s\n", error.c_str());
    return;
  }
  std::string out;
  serializeJson(doc, out);
  std::printf("  = ok %s\n", hexOf(out).c_str());
}

KOReaderProgress progressOf(const std::vector<std::string>& a) {
  KOReaderProgress p{};
  p.document = text(a.at(3));
  p.progress = text(a.at(4));
  p.percentage = floatOf(a.at(5));
  if (a.at(6) != "-") {
    const auto m = split(a.at(6), ',');
    KOReaderMetadata meta;
    meta.filename = text(m.at(0));
    meta.title = text(m.at(1));
    meta.authors = text(m.at(2));
    meta.extraJson = text(m.at(3));
    meta.isbn = text(m.at(4));
    meta.asin = text(m.at(5));
    meta.series = text(m.at(6));
    if (m.at(7) != "-") meta.seriesIndex = floatOf(m.at(7));
    p.metadata = meta;
  }
  if (a.at(7) != "-") {
    const auto q = split(a.at(7), ',');
    KOReaderRichPosition r;
    r.pctQ = static_cast<uint32_t>(std::strtoul(q.at(0).c_str(), nullptr, 10));
    r.spineIndex = static_cast<uint16_t>(std::strtoul(q.at(1).c_str(), nullptr, 10));
    r.pageNumber = static_cast<uint16_t>(std::strtoul(q.at(2).c_str(), nullptr, 10));
    r.totalPages = static_cast<uint16_t>(std::strtoul(q.at(3).c_str(), nullptr, 10));
    if (q.at(4) != "-") r.paragraphIndex = static_cast<uint16_t>(std::strtoul(q.at(4).c_str(), nullptr, 10));
    r.xpath = text(q.at(5));
    p.position = r;
  }
  return p;
}

void parse(const std::vector<std::string>& a) {
  KOReaderProgress out{};
  const std::string body = text(a.at(3));
  const char* error = kosync::parseProgress(body.c_str(), text(a.at(2)), a.at(1) == "1", out);
  if (error) {
    std::printf("  = err %s\n", error);
    return;
  }
  std::printf("  = ok %s %s %s %s %s %lld %s\n", hexOf(out.document).c_str(), hexOf(out.progress).c_str(),
              bitsOf(out.percentage).c_str(), hexOf(out.device).c_str(), hexOf(out.deviceId).c_str(),
              static_cast<long long>(out.timestamp), out.position ? posOf(*out.position).c_str() : "-");
}

}  // namespace

int main() {
  std::string line;
  char buf[1 << 16];
  while (std::fgets(buf, sizeof buf, stdin)) {
    line = buf;
    if (!line.empty() && line.back() == '\n') line.pop_back();
    std::printf("%s\n", line.c_str());
    const auto a = split(line, ' ');
    if (a.at(0) == "json") {
      json(a);
    } else if (a.at(0) == "user") {
      std::printf("  = %s\n", hexOf(kosync::createUserBody(text(a.at(1)), text(a.at(2)))).c_str());
    } else if (a.at(0) == "body") {
      const std::string body = kosync::progressBody(progressOf(a), a.at(1) == "1", a.at(2) == "1");
      std::printf("  = %s\n", hexOf(body).c_str());
    } else if (a.at(0) == "parse") {
      parse(a);
    } else {
      std::printf("  = ?\n");
    }
  }
  return 0;
}
