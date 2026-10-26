#pragma once
using BaseType_t = int;
using SemaphoreHandle_t = void*;
constexpr int pdFALSE = 0, pdTRUE = 1;
#define pdMS_TO_TICKS(x) (x)
#define portYIELD_FROM_ISR()
