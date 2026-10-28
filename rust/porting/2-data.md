# Stage 2: compression and data

Stage 2 of the [porting order](../README.md#porting-order) covers compression and data: the ZIP
container, inflate and the serialization formats. Borrowing the framebuffer and the inflate
call are in C++ (`InflateStream`). ContentProtection (SDK) uses its own C copy of miniz.
✅ marks a module that is done.

- **ZipFile container ✅**
- **InflateStream ✅**: the miniz 1.15 `tinfl_decompress` in the S3 ROM. We use what ESP-IDF
  offers, and it costs no flash. On the emulator it decodes ~1.4–1.9 times faster than our own
  decoder, and the chapter build time did not change. `bembeyaz-inflate` is used only in the PNG
  decoders.
- **InflateReader ✅**: deleted; its users moved to InflateStream.
- Serialization: the format readers moved to Rust, and `Serialization.h` is deleted. What is
  left: the `BufferedFileWriter` in the book and library build, and the C++ ends of
  `bembeyaz-io`: `FileIo.h` offers HalFile and BufferedFileWriter as `bb_io_file`/`bb_io_sink`.
