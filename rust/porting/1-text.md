# Stage 1: plain text

Stage 1 of the [porting order](../README.md#porting-order) covers the plain-text modules: UTF-8,
XML, bidirectional text and JSON. They need no hardware and have tests. ✅ marks a module that
is done.

- **Utf8 ✅**: the small-caps mapping `utf8SmallCapsUpper` is also in `bembeyaz-utf8`.
- AsciiText (a two-line `constexpr` helper; it would get slower behind FFI) and HttpHeader (a
  few lines of setting parsing/formatting with `std::string` fields; moving it brings more cost
  than gain) stay in C++ and move together with their callers.
- **XML parser ✅**: it replaces expat. The firmware uses `XmlParser.h`, and the SDK's OPDS and
  catalog readers use the subset of the expat API in `rust/include/expat.h`. It gives the text
  in the same pieces as expat by imitating expat's buffer, because the old chapter parser laid
  out the text one callback at a time. The chapter parser now uses `bembeyaz-xml` directly;
  whether the imitation can be removed can be tried with `test/chapter_corpus`.
- XmlParserUtils (local name helpers): stays in C++ for the same reason.
- **MiniBidi ✅**: exactly the same as minibidi's behavior; the UAX #9 fixes come next.
- **JSON parser ✅**: `bembeyaz-json` replaces the SDK's StreamingJsonParser. `lib/JsonSax`
  carries the SDK's API; the SDK's OPDS 2, catalog and LCP readers and the firmware's LCP device
  registration and unlock parsers use it unchanged. The difference test compares it event by
  event with the SDK's C++ implementation. A string value that does not fit in the buffer is not
  dropped, unlike in the SDK: it is cut at a UTF-8 character boundary and given. Readers that
  store the whole value (the OAuth token, the plugin catalog) get such a long token or address
  cut, and fail later; the difference test turns this off and compares.
- ReleaseJsonParser (`lib/JsonParser`, the update check): stays in C++, runs on top of this
  reader, and moves with OTA in [stage 5](5-application.md).
