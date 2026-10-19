#pragma once

#include <cstdint>

// Hyphens and dashes a word may already break at (and the underscore and ellipsis).
bool isExplicitHyphen(uint32_t cp);
bool isSoftHyphen(uint32_t cp);
