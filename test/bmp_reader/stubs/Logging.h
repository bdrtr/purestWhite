#pragma once
inline void bmpReaderTestLog(const char*, const char*, ...) {}
#define LOG_DBG(...) bmpReaderTestLog(__VA_ARGS__)
#define LOG_INF(...) bmpReaderTestLog(__VA_ARGS__)
#define LOG_ERR(...) bmpReaderTestLog(__VA_ARGS__)
