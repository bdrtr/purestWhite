// The clipping store's file, as ClippingStore writes and reads it: version 4 written, versions 1
// to 4 read, damaged files refused, and the deletion journal's 65-byte ids.
#include <ClippingStore.h>
#include <HalStorage.h>
#include <gtest/gtest.h>

#include <cstring>
#include <functional>
#include <string>
#include <vector>

namespace {

const std::string BOOK = "/book.epub";

std::string pathFor(const std::string& book) {
  return "/.crosspoint/clippings/epub_" + std::to_string(std::hash<std::string>{}(book)) + ".bin";
}

struct Writer {
  std::string bytes;
  template <typename T>
  Writer& pod(const T value) {
    bytes.append(reinterpret_cast<const char*>(&value), sizeof(T));
    return *this;
  }
  Writer& string(const std::string& s) {
    pod(static_cast<uint32_t>(s.size()));
    bytes += s;
    return *this;
  }
  Writer& raw(const std::string& s, const size_t len) {
    std::string field = s;
    field.resize(len, '\0');
    bytes += field;
    return *this;
  }
};

struct Reader {
  const std::vector<uint8_t>& bytes;
  size_t pos = 0;
  template <typename T>
  T pod() {
    T value{};
    EXPECT_LE(pos + sizeof(T), bytes.size());
    if (pos + sizeof(T) <= bytes.size()) std::memcpy(&value, bytes.data() + pos, sizeof(T));
    pos += sizeof(T);
    return value;
  }
  std::string raw(const size_t len) {
    EXPECT_LE(pos + len, bytes.size());
    std::string s;
    if (pos + len <= bytes.size()) s.assign(bytes.begin() + pos, bytes.begin() + pos + len);
    pos += len;
    return s;
  }
  std::string string() { return raw(pod<uint32_t>()); }
};

struct Fields {
  uint16_t spine, start, end, pages, startWord, endWord, words, paragraph;
  uint32_t timestamp;
};

void writeFields(Writer& w, const Fields& f) {
  w.pod(f.spine).pod(f.start).pod(f.end).pod(f.pages).pod(f.startWord).pod(f.endWord).pod(f.words).pod(f.paragraph);
  w.pod(f.timestamp);
}

void expectFields(const Clipping& c, const Fields& f) {
  EXPECT_EQ(c.spineIndex, f.spine);
  EXPECT_EQ(c.startPage, f.start);
  EXPECT_EQ(c.endPage, f.end);
  EXPECT_EQ(c.pageCount, f.pages);
  EXPECT_EQ(c.startWordIndex, f.startWord);
  EXPECT_EQ(c.endWordIndex, f.endWord);
  EXPECT_EQ(c.wordCount, f.words);
  EXPECT_EQ(c.paragraphIndex, f.paragraph);
  EXPECT_EQ(c.timestamp, f.timestamp);
}

Clipping remote(const unsigned n, const std::string& text) {
  Clipping c;
  c.spineIndex = 100 + n;
  c.startPage = 200 + n;
  c.endPage = 300 + n;
  c.pageCount = 400 + n;
  c.startWordIndex = 500 + n;
  c.endWordIndex = 600 + n;
  c.wordCount = 700 + n;
  c.paragraphIndex = 800 + n;
  c.timestamp = 0x01020304u + n;
  c.layoutSignature = 0x0a0b0c0du + n;
  c.startOffset = 0x11111111u + n;
  c.endOffset = 0x22222222u + n;
  c.syncRevision = 0x0102030405060708ull + n;
  c.pendingUpload = n % 2 == 0;
  snprintf(c.id, sizeof(c.id), "id-%u", n);
  snprintf(c.chapterTitle, sizeof(c.chapterTitle), "Chapter %u", n);
  c.textLength = static_cast<uint16_t>(text.size());
  return c;
}

// Checks the store file field by field, as an independent reader would decode it.
void expectStoreFile(const ClippingStore& store, const std::string& title, const std::string& author,
                     const std::vector<std::pair<Clipping, std::string>>& expected) {
  const auto& bytes = fake::files.at(pathFor(BOOK))->bytes;
  Reader r{bytes};
  EXPECT_EQ(r.pod<uint8_t>(), 4);
  ASSERT_EQ(r.pod<uint16_t>(), expected.size());
  EXPECT_EQ(r.string(), title);
  EXPECT_EQ(r.string(), author);
  EXPECT_EQ(r.string(), BOOK);
  ASSERT_EQ(store.clippingCount(), expected.size());
  for (size_t i = 0; i < expected.size(); ++i) {
    const auto& [c, text] = expected[i];
    EXPECT_EQ(r.pod<uint16_t>(), c.spineIndex);
    EXPECT_EQ(r.pod<uint16_t>(), c.startPage);
    EXPECT_EQ(r.pod<uint16_t>(), c.endPage);
    EXPECT_EQ(r.pod<uint16_t>(), c.pageCount);
    EXPECT_EQ(r.pod<uint16_t>(), c.startWordIndex);
    EXPECT_EQ(r.pod<uint16_t>(), c.endWordIndex);
    EXPECT_EQ(r.pod<uint16_t>(), c.wordCount);
    EXPECT_EQ(r.pod<uint16_t>(), c.paragraphIndex);
    EXPECT_EQ(r.pod<uint32_t>(), c.timestamp);
    EXPECT_EQ(r.pod<uint32_t>(), c.layoutSignature);
    EXPECT_EQ(r.pod<uint32_t>(), c.startOffset);
    EXPECT_EQ(r.pod<uint32_t>(), c.endOffset);
    EXPECT_EQ(r.pod<uint64_t>(), c.syncRevision);
    EXPECT_EQ(r.pod<uint8_t>(), c.pendingUpload ? 1 : 0);
    EXPECT_EQ(r.raw(65), std::string(c.id, 65));
    EXPECT_EQ(r.raw(48), std::string(c.chapterTitle, 48));
    EXPECT_EQ(r.pod<uint16_t>(), text.size());
    EXPECT_EQ(store.clippingAt(i)->textOffset, r.pos);
    EXPECT_EQ(r.raw(text.size()), text);
  }
  EXPECT_EQ(r.pos, bytes.size());
}

void addStore(const std::string& bytes) { fake::add(pathFor(BOOK), bytes); }

Writer header(const uint8_t version, const uint16_t count) {
  Writer w;
  w.pod(version).pod(count).string("Stored title").string("Stored author").string("/old/path.epub");
  return w;
}

// A version 4 record with text.
void writeRecord(Writer& w, const Clipping& c, const std::string& text) {
  writeFields(w, {c.spineIndex, c.startPage, c.endPage, c.pageCount, c.startWordIndex, c.endWordIndex, c.wordCount,
                  c.paragraphIndex, c.timestamp});
  w.pod(c.layoutSignature).pod(c.startOffset).pod(c.endOffset).pod(c.syncRevision);
  w.pod(static_cast<uint8_t>(c.pendingUpload)).raw(std::string(c.id), 65).raw(std::string(c.chapterTitle), 48);
  w.pod(static_cast<uint16_t>(text.size()));
  w.bytes += text;
}

std::string textAt(const ClippingStore& store, const size_t index) {
  std::string text;
  EXPECT_TRUE(store.readClippingText(index, text));
  return text;
}

}  // namespace

TEST(ClippingStoreFormat, WritesVersionFourAndCopiesTextOnRewrite) {
  fake::reset();
  ClippingStore store;
  ASSERT_TRUE(store.loadForBook(BOOK, "Title", "Author", "epub"));
  const std::vector<std::pair<Clipping, std::string>> clippings = {
      {remote(0, "first text"), "first text"},
      {remote(1, ""), ""},
      {remote(2, std::string(300, 'z') + "end"), std::string(300, 'z') + "end"},
      {remote(3, std::string(4096, 'q')), std::string(4096, 'q')},
  };
  for (const auto& [c, text] : clippings) ASSERT_TRUE(store.applyRemote(c, text, false));
  expectStoreFile(store, "Title", "Author", clippings);
  for (size_t i = 0; i < clippings.size(); ++i) EXPECT_EQ(textAt(store, i), clippings[i].second);

  // A rewrite copies every text from the old file.
  ASSERT_TRUE(store.removeClippingAt(1));
  const std::vector<std::pair<Clipping, std::string>> left = {clippings[0], clippings[2], clippings[3]};
  expectStoreFile(store, "Title", "Author", left);
  store.unload();
  ASSERT_TRUE(store.loadForBook(BOOK, "", "", "epub"));
  expectStoreFile(store, "Title", "Author", left);
  for (size_t i = 0; i < left.size(); ++i) EXPECT_EQ(textAt(store, i), left[i].second);

  // A remote change replaces one text and copies the others.
  Clipping changed = clippings[2].first;
  changed.syncRevision += 10;
  changed.textLength = 3;
  ASSERT_TRUE(store.applyRemote(changed, "new", false));
  expectStoreFile(store, "Title", "Author", {left[0], {changed, "new"}, left[2]});
}

TEST(ClippingStoreFormat, ReadsVersionFourFieldsAndForcesStringEnds) {
  fake::reset();
  Writer w = header(4, 2);
  Clipping first = remote(7, "abc");
  writeRecord(w, first, "abc");
  // An id and a chapter title that fill their fields: the last byte reads as the end.
  Writer full;
  writeFields(full, {1, 2, 3, 4, 5, 6, 7, 8, 9});
  full.pod(uint32_t{10}).pod(uint32_t{11}).pod(uint32_t{12}).pod(uint64_t{13}).pod(uint8_t{0});
  full.raw(std::string(65, 'i'), 65).raw(std::string(48, 'c'), 48).pod(uint16_t{0});
  w.bytes += full.bytes;
  w.bytes += "trailing bytes are ignored";
  addStore(w.bytes);
  ClippingStore store;
  ASSERT_TRUE(store.loadForBook(BOOK, "Given", "", "epub"));
  ASSERT_EQ(store.clippingCount(), 2u);
  const Clipping& a = *store.clippingAt(0);
  expectFields(a, {107, 207, 307, 407, 507, 607, 707, 807, 0x01020304u + 7});
  EXPECT_EQ(a.layoutSignature, first.layoutSignature);
  EXPECT_EQ(a.startOffset, first.startOffset);
  EXPECT_EQ(a.endOffset, first.endOffset);
  EXPECT_EQ(a.syncRevision, first.syncRevision);
  EXPECT_FALSE(a.pendingUpload);
  EXPECT_STREQ(a.id, "id-7");
  EXPECT_STREQ(a.chapterTitle, "Chapter 7");
  EXPECT_EQ(a.textLength, 3u);
  EXPECT_EQ(textAt(store, 0), "abc");
  const Clipping& b = *store.clippingAt(1);
  expectFields(b, {1, 2, 3, 4, 5, 6, 7, 8, 9});
  EXPECT_EQ(b.layoutSignature, 10u);
  EXPECT_EQ(b.startOffset, 11u);
  EXPECT_EQ(b.endOffset, 12u);
  EXPECT_EQ(b.syncRevision, 13u);
  EXPECT_FALSE(b.pendingUpload);
  EXPECT_EQ(std::string(b.id), std::string(64, 'i'));
  EXPECT_EQ(std::string(b.chapterTitle), std::string(47, 'c'));
  EXPECT_EQ(b.textLength, 0u);

  // The given title wins; the stored author fills the empty one; the path is the book's.
  first.pendingUpload = true;
  first.syncRevision += 1;
  Clipping second = b;
  ASSERT_TRUE(store.applyRemote(first, "abc", false));
  expectStoreFile(store, "Given", "Stored author", {{first, "abc"}, {second, ""}});
}

TEST(ClippingStoreFormat, ReadsVersionOneWithDefaultsAndClampedText) {
  fake::reset();
  Writer w = header(1, 3);
  writeFields(w, {1, 2, 3, 4, 5, 6, 7, 8, 9});
  w.raw("Old chapter", 48).pod(uint32_t{5}).bytes += "hello";
  // Longer than a clipping holds: the first 4096 bytes are its text; the rest is skipped.
  writeFields(w, {11, 12, 13, 14, 15, 16, 17, 18, 19});
  w.raw("", 48).pod(uint32_t{5000});
  w.bytes += std::string(4096, 'a') + std::string(904, 'b');
  writeFields(w, {21, 22, 23, 24, 25, 26, 27, 28, 29});
  w.raw("Last", 48).pod(uint32_t{0});
  addStore(w.bytes);
  ClippingStore store;
  ASSERT_TRUE(store.loadForBook(BOOK, "", "", "epub"));
  ASSERT_EQ(store.clippingCount(), 3u);
  const Fields fields[] = {{1, 2, 3, 4, 5, 6, 7, 8, 9}, {11, 12, 13, 14, 15, 16, 17, 18, 19},
                           {21, 22, 23, 24, 25, 26, 27, 28, 29}};
  for (size_t i = 0; i < 3; ++i) {
    const Clipping& c = *store.clippingAt(i);
    expectFields(c, fields[i]);
    EXPECT_EQ(c.layoutSignature, 0u);
    EXPECT_EQ(c.startOffset, UINT32_MAX);
    EXPECT_EQ(c.endOffset, UINT32_MAX);
    EXPECT_EQ(c.syncRevision, 0u);
    EXPECT_TRUE(c.pendingUpload);
    EXPECT_EQ(c.id[0], '\0');
  }
  EXPECT_STREQ(store.clippingAt(0)->chapterTitle, "Old chapter");
  EXPECT_EQ(store.clippingAt(1)->textLength, 4096u);
  EXPECT_EQ(textAt(store, 0), "hello");
  EXPECT_EQ(textAt(store, 1), std::string(4096, 'a'));
  EXPECT_EQ(textAt(store, 2), "");

  // Rewritten as version 4 with the stored title and author.
  ASSERT_TRUE(store.prepareSync());
  std::vector<std::pair<Clipping, std::string>> expected;
  for (size_t i = 0; i < 3; ++i) expected.push_back({*store.clippingAt(i), textAt(store, i)});
  expectStoreFile(store, "Stored title", "Stored author", expected);
}

TEST(ClippingStoreFormat, ReadsVersionThreeLayoutSignature) {
  fake::reset();
  Writer w = header(3, 1);
  writeFields(w, {1, 2, 3, 4, 5, 6, 7, 8, 9});
  w.pod(uint32_t{0xfeedbeef}).raw("Chapter", 48).pod(uint16_t{4}).bytes += "text";
  addStore(w.bytes);
  ClippingStore store;
  ASSERT_TRUE(store.loadForBook(BOOK, "T", "A", "epub"));
  ASSERT_EQ(store.clippingCount(), 1u);
  const Clipping& c = *store.clippingAt(0);
  expectFields(c, {1, 2, 3, 4, 5, 6, 7, 8, 9});
  EXPECT_EQ(c.layoutSignature, 0xfeedbeefu);
  EXPECT_EQ(c.startOffset, UINT32_MAX);
  EXPECT_TRUE(c.pendingUpload);
  EXPECT_EQ(c.id[0], '\0');
  EXPECT_STREQ(c.chapterTitle, "Chapter");
  EXPECT_EQ(textAt(store, 0), "text");
}

// Version 2 was never written; it reads as version 1 with version 3's text length.
TEST(ClippingStoreFormat, ReadsVersionTwoAsOneWithShortTextLength) {
  fake::reset();
  Writer w = header(2, 1);
  writeFields(w, {1, 2, 3, 4, 5, 6, 7, 8, 9});
  w.raw("Chapter", 48).pod(uint16_t{2}).bytes += "hi";
  addStore(w.bytes);
  ClippingStore store;
  ASSERT_TRUE(store.loadForBook(BOOK, "T", "A", "epub"));
  ASSERT_EQ(store.clippingCount(), 1u);
  expectFields(*store.clippingAt(0), {1, 2, 3, 4, 5, 6, 7, 8, 9});
  EXPECT_EQ(store.clippingAt(0)->layoutSignature, 0u);
  EXPECT_EQ(textAt(store, 0), "hi");
}

TEST(ClippingStoreFormat, RefusesDamagedFilesAndLeavesThemAlone) {
  Writer valid = header(4, 2);
  writeRecord(valid, remote(0, "one"), "one");
  writeRecord(valid, remote(1, "two!"), "two!");
  std::vector<std::string> damaged;
  // Every cut short of the last record's end.
  for (size_t len = 0; len < valid.bytes.size(); ++len) damaged.push_back(valid.bytes.substr(0, len));
  for (const uint8_t version : {0, 5, 255}) {
    std::string bytes = valid.bytes;
    bytes[0] = static_cast<char>(version);
    damaged.push_back(bytes);
  }
  // A header string over 4096 bytes, though the file holds it.
  {
    Writer w;
    w.pod(uint8_t{4}).pod(uint16_t{0}).string(std::string(4097, 't')).string("").string("");
    damaged.push_back(w.bytes);
  }
  // A header string longer than the file.
  {
    Writer w;
    w.pod(uint8_t{4}).pod(uint16_t{0}).string("").pod(uint32_t{100});
    damaged.push_back(w.bytes + "short");
  }
  // More clippings than a book holds.
  {
    Writer w = header(4, CLIPPING_MAX_PER_BOOK + 1);
    for (unsigned i = 0; i <= CLIPPING_MAX_PER_BOOK; ++i) writeRecord(w, remote(i, ""), "");
    damaged.push_back(w.bytes);
  }
  // Versions 3 and 4: a text over 4096 bytes, though the file holds it.
  {
    Writer w = header(4, 1);
    writeRecord(w, remote(0, ""), std::string(4097, 'x'));
    damaged.push_back(w.bytes);
    Writer v3 = header(3, 1);
    writeFields(v3, {});
    v3.pod(uint32_t{1}).raw("", 48).pod(uint16_t{4097});
    damaged.push_back(v3.bytes + std::string(4097, 'x'));
  }
  // Version 1: a text longer than the file.
  {
    Writer w = header(1, 1);
    writeFields(w, {});
    w.raw("", 48).pod(uint32_t{10});
    damaged.push_back(w.bytes + "short");
  }
  for (const auto& bytes : damaged) {
    fake::reset();
    addStore(bytes);
    ClippingStore store;
    EXPECT_FALSE(store.loadForBook(BOOK, "Book", "", "epub")) << bytes.size();
    EXPECT_EQ(store.clippingCount(), 0u);
    store.unload();
    const auto& stored = fake::files.at(pathFor(BOOK))->bytes;
    EXPECT_EQ(std::string(stored.begin(), stored.end()), bytes);
  }
  fake::reset();
  addStore(valid.bytes);
  ClippingStore store;
  ASSERT_TRUE(store.loadForBook(BOOK, "Book", "", "epub"));
  EXPECT_EQ(store.clippingCount(), 2u);
  EXPECT_EQ(textAt(store, 1), "two!");
}

TEST(ClippingStoreFormat, DeletionJournalSkipsIdsStillStored) {
  fake::reset();
  ClippingStore store;
  ASSERT_TRUE(store.loadForBook(BOOK, "Book", "", "epub"));
  uint32_t offset = 0;
  char id[65] = "stale";
  ASSERT_TRUE(store.nextDeletion(offset, id));  // no journal
  EXPECT_EQ(id[0], '\0');
  ASSERT_TRUE(store.applyRemote(remote(1, ""), "", false));
  Writer journal;
  journal.raw("gone-a", 65).raw("id-1", 65).raw("id-1", 65).raw(std::string(65, 'g'), 65).raw("id-1", 65);
  fake::add(pathFor(BOOK) + ".deleted", journal.bytes);

  ASSERT_TRUE(store.nextDeletion(offset, id));
  EXPECT_STREQ(id, "gone-a");
  EXPECT_EQ(offset, 65u);
  ASSERT_TRUE(store.nextDeletion(offset, id));
  EXPECT_EQ(std::string(id), std::string(64, 'g'));
  EXPECT_EQ(offset, 260u);
  ASSERT_TRUE(store.nextDeletion(offset, id));  // only stored ids left
  EXPECT_EQ(id[0], '\0');
  EXPECT_EQ(offset, 325u);
  ASSERT_TRUE(store.nextDeletion(offset, id));
  EXPECT_EQ(id[0], '\0');
  EXPECT_EQ(offset, 325u);

  offset = 64;  // not at an id
  EXPECT_FALSE(store.nextDeletion(offset, id));
  offset = 390;  // past the end
  EXPECT_FALSE(store.nextDeletion(offset, id));
  offset = 0;
  fake::failRead = 0;
  EXPECT_FALSE(store.nextDeletion(offset, id));
  EXPECT_EQ(offset, 0u);
  // A cut id is dropped before reading.
  fake::files.at(pathFor(BOOK) + ".deleted")->bytes.resize(65 + 30);
  ASSERT_TRUE(store.nextDeletion(offset, id));
  EXPECT_STREQ(id, "gone-a");
  ASSERT_TRUE(store.nextDeletion(offset, id));
  EXPECT_EQ(id[0], '\0');
  EXPECT_EQ(fake::files.at(pathFor(BOOK) + ".deleted")->bytes.size(), 65u);
}
