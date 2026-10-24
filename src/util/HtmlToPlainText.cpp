#include "HtmlToPlainText.h"

#include <bembeyaz/html.h>

// bembeyaz-html's plain module; the text never grows longer than the HTML.
std::string htmlToPlainText(const std::string& html) {
  std::string output(html.size(), '\0');
  output.resize(bb_html_to_plain(html.data(), html.size(), output.data(), output.size()));
  return output;
}
