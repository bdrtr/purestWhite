// Host stub: the stores' logs are not part of their answers. The macros still take their arguments,
// so a variable used only in a log is not reported as unused.
#pragma once

template <typename... Args>
inline void logStub(const char*, Args&&...) {}

#define LOG_ERR(...) logStub(__VA_ARGS__)
#define LOG_DBG(...) logStub(__VA_ARGS__)
#define LOG_INF(...) logStub(__VA_ARGS__)
