#pragma once
inline void pngToBmpTestLog(const char*, const char*, ...) {}
#define LOG_DBG(...) pngToBmpTestLog(__VA_ARGS__)
#define LOG_INF(...) pngToBmpTestLog(__VA_ARGS__)
#define LOG_ERR(...) pngToBmpTestLog(__VA_ARGS__)
