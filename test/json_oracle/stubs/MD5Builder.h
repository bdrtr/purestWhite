// Host stub: the KOReader store's MD5 of its password (never asked for by a driver).
#pragma once

#include <Arduino.h>

class MD5Builder {
 public:
  void begin() {}
  void add(const char*) {}
  void calculate() {}
  String toString() const { return String(""); }
};
