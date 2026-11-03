#pragma once
#include <string>

#include "KOReaderSyncClient.h"

// The JSON KOReaderSyncClient sends and reads, apart from the HTTP: what each request's body
// holds and how a progress answer is read.
namespace kosync {

// The device name and id sent with every progress update.
inline constexpr char DEVICE_NAME[] = "CrossPoint";
inline constexpr char DEVICE_ID[] = "crosspoint-reader";

// POST /users/create's body: the username and the MD5 auth key.
std::string createUserBody(const std::string& username, const std::string& md5Password);

// PUT /syncs/progress's body. extendedMetadata adds the bibliographic fields (custom servers);
// richProgress adds the CrossPoint position (any server but strict KOSync).
std::string progressBody(const KOReaderProgress& progress, bool extendedMetadata, bool richProgress);

// Reads GET /syncs/progress's answer into out (its document set to documentHash). nullptr when it
// was read, else why the JSON was not (ArduinoJson's error name).
const char* parseProgress(const char* body, const std::string& documentHash, bool richProgress,
                          KOReaderProgress& out);

}  // namespace kosync
