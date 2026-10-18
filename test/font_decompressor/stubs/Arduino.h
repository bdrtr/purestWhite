#pragma once

#include <cstdint>
#include <cstring>  // the real Arduino.h brings in string.h

inline uint32_t millis() { return 0; }
inline uint32_t micros() { return 0; }
