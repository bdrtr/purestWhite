#pragma once
#include "FreeRTOS.h"
// The UC81xx path never waits on the refresh semaphore; no semaphore exists here.
inline SemaphoreHandle_t xSemaphoreCreateBinary() { return nullptr; }
inline int xSemaphoreTake(SemaphoreHandle_t, int) { return pdFALSE; }
inline int xSemaphoreGiveFromISR(SemaphoreHandle_t, BaseType_t*) { return pdFALSE; }
