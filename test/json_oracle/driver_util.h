// What the oracle drivers (test/*/driver.cpp) share: the hex their steps and answers use.
#pragma once

#include <cstdio>
#include <string>

// hex's bytes ("-" is none).
inline std::string bytesOf(const std::string& hex) {
  std::string out;
  if (hex == "-") return out;
  for (size_t i = 0; i + 1 < hex.size(); i += 2) {
    out.push_back(static_cast<char>(std::stoi(hex.substr(i, 2), nullptr, 16)));
  }
  return out;
}

// s in hex ("-" for none).
inline std::string hexOf(const std::string& s) {
  std::string out;
  char b[3];
  for (const unsigned char c : s) {
    std::snprintf(b, sizeof(b), "%02x", c);
    out += b;
  }
  return out.empty() ? "-" : out;
}
