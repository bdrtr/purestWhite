//! Unicode data for the scripts CrossPoint renders, converted entry for entry from the C tables
//! this crate replaces: `lib/MiniBidi/bidiclasses.t`, `bidi_pairs.t` and the shaping tables in
//! `minibidi.c`, as of fdd0780. Codepoints the tables leave out get `Class::ON`, joining type
//! `Join::None` and no presentation forms.

use crate::{Class, Join, Pair};
use Class::{
    AL, AN, B, BN, CS, EN, ES, ET, FSI, L, LRE, LRI, LRO, NSM, ON, PDF, PDI, R, RLE, RLI, RLO, S,
    WS,
};

/// Bidi classes of U+0000..U+007F.
pub(crate) static ASCII: [Class; 128] = [
    BN, BN, BN, BN, BN, BN, BN, BN, BN, S, B, S, WS, B, BN, BN, BN, BN, BN, BN, BN, BN, BN, BN, BN,
    BN, BN, BN, B, B, B, S, WS, ON, ON, ET, ET, ET, ON, ON, ON, ON, ON, ES, CS, ES, CS, CS, EN, EN,
    EN, EN, EN, EN, EN, EN, EN, EN, CS, ON, ON, ON, ON, ON, ON, L, L, L, L, L, L, L, L, L, L, L, L,
    L, L, L, L, L, L, L, L, L, L, L, L, L, L, ON, ON, ON, ON, ON, ON, L, L, L, L, L, L, L, L, L, L,
    L, L, L, L, L, L, L, L, L, L, L, L, L, L, L, L, ON, ON, ON, ON, BN,
];

/// Bidi classes of the rest of the BMP, as sorted, disjoint `(first, last, class)` ranges, from
/// DerivedBidiClass.txt (Unicode 17.0.0): Latin and Cyrillic (which must be L next to Hebrew),
/// Hebrew, Arabic and its presentation forms, punctuation and the directional formatting
/// characters. Other scripts are left out on purpose and fall to ON.
pub(crate) static RANGES: [(u16, u16, Class); 77] = [
    (0x0080, 0x0084, BN),
    (0x0085, 0x0085, B),
    (0x0086, 0x009F, BN),
    (0x00A0, 0x00A0, CS),
    (0x00A2, 0x00A5, ET),
    (0x00AA, 0x00AA, L),
    (0x00AD, 0x00AD, BN),
    (0x00B0, 0x00B1, ET),
    (0x00B2, 0x00B3, EN),
    (0x00B5, 0x00B5, L),
    (0x00B9, 0x00B9, EN),
    (0x00BA, 0x00BA, L),
    (0x00C0, 0x00D6, L),
    (0x00D8, 0x00F6, L),
    (0x00F8, 0x02B8, L),
    (0x0300, 0x036F, NSM),
    (0x0400, 0x04FF, L),
    (0x0500, 0x052F, L),
    (0x0591, 0x05A1, NSM),
    (0x05A3, 0x05B9, NSM),
    (0x05BB, 0x05BD, NSM),
    (0x05BE, 0x05BE, R),
    (0x05BF, 0x05BF, NSM),
    (0x05C0, 0x05C0, R),
    (0x05C1, 0x05C2, NSM),
    (0x05C3, 0x05C3, R),
    (0x05C4, 0x05C4, NSM),
    (0x05D0, 0x05EA, R),
    (0x05F0, 0x05F4, R),
    (0x0600, 0x0605, AN),
    (0x0608, 0x0608, AL),
    (0x0609, 0x060A, ET),
    (0x060B, 0x060B, AL),
    (0x060C, 0x060C, CS),
    (0x060D, 0x060D, AL),
    (0x0610, 0x061A, NSM),
    (0x061B, 0x061F, AL),
    (0x0620, 0x064A, AL),
    (0x064B, 0x065F, NSM),
    (0x0660, 0x0669, AN),
    (0x066A, 0x066A, ET),
    (0x066B, 0x066C, AN),
    (0x066D, 0x066F, AL),
    (0x0670, 0x0670, NSM),
    (0x0671, 0x06D5, AL),
    (0x06D6, 0x06DC, NSM),
    (0x06DD, 0x06DD, AN),
    (0x06DF, 0x06E4, NSM),
    (0x06E5, 0x06E6, AL),
    (0x06E7, 0x06E8, NSM),
    (0x06EA, 0x06ED, NSM),
    (0x06EE, 0x06EF, AL),
    (0x06F0, 0x06F9, EN),
    (0x06FA, 0x06FF, AL),
    (0x1E00, 0x1EFF, L),
    (0x200B, 0x200D, BN),
    (0x200E, 0x200E, L),
    (0x200F, 0x200F, R),
    (0x2028, 0x2028, WS),
    (0x2029, 0x2029, B),
    (0x202A, 0x202A, LRE),
    (0x202B, 0x202B, RLE),
    (0x202C, 0x202C, PDF),
    (0x202D, 0x202D, LRO),
    (0x202E, 0x202E, RLO),
    (0x202F, 0x202F, WS),
    (0x2060, 0x2063, BN),
    (0x2066, 0x2066, LRI),
    (0x2067, 0x2067, RLI),
    (0x2068, 0x2068, FSI),
    (0x2069, 0x2069, PDI),
    (0x206A, 0x206F, BN),
    (0xFB50, 0xFBC2, AL),
    (0xFBD3, 0xFBFF, AL),
    (0xFE70, 0xFE74, AL),
    (0xFE76, 0xFEFC, AL),
    (0xFEFF, 0xFEFF, BN),
];

/// Mirrored glyphs (rule L4) and the bracket pairs of rule N0, sorted by codepoint. Beyond
/// Unicode's tables (BidiMirroring.txt, BidiBrackets.txt), `<`/`>` and `‹`/`›` are bracket pairs
/// and the curly quotes mirror; the canonically equivalent brackets are left out.
pub(crate) static PAIRS: [(u16, u16, Pair); 16] = [
    (0x0028, 0x0029, Pair::Open),
    (0x0029, 0x0028, Pair::Close),
    (0x003C, 0x003E, Pair::Open),
    (0x003E, 0x003C, Pair::Close),
    (0x005B, 0x005D, Pair::Open),
    (0x005D, 0x005B, Pair::Close),
    (0x007B, 0x007D, Pair::Open),
    (0x007D, 0x007B, Pair::Close),
    (0x00AB, 0x00BB, Pair::Mirror),
    (0x00BB, 0x00AB, Pair::Mirror),
    (0x2018, 0x2019, Pair::Mirror),
    (0x2019, 0x2018, Pair::Mirror),
    (0x201C, 0x201D, Pair::Mirror),
    (0x201D, 0x201C, Pair::Mirror),
    (0x2039, 0x203A, Pair::Open),
    (0x203A, 0x2039, Pair::Close),
];

/// Joining type and isolated form of U+0621..U+064A, as in mintty: the isolated form is
/// `0xFE00 +` the byte (Arabic Presentation Forms-B), 0 when there is none. The final, initial
/// and medial forms follow it.
pub(crate) static ARABIC: [(Join, u8); 42] = [
    (Join::None, 0x80),   // U+0621
    (Join::Right, 0x81),  // U+0622
    (Join::Right, 0x83),  // U+0623
    (Join::Right, 0x85),  // U+0624
    (Join::Right, 0x87),  // U+0625
    (Join::Dual, 0x89),   // U+0626
    (Join::Right, 0x8D),  // U+0627
    (Join::Dual, 0x8F),   // U+0628
    (Join::Right, 0x93),  // U+0629
    (Join::Dual, 0x95),   // U+062A
    (Join::Dual, 0x99),   // U+062B
    (Join::Dual, 0x9D),   // U+062C
    (Join::Dual, 0xA1),   // U+062D
    (Join::Dual, 0xA5),   // U+062E
    (Join::Right, 0xA9),  // U+062F
    (Join::Right, 0xAB),  // U+0630
    (Join::Right, 0xAD),  // U+0631
    (Join::Right, 0xAF),  // U+0632
    (Join::Dual, 0xB1),   // U+0633
    (Join::Dual, 0xB5),   // U+0634
    (Join::Dual, 0xB9),   // U+0635
    (Join::Dual, 0xBD),   // U+0636
    (Join::Dual, 0xC1),   // U+0637
    (Join::Dual, 0xC5),   // U+0638
    (Join::Dual, 0xC9),   // U+0639
    (Join::Dual, 0xCD),   // U+063A
    (Join::None, 0x0),    // U+063B
    (Join::None, 0x0),    // U+063C
    (Join::None, 0x0),    // U+063D
    (Join::None, 0x0),    // U+063E
    (Join::None, 0x0),    // U+063F
    (Join::Causing, 0x0), // U+0640
    (Join::Dual, 0xD1),   // U+0641
    (Join::Dual, 0xD5),   // U+0642
    (Join::Dual, 0xD9),   // U+0643
    (Join::Dual, 0xDD),   // U+0644
    (Join::Dual, 0xE1),   // U+0645
    (Join::Dual, 0xE5),   // U+0646
    (Join::Dual, 0xE9),   // U+0647
    (Join::Right, 0xED),  // U+0648
    (Join::Right, 0xEF),  // U+0649
    (Join::Dual, 0xF1),   // U+064A
];

/// Joining types of the other Arabic-script letters and of ZWNJ/ZWJ, from
/// ArabicShaping.txt (Unicode 17.0.0), as sorted `(first, last, type)` ranges.
pub(crate) static JOINING: [(u16, u16, Join); 22] = [
    (0x0620, 0x0620, Join::Dual),
    (0x066E, 0x066F, Join::Dual),
    (0x0671, 0x0673, Join::Right),
    (0x0675, 0x0677, Join::Right),
    (0x0678, 0x0687, Join::Dual),
    (0x0688, 0x0699, Join::Right),
    (0x069A, 0x06BF, Join::Dual),
    (0x06C0, 0x06C0, Join::Right),
    (0x06C1, 0x06C2, Join::Dual),
    (0x06C3, 0x06CB, Join::Right),
    (0x06CC, 0x06CC, Join::Dual),
    (0x06CD, 0x06CD, Join::Right),
    (0x06CE, 0x06CE, Join::Dual),
    (0x06CF, 0x06CF, Join::Right),
    (0x06D0, 0x06D1, Join::Dual),
    (0x06D2, 0x06D3, Join::Right),
    (0x06D5, 0x06D5, Join::Right),
    (0x06EE, 0x06EF, Join::Right),
    (0x06FA, 0x06FC, Join::Dual),
    (0x06FF, 0x06FF, Join::Dual),
    (0x200C, 0x200C, Join::None),
    (0x200D, 0x200D, Join::Causing),
];

/// Arabic Presentation Forms-A of those letters, from UnicodeData.txt (Unicode 17.0.0):
/// `(letter, isolated form, form count)`, the count being 2 (isolated, final) or 4 (also
/// initial, medial). Sorted by letter; letters left out keep their codepoint.
pub(crate) static FORMS_A: [(u16, u16, u8); 39] = [
    (0x0671, 0xFB50, 2),
    (0x0679, 0xFB66, 4),
    (0x067A, 0xFB5E, 4),
    (0x067B, 0xFB52, 4),
    (0x067E, 0xFB56, 4),
    (0x067F, 0xFB62, 4),
    (0x0680, 0xFB5A, 4),
    (0x0683, 0xFB76, 4),
    (0x0684, 0xFB72, 4),
    (0x0686, 0xFB7A, 4),
    (0x0687, 0xFB7E, 4),
    (0x0688, 0xFB88, 2),
    (0x068C, 0xFB84, 2),
    (0x068D, 0xFB82, 2),
    (0x068E, 0xFB86, 2),
    (0x0691, 0xFB8C, 2),
    (0x0698, 0xFB8A, 2),
    (0x06A4, 0xFB6A, 4),
    (0x06A6, 0xFB6E, 4),
    (0x06A9, 0xFB8E, 4),
    (0x06AD, 0xFBD3, 4),
    (0x06AF, 0xFB92, 4),
    (0x06B1, 0xFB9A, 4),
    (0x06B3, 0xFB96, 4),
    (0x06BA, 0xFB9E, 2),
    (0x06BB, 0xFBA0, 4),
    (0x06BE, 0xFBAA, 4),
    (0x06C0, 0xFBA4, 2),
    (0x06C1, 0xFBA6, 4),
    (0x06C5, 0xFBE0, 2),
    (0x06C6, 0xFBD9, 2),
    (0x06C7, 0xFBD7, 2),
    (0x06C8, 0xFBDB, 2),
    (0x06C9, 0xFBE2, 2),
    (0x06CB, 0xFBDE, 2),
    (0x06CC, 0xFBFC, 4),
    (0x06D0, 0xFBE4, 4),
    (0x06D2, 0xFBAE, 2),
    (0x06D3, 0xFBB0, 2),
];
