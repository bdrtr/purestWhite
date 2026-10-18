#pragma once
inline void dictZipTestLog(const char*, const char*, ...) {}
#define LOG_ERR(...) dictZipTestLog(__VA_ARGS__)
