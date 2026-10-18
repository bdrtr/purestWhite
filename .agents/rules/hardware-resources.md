## Platform and Hardware Constraints

### Hardware Specs

* MCU: ESP32-S3 (X4 Pro, X4 Classic; dual-core Xtensa LX7)
* RAM: ~320KB internal plus 8MB PSRAM. Internal RAM is the limit: DMA buffers,
  task stacks and the radio need it.
  * **Single Buffer Mode**: One framebuffer sized for the runtime panel
* Flash: 16MB (Instruction storage and static data)
* Display: 800x480 E-Ink (Slow refresh, monochrome, 1-2s full update)
  * Framebuffer: 48,000 bytes (800 × 480 ÷ 8)
* Storage: SD Card (Used for books and aggressive caching)

Check `platformio.ini`, the selected SDK `BoardConfig`, and `lib/hal/` for the
actual display size, touch/buttons, frontlight, grayscale, storage bus, USB-MSC,
and PSRAM capabilities. S3 does not by itself guarantee usable PSRAM or touch;
X4 Classic is a button-only board. Keep it working when adding a capability
that needs touch or a frontlight.

The checked-in build and release targets are `x4pro` and `x4c`. Inspect the
current workflow matrices for coverage. Additional SDK profiles are not automatically
CrossPoint release targets or evidence of device verification.

Budget internal RAM and PSRAM separately with `HalMemory::getInternalHeap()`
and `getPsramHeap()`, including `largestBlockBytes`; a combined free-heap
reading can hide internal-heap pressure. `HalMemory::allocatePsram()` returns
null rather than falling back to internal RAM. Preserve that failure contract.

### The Resource Protocol

1. Stack Safety: Limit local function variables to < 256 bytes. Task stacks are small (3-24 KiB); use std::unique_ptr or static pools for larger buffers.
2. Heap Fragmentation: Avoid repeated new/delete in loops. Allocate buffers once during onEnter() and reuse them.
3. Flash Persistence: Give large immutable tables static storage and const qualification so the linker can place them in flash. Check the build map for material memory claims; data needed while the flash cache is disabled must be in accessible internal RAM.
4. String Policy: std::string and Arduino String are acceptable in cold file-I/O/setup paths. Hot/render paths use std::string_view for read-only access and snprintf with fixed char[] buffers; an unavoidable std::string must reserve storage first.
5. User-facing strings follow the [translation workflow](generated-files-cache.md#translation-workflow).
6. `constexpr` First: Use `constexpr` for compile-time constants and lookup tables, and `static constexpr` for class-level constants. This enables constant evaluation; it does not guarantee flash placement or remove runtime storage in every use.
7. `std::vector` Pre-allocation: Always call `.reserve(N)` before any `push_back()` loop. Each growth event allocates a new block (2×), copies all elements, then frees the old one — three heap operations that fragment DRAM. When the final size is unknown, estimate conservatively.
8. SD Persistence Throttling: Settings, state, credentials, and other `PersistableStore` JSON files live on SD under `/.crosspoint/` through `HalStorage`; SPIFFS is not mounted. Guard redundant writes and debounce progress saves to avoid serialization, SD I/O, and `storageMutex` cost.
9. Fallible storage follows the [checked-allocation contract](coding-standards.md#heap-buffer-allocation); budget peak coexistence during container growth, not just final size.

---
