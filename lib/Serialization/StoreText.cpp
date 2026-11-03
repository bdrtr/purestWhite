#include "StoreText.h"

#include <ObfuscationUtils.h>

bb_store_text ObfuscatedTexts::add(const std::string& plain) {
  texts_.push_back(obfuscation::obfuscateToBase64(plain));
  const String& text = texts_.back();
  // Only an empty text obfuscates to an empty one; anything else is a failed allocation.
  if (!plain.empty() && text.isEmpty()) ok_ = false;
  return storeText(text);
}
