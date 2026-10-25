#pragma once
// Arduino's TwoWire for test/gt911_touch: each transfer goes to the test's onTransmit and
// onRequest, as the device's controller would see it.
#include <cstddef>
#include <cstdint>
#include <functional>
#include <vector>

// The bytes written to addr, and whether a stop ended them; 0 when acknowledged.
inline std::function<int(uint8_t addr, const std::vector<uint8_t>& bytes, bool stop)> onTransmit;
// Up to len bytes read from addr into out; the count read.
inline std::function<uint8_t(uint8_t addr, uint8_t len, std::vector<uint8_t>& out)> onRequest;

class TwoWire {
 public:
  bool begin(int, int, uint32_t) { return true; }
  void setTimeOut(uint16_t) {}
  void beginTransmission(uint8_t addr) {
    _addr = addr;
    _tx.clear();
  }
  size_t write(uint8_t value) {
    _tx.push_back(value);
    return 1;
  }
  size_t write(const uint8_t* data, size_t len) {
    _tx.insert(_tx.end(), data, data + len);
    return len;
  }
  uint8_t endTransmission(bool stop = true) { return static_cast<uint8_t>(onTransmit(_addr, _tx, stop)); }
  uint8_t requestFrom(uint8_t addr, uint8_t len, uint8_t) {
    _rx.clear();
    _at = 0;
    return onRequest(addr, len, _rx);
  }
  int available() const { return static_cast<int>(_rx.size() - _at); }
  int read() { return _at < _rx.size() ? _rx[_at++] : -1; }

 private:
  uint8_t _addr = 0;
  std::vector<uint8_t> _tx, _rx;
  size_t _at = 0;
};

inline TwoWire Wire;
