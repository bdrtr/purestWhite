#include "StoreText.h"

#include <ObfuscationUtils.h>

bb_store_text ObfuscatedTexts::add(const std::string& plain) {
  texts_.push_back(obfuscation::obfuscateToBase64(plain));
  const String& text = texts_.back();
  // Only an empty text obfuscates to an empty one; anything else is a failed allocation.
  if (!plain.empty() && text.isEmpty()) ok_ = false;
  return storeText(text);
}

bool decodeStoredSecret(const char* data, const size_t len, const size_t most, const bb_store_write put,
                        void* putContext, bool* tooLong, void*) {
  bool ok = false;
  bool longer = false;
  std::string plain = obfuscation::deobfuscateFromBase64(storeString({data, len}).c_str(), most, &ok, &longer);
  if (ok) put(plain.data(), plain.size(), putContext);
  // Wiped through a volatile pointer, so the compiler keeps the stores before the string is freed.
  volatile char* bytes = plain.data();
  for (size_t i = 0; i < plain.size(); i++) bytes[i] = '\0';
  if (tooLong) *tooLong = longer;
  return ok;
}
