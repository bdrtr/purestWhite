#pragma once
#include <Print.h>
#include <bembeyaz/container.h>

#include <string>

// Reads META-INF/container.xml streamed into it (rust/crates/bembeyaz-opf, container module):
// fullPath is the package document's path read so far.
class ContainerParser final : public Print {
  size_t remainingSize;
  bb_container* container = nullptr;

 public:
  std::string fullPath;

  explicit ContainerParser(const size_t xmlSize) : remainingSize(xmlSize) {}
  ~ContainerParser() override;
  ContainerParser(const ContainerParser&) = delete;
  ContainerParser& operator=(const ContainerParser&) = delete;

  bool setup();

  size_t write(uint8_t) override;
  size_t write(const uint8_t* buffer, size_t size) override;
};
