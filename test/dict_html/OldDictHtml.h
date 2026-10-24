// src/util/DictHtmlPages.cpp's tag-soup normalizer at 707d5e38 (BufferedFileWriter, isVoidElement,
// isEntityRef, writeNormalizedXhtml), verbatim but for the namespace, over a file in memory: the
// oracle for bembeyaz-html's xhtml module.
#pragma once

#include <algorithm>
#include <cctype>
#include <cstddef>
#include <cstring>
#include <iterator>
#include <string>
#include <vector>

// A file in memory that takes at most `room` bytes before writes come up short.
struct HalFile {
  std::vector<uint8_t> bytes;
  size_t room = SIZE_MAX;
  size_t write(const void* data, size_t len) {
    const size_t n = std::min(len, room - std::min(room, bytes.size()));
    bytes.insert(bytes.end(), static_cast<const uint8_t*>(data), static_cast<const uint8_t*>(data) + n);
    return n;
  }
};

namespace oldDictHtml {

class BufferedFileWriter {
 public:
  explicit BufferedFileWriter(HalFile& file) : file(file) {}

  bool append(const char c) { return append(&c, 1); }

  bool append(const char* data, size_t len) {
    while (len > 0) {
      const size_t available = sizeof(buffer) - used;
      const size_t chunk = std::min(available, len);
      memcpy(buffer + used, data, chunk);
      used += chunk;
      data += chunk;
      len -= chunk;
      if (used == sizeof(buffer) && !flush()) return false;
    }
    return true;
  }

  bool append(const char* text) { return append(text, strlen(text)); }

  bool flush() {
    if (used == 0) return true;
    if (file.write(buffer, used) != used) return false;
    used = 0;
    return true;
  }

 private:
  HalFile& file;
  // Fixed stack staging avoids an expansion-sized XHTML heap allocation.
  char buffer[128] = {};
  size_t used = 0;
};

// HTML void elements: legal without a closing tag in HTML, but must be
// self-closed to be well-formed XML.
bool isVoidElement(const char* name, const size_t len) {
  static constexpr const char* VOID_ELEMENTS[] = {"area",  "base", "br",   "col",   "embed",  "hr",    "img",
                                                  "input", "link", "meta", "param", "source", "track", "wbr"};
  return std::any_of(std::begin(VOID_ELEMENTS), std::end(VOID_ELEMENTS),
                     [name, len](const char* v) { return strlen(v) == len && strncmp(v, name, len) == 0; });
}

// True for a well-formed entity reference at html[pos] ('&'): &name; &#123;
// or &#x1F;. On success *end is the index of the ';'.
bool isEntityRef(const std::string& html, const size_t pos, size_t* end) {
  size_t j = pos + 1;
  const size_t n = html.size();
  if (j < n && html[j] == '#') {
    j++;
    if (j < n && (html[j] == 'x' || html[j] == 'X')) j++;
    const size_t digits = j;
    while (j < n && std::isxdigit(static_cast<unsigned char>(html[j]))) j++;
    if (j == digits) return false;
  } else {
    const size_t letters = j;
    while (j < n && std::isalnum(static_cast<unsigned char>(html[j]))) j++;
    if (j == letters) return false;
  }
  if (j >= n || html[j] != ';') return false;
  *end = j;
  return true;
}

// StarDict HTML is tag soup; the XML parser is strict. Produce a
// well-formed XHTML document from the fragment: wrap it in a root element,
// lowercase tag names (XML is case-sensitive and the parser matches
// lowercase), self-close void elements (<br> → <br/>), drop stray void
// closers (</br>) and <!…>/<?…> constructs, and escape '&'/'<' characters
// that are not part of markup. Structural damage this cannot repair
// (mismatched tags, unquoted attribute values) surfaces as a parse error and
// the caller falls back to the plain-text path.
bool writeNormalizedXhtml(const std::string& html, HalFile& file) {
  BufferedFileWriter out(file);
  if (!out.append("<html><body>")) return false;

  const size_t n = html.size();
  size_t i = 0;
  while (i < n) {
    const char c = html[i];
    if (c == '<' && i + 1 < n && (html[i + 1] == '!' || html[i + 1] == '?')) {
      // Comment, doctype or processing instruction: drop it entirely.
      const bool isComment = html.compare(i, 4, "<!--") == 0;
      const size_t j = isComment ? html.find("-->", i + 4) : html.find('>', i);
      i = (j == std::string::npos) ? n : j + (isComment ? 3 : 1);
      continue;
    }
    if (c == '<' && i + 1 < n && (html[i + 1] == '/' || std::isalpha(static_cast<unsigned char>(html[i + 1])))) {
      // Find the tag end, honouring quoted attribute values.
      size_t j = i + 1;
      char quote = 0;
      while (j < n) {
        const char d = html[j];
        if (quote) {
          if (d == quote) quote = 0;
        } else if (d == '"' || d == '\'') {
          quote = d;
        } else if (d == '>') {
          break;
        }
        j++;
      }
      if (j == n) {  // unterminated tag: treat the '<' as literal text
        if (!out.append("&lt;")) return false;
        i++;
        continue;
      }

      const bool closing = html[i + 1] == '/';
      const size_t nameStart = i + (closing ? 2 : 1);
      size_t nameEnd = nameStart;
      char nameBuf[16] = {};
      size_t nameLen = 0;
      while (nameEnd < j && std::isalnum(static_cast<unsigned char>(html[nameEnd]))) {
        if (nameLen < sizeof(nameBuf) - 1) {
          nameBuf[nameLen++] = static_cast<char>(std::tolower(static_cast<unsigned char>(html[nameEnd])));
        }
        nameEnd++;
      }
      const bool isVoid = isVoidElement(nameBuf, nameLen);
      if (closing && isVoid) {  // "</br>" — no XML equivalent, drop it
        i = j + 1;
        continue;
      }
      if (!out.append('<')) return false;
      if (closing && !out.append('/')) return false;
      if (!out.append(nameBuf, nameLen)) return false;
      if (!out.append(html.data() + nameEnd, j - nameEnd)) return false;  // attributes verbatim
      if (!closing && isVoid && html[j - 1] != '/' && !out.append('/')) return false;
      if (!out.append('>')) return false;
      i = j + 1;
      continue;
    }
    if (c == '<') {  // stray '<' in text ("x < y")
      if (!out.append("&lt;")) return false;
      i++;
      continue;
    }
    if (c == '&') {
      size_t entityEnd = 0;
      if (isEntityRef(html, i, &entityEnd)) {
        if (!out.append(html.data() + i, entityEnd - i + 1)) return false;
        i = entityEnd + 1;
      } else {  // bare ampersand ("Tom & Jerry")
        if (!out.append("&amp;")) return false;
        i++;
      }
      continue;
    }
    if (!out.append(c)) return false;
    i++;
  }

  return out.append("</body></html>") && out.flush();
}

}  // namespace oldDictHtml
