#pragma once

// Logs go nowhere, but their arguments count as used.
template <typename... Args>
inline void dictionaryTestLog(const Args&...) {}
#define LOG_ERR(...) dictionaryTestLog(__VA_ARGS__)
#define LOG_INF(...) dictionaryTestLog(__VA_ARGS__)
#define LOG_DBG(...) dictionaryTestLog(__VA_ARGS__)
