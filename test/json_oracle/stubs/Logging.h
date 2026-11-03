// Host stub: the stores' logs are not part of their answers, but where a driver is built with
// -DSTORE_LOG_ERRORS: then each LOG_ERR line goes to the transcript as "  ! " and the line
// ("<tag>: <message>") in hex, before the step's answer. The macros still take their arguments,
// so a variable used only in a log is not reported as unused.
#pragma once

#ifdef STORE_LOG_ERRORS
#include <cstdio>

template <typename... Args>
inline void logError(const char* tag, const char* format, Args&&... args) {
  char line[512];
  int len = std::snprintf(line, sizeof(line), "%s: ", tag);
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wformat-nonliteral"
#pragma GCC diagnostic ignored "-Wformat-security"
  len += std::snprintf(line + len, sizeof(line) - static_cast<size_t>(len), format, args...);
#pragma GCC diagnostic pop
  std::printf("  ! ");
  for (int i = 0; i < len && i < static_cast<int>(sizeof(line)) - 1; i++) {
    std::printf("%02x", static_cast<unsigned char>(line[i]));
  }
  std::printf("\n");
}
#endif

template <typename... Args>
inline void logStub(const char*, Args&&...) {}

#ifdef STORE_LOG_ERRORS
#define LOG_ERR(...) logError(__VA_ARGS__)
#else
#define LOG_ERR(...) logStub(__VA_ARGS__)
#endif
#define LOG_DBG(...) logStub(__VA_ARGS__)
#define LOG_INF(...) logStub(__VA_ARGS__)
