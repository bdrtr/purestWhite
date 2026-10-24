#include "Dictionary.h"

#include <Arduino.h>
#include <Logging.h>
#include <Memory.h>
#include <bembeyaz/dict.h>

#include <cstddef>
#include <cstdio>
#include <cstring>

#include "DictZip.h"
#include "DictionaryRegistry.h"

namespace {

// Shared temp file for entries lazily extracted from .dict.dz.
constexpr const char* DICT_TMP_FILE = "/.crosspoint/dict.tmp";

// Slack required above a definition buffer before we commit to reading it, so a
// request that would only just fit is refused rather than left to abort mid-read.
constexpr uint32_t DEFINITION_HEAP_HEADROOM_BYTES = 8 * 1024;

// The sidecars (.qidx over .idx, .sidx over .syn) and lookups are bembeyaz-dict's (dict.h).
// dict.h's layouts, which bembeyaz-ffi's dict.rs checks too.
static_assert(sizeof(bb_dict_source) == 2 * sizeof(void*) && offsetof(bb_dict_index, sidecar) == 2 * sizeof(void*) &&
                  offsetof(bb_dict_index, entry_count) == 4 * sizeof(void*) + 8,
              "bb_dict_index layout");
static_assert(sizeof(bb_dict_location) == 12 && offsetof(bb_dict_location, read_error) == 9, "bb_dict_location layout");
static_assert(sizeof(bb_dict_scan) == 24 && alignof(bb_dict_scan) == 4, "bb_dict_scan layout");
static_assert(BB_DICT_HEADER_LEN == 24 && BB_DICT_WORD_MAX == 255 && BB_DICT_QIDX_MAGIC == 0x58444951u &&
                  BB_DICT_SIDX_MAGIC == 0x58444953u && BB_DICT_IDX_SUFFIX == 8 && BB_DICT_SYN_SUFFIX == 4,
              "dict.h values");
constexpr uint32_t QIDX_MAGIC = BB_DICT_QIDX_MAGIC;
constexpr uint32_t SIDX_MAGIC = BB_DICT_SIDX_MAGIC;

struct SidecarHeader {
  uint32_t sampleCount = 0;
  uint32_t sourceFileSize = 0;
  uint32_t entryCount = 0;
  bool valid = false;
};

SidecarHeader readSidecarHeader(HalFile& file, uint32_t magic) {
  SidecarHeader header;
  uint8_t raw[BB_DICT_HEADER_LEN];
  if (!file.seekSet(0) || file.read(raw, sizeof(raw)) != static_cast<int>(sizeof(raw))) return header;
  header.valid = bb_dict_header(raw, magic, &header.sampleCount, &header.sourceFileSize, &header.entryCount);
  return header;
}

// A file the lookups read at positions.
bb_dict_source sourceOf(HalFile& file) {
  return {[](void* f, const uint32_t pos, uint8_t* buf, const size_t len) -> int32_t {
            auto& file = *static_cast<HalFile*>(f);
            if (!file.seekSet(pos)) return -1;
            const int n = file.read(buf, len);
            return n < 0 ? -2 : n;
          },
          &file};
}

// Facts read from the .ifo at open time. Only the first 2KB is scanned — .ifo
// headers are tiny and both keys always appear early when present.
struct IfoFacts {
  bool offsets64 = false;        // idxoffsetbits=64 (unsupported)
  bool htmlDefinitions = false;  // sametypesequence=h or x (HTML or XDXF)
};

IfoFacts readIfoFacts(const std::string& ifoPath) {
  IfoFacts facts;
  HalFile ifo;
  if (!Storage.openFileForRead("DICT", ifoPath, ifo)) return facts;
  uint8_t buf[2047];
  const int n = ifo.read(buf, sizeof(buf));
  if (n <= 0) return facts;
  // HTML and XDXF share the styled renderer; multi-type entries stay plain text.
  bb_dict_ifo(buf, static_cast<size_t>(n), &facts.offsets64, &facts.htmlDefinitions);
  return facts;
}

}  // namespace

bool Dictionary::open(const char* folderName) {
  basePath.clear();
  hasSyn = false;
  htmlDefinitions = false;
  std::string resolved;
  if (!DictionaryRegistry::resolveBasePath(folderName, resolved)) {
    LOG_ERR("DICT", "No dictionary found in folder '%s'", folderName ? folderName : "");
    return false;
  }

  if (!Storage.exists((resolved + ".idx").c_str())) {
    LOG_ERR("DICT", "%s.idx missing (compressed .idx.gz is not supported)", resolved.c_str());
    return false;
  }
  hasPlainDict = Storage.exists((resolved + ".dict").c_str());
  if (!hasPlainDict && !Storage.exists((resolved + ".dict.dz").c_str())) {
    LOG_ERR("DICT", "%s has no .dict or .dict.dz", resolved.c_str());
    return false;
  }
  const IfoFacts ifo = readIfoFacts(resolved + ".ifo");
  if (ifo.offsets64) {
    LOG_ERR("DICT", "%s uses 64-bit index offsets (unsupported)", resolved.c_str());
    return false;
  }
  // Checked once here so buildPath() can never fail on the lookup path.
  if (resolved.size() + LONGEST_SUFFIX_LEN + 1 > PATH_BUF_BYTES) {
    LOG_ERR("DICT", "Dictionary path too long (%u chars, max %u)", static_cast<unsigned>(resolved.size()),
            static_cast<unsigned>(PATH_BUF_BYTES - LONGEST_SUFFIX_LEN - 1));
    return false;
  }
  hasSyn = Storage.exists((resolved + ".syn").c_str());
  htmlDefinitions = ifo.htmlDefinitions;

  basePath = std::move(resolved);
  return true;
}

bool Dictionary::buildPath(char* buf, size_t bufSize, const char* suffix) const {
  const int n = snprintf(buf, bufSize, "%s%s", basePath.c_str(), suffix);
  if (n < 0 || static_cast<size_t>(n) >= bufSize) {
    LOG_ERR("DICT", "Path too long: %s%s", basePath.c_str(), suffix);
    return false;
  }
  return true;
}

// A sidecar is stale when it is missing, unreadable, the wrong version, or built
// from a different source size. Missing source returns false (not stale): the
// dictionary is unusable without its .idx, and a vanished .syn degrades to no
// synonyms — neither is fixable by re-indexing here. This is the same rule
// openSession() applies to .qidx (a successful build always writes at least
// entry 0, so sampleCount == 0 means absent, stale or corrupt), expressed over a
// path so it can cover the .syn/.sidx pair too.
bool Dictionary::sidecarIsStale(const std::string& sourcePath, const std::string& sidecarPath, uint32_t magic) {
  HalFile src;
  if (!Storage.openFileForRead("DICT", sourcePath, src)) return false;
  const uint32_t srcSize = static_cast<uint32_t>(src.fileSize());

  HalFile sidecar;
  if (!Storage.openFileForRead("DICT", sidecarPath, sidecar)) return true;
  const SidecarHeader header = readSidecarHeader(sidecar, magic);
  return !header.valid || header.sourceFileSize != srcSize;
}

bool Dictionary::needsIndex() {
  if (!isOpen()) return false;
  if (sidecarIsStale(basePath + ".idx", basePath + ".qidx", QIDX_MAGIC)) return true;
  return hasSyn && sidecarIsStale(basePath + ".syn", basePath + ".sidx", SIDX_MAGIC);
}

bool Dictionary::buildIndex(void (*yieldFn)(void*), void* ctx, IndexResult* outResult) {
  const auto fail = [outResult](IndexResult r) {
    if (outResult) *outResult = r;
    return false;
  };
  if (outResult) *outResult = IndexResult::Ok;
  if (!isOpen()) return fail(IndexResult::ReadError);

  // The .idx sidecar is mandatory — lookups binary-search it. Rebuild only when
  // stale so a .syn-only change doesn't force a needless rescan of the (much
  // larger) .idx, and vice versa.
  if (sidecarIsStale(basePath + ".idx", basePath + ".qidx", QIDX_MAGIC) &&
      !buildSidecar(basePath + ".idx", basePath + ".qidx", QIDX_MAGIC, BB_DICT_IDX_SUFFIX, yieldFn, ctx, outResult)) {
    return false;
  }

  // The synonym sidecar is best-effort: a failure here (e.g. transient OOM)
  // leaves synonym lookups disabled but the dictionary otherwise usable, so it
  // does not fail the build or overwrite *outResult. hasSyn is left alone — it
  // means "a .syn file exists", so needsIndex() keeps reporting the sidecar
  // stale and a later build retries. openSynonyms() is what declines the
  // synonym path while the sidecar is unusable.
  if (hasSyn && sidecarIsStale(basePath + ".syn", basePath + ".sidx", SIDX_MAGIC) &&
      !buildSidecar(basePath + ".syn", basePath + ".sidx", SIDX_MAGIC, BB_DICT_SYN_SUFFIX, yieldFn, ctx, nullptr)) {
    LOG_ERR("DICT", "Synonym index build failed; synonyms disabled for %s", basePath.c_str());
  }
  return true;
}

bool Dictionary::buildSidecar(const std::string& sourcePath, const std::string& sidecarPath, uint32_t magic,
                              uint32_t suffixBytes, void (*yieldFn)(void*), void* ctx, IndexResult* outResult) {
  const auto fail = [outResult](IndexResult r) {
    if (outResult) *outResult = r;
    return false;
  };

  HalFile src;
  if (!Storage.openFileForRead("DICT", sourcePath, src)) return fail(IndexResult::ReadError);
  const uint32_t srcSize = static_cast<uint32_t>(src.fileSize());

  constexpr size_t CHUNK_BYTES = 4096;
  auto buf = makeUniqueNoThrow<uint8_t[]>(CHUNK_BYTES);
  if (!buf) {
    LOG_ERR("DICT", "OOM: %u byte index scan buffer", CHUNK_BYTES);
    return fail(IndexResult::LowMemory);
  }

  // Stream each sample offset straight to the sidecar instead of accumulating
  // them in RAM: a large source would otherwise cost tens of KB of vector heap,
  // and vector growth aborts on OOM under -fno-exceptions. The header slot is
  // zero-filled until the scan succeeds, so an interrupted build leaves a file
  // readSidecarHeader rejects (magic mismatch) and needsIndex() triggers a rebuild.
  HalFile out;
  if (!Storage.openFileForWrite("DICT", sidecarPath, out)) return fail(IndexResult::ReadError);
  const auto writeU32 = [](void* file, const uint32_t v) {
    return static_cast<HalFile*>(file)->write(&v, sizeof(v)) == static_cast<int>(sizeof(v));
  };
  const uint8_t placeholder[BB_DICT_HEADER_LEN] = {};
  // Entry 0 always starts at byte 0.
  bool ok = out.write(placeholder, sizeof(placeholder)) == sizeof(placeholder) && writeU32(&out, 0);

  const unsigned long startMs = millis();
  bb_dict_scan scan;
  bb_dict_scan_init(&scan, suffixBytes, srcSize);
  uint32_t pos = 0;
  uint32_t sinceYield = 0;
  while (ok && pos < srcSize) {
    const int n = src.read(buf.get(), CHUNK_BYTES);
    if (n <= 0) {
      LOG_ERR("DICT", "Index scan read failed at %lu", static_cast<unsigned long>(pos));
      ok = false;
      break;
    }
    ok = bb_dict_scan_feed(&scan, buf.get(), static_cast<size_t>(n), writeU32, &out);
    pos += n;
    sinceYield += n;
    if (yieldFn && sinceYield >= 64 * 1024) {
      sinceYield = 0;
      yieldFn(ctx);
    }
  }
  uint32_t entryCount = 0;
  uint32_t sampleCount = 0;
  bb_dict_scan_counts(&scan, &entryCount, &sampleCount);

  if (ok) {
    // Backpatch the now-valid header over the placeholder.
    uint8_t header[BB_DICT_HEADER_LEN];
    bb_dict_header_bytes(magic, sampleCount, srcSize, entryCount, header);
    ok = out.seekSet(0) && out.write(header, sizeof(header)) == sizeof(header);
  }
  if (!ok) {
    LOG_ERR("DICT", "Index build failed, removing %s", sidecarPath.c_str());
    out.close();  // close before remove of the same path
    Storage.remove(sidecarPath.c_str());
    return fail(IndexResult::ReadError);
  }

  LOG_INF("DICT", "Indexed %lu entries (%lu samples) from %s in %lu ms", static_cast<unsigned long>(entryCount),
          static_cast<unsigned long>(sampleCount), sourcePath.c_str(), millis() - startMs);
  return true;
}

bool Dictionary::openSession(LookupSession& session) {
  if (!isOpen()) return false;

  // One buffer reused for both paths — no transient heap on the lookup path.
  char path[PATH_BUF_BYTES];
  if (!buildPath(path, sizeof(path), ".idx")) return false;
  if (!Storage.openFileForRead("DICT", path, session.idx)) return false;
  session.idxSize = static_cast<uint32_t>(session.idx.fileSize());

  // The sidecar is optional and its header only has to be validated once per
  // lookup; without a usable one locate() scans from byte 0.
  if (!buildPath(path, sizeof(path), ".qidx")) return true;
  if (Storage.openFileForRead("DICT", path, session.qidx)) {
    const SidecarHeader header = readSidecarHeader(session.qidx, QIDX_MAGIC);
    if (header.valid && header.sourceFileSize == session.idxSize) {
      session.sampleCount = header.sampleCount;
      session.entryCount = header.entryCount;
    }
  }
  return true;
}

bool Dictionary::openSynonyms(LookupSession& session) {
  if (session.synOpened) return session.synSize > 0;
  session.synOpened = true;
  if (!hasSyn) return false;

  char path[PATH_BUF_BYTES];
  if (!buildPath(path, sizeof(path), ".syn") || !Storage.openFileForRead("DICT", path, session.syn)) {
    // A .syn exists but won't open: the synonym probe never reached a verdict, so
    // flag it the way openSession() flags an unopenable .idx rather than let
    // lookup() report a genuine miss for a word the .syn does carry.
    session.synFailed = true;
    return false;
  }
  session.synSize = static_cast<uint32_t>(session.syn.fileSize());

  if (buildPath(path, sizeof(path), ".sidx") && Storage.openFileForRead("DICT", path, session.sidx)) {
    const SidecarHeader header = readSidecarHeader(session.sidx, SIDX_MAGIC);
    if (header.valid && header.sourceFileSize == session.synSize) session.synSampleCount = header.sampleCount;
  }

  // Unlike .qidx, whose absence only costs locate() a bounded-cost scan of an
  // already-open file, an unusable .sidx would make every miss read the whole
  // .syn a byte at a time under the storage mutex. Decline the synonym path
  // instead and release both handles; needsIndex() still reports .sidx stale, so
  // the next index pass rebuilds it.
  if (session.synSampleCount == 0) {
    LOG_ERR("DICT", "Synonym index unusable for %s; synonyms skipped", basePath.c_str());
    session.synFailed = true;
    session.synSize = 0;
    session.sidx.close();
    session.syn.close();
    return false;
  }
  return true;
}

bb_dict_index Dictionary::LookupSession::index() {
  return {sourceOf(idx), sourceOf(qidx), idxSize, sampleCount, entryCount};
}

// Bisect the .qidx samples to the last entry <= target, then scan at most
// 256 entries. Both files stay open across the stem-variant probes.
DictLocation Dictionary::locate(LookupSession& session, const char* target, std::string* matchedHeadwordOut) {
  const bb_dict_index idx = session.index();
  bb_dict_location at{};
  bb_dict_locate(&idx, target, strlen(target), wordBuf, &at);
  return located(at, wordBuf, matchedHeadwordOut);
}

DictLocation Dictionary::locateSynonym(LookupSession& session, const char* target, std::string* matchedHeadwordOut) {
  DictLocation result;
  if (!openSynonyms(session)) {
    result.readError = session.synFailed;  // false when there is simply no .syn
    return result;
  }
  // A .syn entry carries the ordinal of its .idx entry.
  const bb_dict_index syn{sourceOf(session.syn), sourceOf(session.sidx), session.synSize, session.synSampleCount, 0};
  const bb_dict_index idx = session.index();
  bb_dict_location at{};
  bb_dict_locate_synonym(&syn, &idx, target, strlen(target), wordBuf, &at);
  return located(at, wordBuf, matchedHeadwordOut);
}

DictLocation Dictionary::located(const bb_dict_location& at, const char* headword, std::string* matchedHeadwordOut) {
  if (at.read_error) LOG_ERR("DICT", "Index seek failed");
  if (at.found && matchedHeadwordOut) *matchedHeadwordOut = headword;
  DictLocation result;
  result.offset = at.offset;
  result.size = at.size;
  result.found = at.found;
  result.readError = at.read_error;
  return result;
}

bool Dictionary::readDefinition(const DictLocation& location, std::string& out, LookupResult* outResult) {
  const auto fail = [outResult](LookupResult r) {
    if (outResult) *outResult = r;
    return false;
  };
  if (!location.found) return fail(LookupResult::NotFound);
  const uint32_t size = std::min(location.size, MAX_DEFINITION_BYTES);
  if (size == 0) {
    LOG_ERR("DICT", "Zero-length definition entry");
    return fail(LookupResult::ReadError);
  }

  // Refuse before touching the heap or the SD: std::string growth aborts on OOM
  // (-fno-exceptions), and the extraction below transiently holds small buffers
  // plus a 32KB inflate window and ~11KB decoder state we'd rather not commit to
  // a doomed lookup.
  if (ESP.getMaxAllocHeap() < size + DEFINITION_HEAP_HEADROOM_BYTES) {
    LOG_ERR("DICT", "Low heap for %lu byte definition", static_cast<unsigned long>(size));
    return fail(LookupResult::LowMemory);
  }

  char pathBuf[PATH_BUF_BYTES];
  const char* path = pathBuf;
  uint32_t offset = 0;
  if (hasPlainDict) {
    if (!buildPath(pathBuf, sizeof(pathBuf), ".dict")) return fail(LookupResult::ReadError);
    offset = location.offset;
  } else {
    if (!buildPath(pathBuf, sizeof(pathBuf), ".dict.dz")) return fail(LookupResult::ReadError);
    HalFile tmp = Storage.open(DICT_TMP_FILE, O_WRITE | O_CREAT | O_TRUNC);
    if (!tmp) {
      LOG_ERR("DICT", "Failed to open %s", DICT_TMP_FILE);
      return fail(LookupResult::ReadError);
    }
    DictZip::ExtractError xerr = DictZip::ExtractError::None;
    if (!DictZip::extractEntry(pathBuf, location.offset, size, tmp, &xerr)) {
      // Map the specific extraction cause to the lookup result: allocation
      // failure (heap fragmentation) vs corrupt/truncated .dz vs an IO error.
      LOG_ERR("DICT", "dictzip extraction failed for %s (error %d)", basePath.c_str(), static_cast<int>(xerr));
      switch (xerr) {
        case DictZip::ExtractError::LowMemory:
          return fail(LookupResult::LowMemory);
        case DictZip::ExtractError::ReadError:
          return fail(LookupResult::ReadError);
        case DictZip::ExtractError::Decompress:
        default:
          return fail(LookupResult::Decompress);
      }
    }
    tmp.close();  // close before reopening the same path for read
    path = DICT_TMP_FILE;
  }

  HalFile dict;
  if (!Storage.openFileForRead("DICT", path, dict)) return fail(LookupResult::ReadError);
  const uint32_t dictSize = static_cast<uint32_t>(dict.fileSize());
  if (offset > dictSize || size > dictSize - offset) {
    LOG_ERR("DICT", "Definition out of bounds (%lu+%lu > %lu)", static_cast<unsigned long>(offset),
            static_cast<unsigned long>(size), static_cast<unsigned long>(dictSize));
    return fail(LookupResult::ReadError);
  }

  if (!dict.seekSet(offset)) {
    LOG_ERR("DICT", "Seek to %lu failed", static_cast<unsigned long>(offset));
    return fail(LookupResult::ReadError);
  }
  out.assign(size, '\0');
  // The bounds check above guarantees the bytes exist, so a short read is an IO
  // failure — surface it instead of returning a silently truncated definition.
  if (dict.read(&out[0], size) != static_cast<int>(size)) {
    out.clear();
    return fail(LookupResult::ReadError);
  }
  // Keep room for the viewer's plain-text fallback while retaining the input.
  if (htmlDefinitions && ESP.getMaxAllocHeap() < out.size() + DEFINITION_HEAP_HEADROOM_BYTES) {
    LOG_ERR("DICT", "Low heap for %lu byte definition fallback", static_cast<unsigned long>(out.size()));
    return fail(LookupResult::LowMemory);
  }
  return true;
}

std::string Dictionary::cleanWord(const char* word) {
  if (!word) return "";
  const size_t len = strlen(word);
  std::string result(len, '\0');
  result.resize(bb_dict_clean_word(word, len, &result[0], len));
  return result;
}

void Dictionary::stemVariants(const std::string& word, std::vector<std::string>& out) {
  out.clear();
  out.reserve(6);
  struct Context {
    const std::string& word;
    std::vector<std::string>& out;
  } context{word, out};
  bb_dict_stem_variants(
      word.data(), word.size(),
      [](void* c, const size_t stem, const char* ending, const size_t endingLen) {
        auto& [word, out] = *static_cast<Context*>(c);
        out.push_back(word.substr(0, stem).append(ending, endingLen));
      },
      &context);
}

bool Dictionary::lookup(const char* word, std::string& definitionOut, std::string& matchedHeadwordOut,
                        LookupResult* outResult) {
  const auto setResult = [outResult](LookupResult r) {
    if (outResult) *outResult = r;
  };
  setResult(LookupResult::NotFound);
  const std::string cleaned = cleanWord(word);
  if (cleaned.empty() || !isOpen()) return false;

  // One set of open handles for the exact-match probe, the synonym probe and
  // every stem variant, scoped so .idx/.qidx (and .syn/.sidx) close before
  // readDefinition() opens the data file.
  DictLocation location;
  bool searchFailed = false;
  {
    LookupSession session;
    // Couldn't open .idx: the search never reached a verdict, so this is a read
    // failure, not a miss.
    if (!openSession(session)) {
      setResult(LookupResult::ReadError);
      return false;
    }

    location = locate(session, cleaned.c_str(), &matchedHeadwordOut);
    searchFailed = location.readError;

    // Dictionary-authored synonyms (alternate spellings, irregular forms) take
    // precedence over the English-only stemmer, and are language-agnostic.
    if (!location.found && hasSyn) {
      location = locateSynonym(session, cleaned.c_str(), &matchedHeadwordOut);
      searchFailed = searchFailed || location.readError;
    }

    if (!location.found) {
      std::vector<std::string> variants;
      stemVariants(cleaned, variants);
      for (const auto& variant : variants) {
        location = locate(session, variant.c_str(), &matchedHeadwordOut);
        searchFailed = searchFailed || location.readError;
        if (location.found) break;
      }
    }
  }
  if (!location.found) {
    // A search that never reached a verdict (couldn't open or seek .idx) is a
    // read failure, not a miss — reporting "Not found" is the bug this PR exists
    // for. Otherwise the word is genuinely not in the dictionary.
    if (searchFailed) setResult(LookupResult::ReadError);
    return false;
  }

  // Found in the index — propagate the precise failure reason from readDefinition
  // (decompression / low memory / read error) so the caller can name it.
  if (readDefinition(location, definitionOut, outResult)) {
    setResult(LookupResult::Found);
    return true;
  }
  return false;
}
