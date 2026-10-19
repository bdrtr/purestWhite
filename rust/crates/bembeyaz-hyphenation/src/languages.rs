//! The languages with patterns, and how a book's language tag picks one.

use crate::Breaks;
use crate::liang::{self, Trie};
use crate::text::{is_cyrillic_letter, is_latin_letter, to_lower_cyrillic, to_lower_latin};

/// A language with patterns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language(u8);

struct Entry {
    tag: &'static [u8],
    trie: Trie,
    cyrillic: bool,
    min_prefix: u8,
    min_suffix: u8,
}

const fn entry(tag: &'static [u8], file: &'static [u8], cyrillic: bool, min: u8) -> Entry {
    Entry {
        tag,
        trie: Trie::new(file),
        cyrillic,
        min_prefix: min,
        min_suffix: min,
    }
}

static LANGUAGES: [Entry; 11] = [
    // English breaks with three letters or more on each side.
    entry(b"en", include_bytes!("../tries/en.bin"), false, 3),
    entry(b"fr", include_bytes!("../tries/fr.bin"), false, 2),
    entry(b"de", include_bytes!("../tries/de.bin"), false, 2),
    entry(b"ru", include_bytes!("../tries/ru.bin"), true, 2),
    entry(b"es", include_bytes!("../tries/es.bin"), false, 2),
    entry(b"it", include_bytes!("../tries/it.bin"), false, 2),
    entry(b"pl", include_bytes!("../tries/pl.bin"), false, 2),
    entry(b"sv", include_bytes!("../tries/sv.bin"), false, 2),
    entry(b"uk", include_bytes!("../tries/uk.bin"), true, 2),
    entry(b"fi", include_bytes!("../tries/fi.bin"), false, 2),
    entry(b"pt", include_bytes!("../tries/pt.bin"), false, 2),
];

/// ISO 639-2 codes books use instead of the two-letter ones, bibliographic and terminological.
/// Polish and Portuguese (`pol`, `por`) are not here, as in the C++ code this replaces.
const THREE_LETTER: [(&[u8], &[u8]); 11] = [
    (b"eng", b"en"),
    (b"fra", b"fr"),
    (b"fre", b"fr"),
    (b"deu", b"de"),
    (b"ger", b"de"),
    (b"rus", b"ru"),
    (b"spa", b"es"),
    (b"ita", b"it"),
    (b"ukr", b"uk"),
    (b"swe", b"sv"),
    (b"fin", b"fi"),
];

/// The language of a BCP 47 or ISO 639-2 tag (`en`, `en-US`, `pt_BR`, `ger`), by its primary
/// subtag in ASCII lowercase; `None` when there are no patterns for it.
#[must_use]
pub fn language(tag: &[u8]) -> Option<Language> {
    let mut buf = [0u8; 8];
    let primary = tag
        .split(|&b| b == b'-' || b == b'_')
        .next()
        .unwrap_or_default();
    // Every known tag is shorter than this.
    let lower = buf.get_mut(..primary.len())?;
    for (dst, &src) in lower.iter_mut().zip(primary) {
        *dst = src.to_ascii_lowercase();
    }
    let lower: &[u8] = lower;
    let primary = THREE_LETTER
        .iter()
        .find(|&&(three, _)| three == lower)
        .map_or(lower, |&(_, two)| two);
    let index = LANGUAGES.iter().position(|e| e.tag == primary)?;
    Some(Language(u8::try_from(index).ok()?))
}

impl Language {
    /// The language for [`Language::index`].
    #[must_use]
    pub fn from_index(index: usize) -> Option<Self> {
        (index < LANGUAGES.len()).then(|| Language(u8::try_from(index).unwrap_or(0)))
    }

    /// A small number that stands for this language in this build: the order of the table, so
    /// never store it.
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    fn entry(self) -> Option<&'static Entry> {
        LANGUAGES.get(self.index())
    }

    pub(crate) fn minima(self) -> (usize, usize) {
        self.entry().map_or((0, 0), |e| {
            (usize::from(e.min_prefix), usize::from(e.min_suffix))
        })
    }

    /// The pattern breaks of a word of letters only.
    pub(crate) fn pattern_breaks(self, cps: &[u32]) -> Breaks {
        let Some(e) = self.entry() else {
            return Breaks::None;
        };
        let script = if e.cyrillic {
            liang::Script {
                is_letter: is_cyrillic_letter,
                lower: to_lower_cyrillic,
            }
        } else {
            liang::Script {
                is_letter: is_latin_letter,
                lower: to_lower_latin,
            }
        };
        let (min_prefix, min_suffix) = self.minima();
        match liang::breaks(&e.trie, cps, &script, min_prefix, min_suffix) {
            0 => Breaks::None,
            bits => Breaks::Patterns(bits),
        }
    }
}
