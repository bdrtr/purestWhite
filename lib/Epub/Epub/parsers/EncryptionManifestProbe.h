#pragma once

#include <Print.h>
#include <XmlParser.h>
#include <XmlParserUtils.h>

#include <cstring>

// Stream only the algorithm attributes; resource names stay in the ZIP.
class EncryptionManifestProbe : public Print {
 public:
  bool setup() {
    if (!parser.create()) return false;
    parser.setUserData(this);
    parser.setElementHandler(onStartElement, nullptr);
    return true;
  }

  size_t write(uint8_t data) override { return write(&data, 1); }
  size_t write(const uint8_t* data, size_t size) override { return parser.parse(data, size, false) ? size : 0; }

  bool finish() { return parser.parse(nullptr, 0, true); }
  bool needsProtectionCheck() const { return aes128; }

 private:
  XmlParser parser;
  bool aes128 = false;

  static void onStartElement(void* context, const char* name, const char** attributes) {
    if (!xmlLocalNameEquals(name, "EncryptionMethod")) return;
    for (size_t i = 0; attributes[i]; i += 2) {
      // Match the SDK's supported algorithm; it validates resource references.
      if (xmlLocalNameEquals(attributes[i], "Algorithm") &&
          (std::strstr(attributes[i + 1], "aes128-cbc") || std::strstr(attributes[i + 1], "aes256-cbc"))) {
        static_cast<EncryptionManifestProbe*>(context)->aes128 = true;
      }
    }
  }
};
