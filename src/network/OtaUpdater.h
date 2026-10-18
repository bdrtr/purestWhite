#pragma once

#include <string>

// GitHub API URL of the release to update from (".../releases/latest"); its
// assets are named crosspoint-<tag>-<board>.bin. Set it in platformio.ini.
// Unset, the firmware has no update channel and offers no online update.
#ifndef FIRMWARE_RELEASE_API_URL
#define FIRMWARE_RELEASE_API_URL ""
#endif

class OtaUpdater {
  bool updateAvailable = false;
  std::string latestVersion;
  std::string otaUrl;
  size_t otaSize = 0;
  size_t processedSize = 0;
  size_t totalSize = 0;

 public:
  using ProgressCallback = void (*)(void* ctx);

  enum OtaUpdaterError {
    OK = 0,
    NO_UPDATE,
    HTTP_ERROR,
    JSON_PARSE_ERROR,
    UPDATE_OLDER_ERROR,
    INTERNAL_UPDATE_ERROR,
    OOM_ERROR,
    WRONG_DEVICE_ERROR,
  };

  size_t getOtaSize() const { return otaSize; }

  size_t getProcessedSize() const { return processedSize; }

  size_t getTotalSize() const { return totalSize; }

  OtaUpdater() = default;
  static constexpr bool hasReleaseChannel() { return sizeof(FIRMWARE_RELEASE_API_URL) > 1; }
  bool isUpdateNewer() const;
  const std::string& getLatestVersion() const;
  OtaUpdaterError checkForUpdate();
  OtaUpdaterError installUpdate(ProgressCallback onProgress = nullptr, void* ctx = nullptr);
};
