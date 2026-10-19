#include "HyphenationCommon.h"

#include <bembeyaz/hyphenation.h>

bool isExplicitHyphen(const uint32_t cp) { return bb_hyph_is_explicit_hyphen(cp); }

bool isSoftHyphen(const uint32_t cp) { return cp == 0x00AD; }
