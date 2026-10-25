#pragma once

// The shared fixture's Arduino.h, plus what ImageToFramebufferDecoder.cpp needs.
#include_next <Arduino.h>
#include <HalStorage.h>

inline void vTaskDelay(int) {}
