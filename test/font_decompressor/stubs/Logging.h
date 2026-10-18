#pragma once

inline void fontDecompressorTestLog(const char*, const char*, ...) {}
#define LOG_ERR(...) fontDecompressorTestLog(__VA_ARGS__)
#define LOG_DBG(...) fontDecompressorTestLog(__VA_ARGS__)
#define LOG_INF(...) fontDecompressorTestLog(__VA_ARGS__)
