use super::*;

/// The codepoints `visual` emits, without allocating.
struct Out {
    cps: [u32; 256],
    len: usize,
}

impl Out {
    fn new() -> Self {
        Self {
            cps: [0; 256],
            len: 0,
        }
    }
    fn push(&mut self, cp: u32) {
        self.cps[self.len] = cp;
        self.len += 1;
    }
    fn as_slice(&self) -> &[u32] {
        &self.cps[..self.len]
    }
}

fn visual_of(text: &str, level: Level) -> (Visual, Out) {
    let mut ws = Workspace::new();
    let mut out = Out::new();
    let result = visual(text.as_bytes(), level, &mut ws, |cp| out.push(cp));
    (result, out)
}

#[test]
fn classes() {
    assert_eq!(class(u32::from('a')), Class::L);
    assert_eq!(class(u32::from(' ')), Class::WS);
    assert_eq!(class(0x05D0), Class::R);
    assert_eq!(class(0x0627), Class::AL);
    assert_eq!(class(0x0661), Class::AN);
    assert_eq!(class(0x064E), Class::NSM);
    assert_eq!(class(0x2067), Class::RLI);
    assert_eq!(class(0x4E2D), Class::ON);
    assert_eq!(class(0x1F600), Class::ON);
    assert_eq!(class(u32::MAX), Class::ON);
}

/// `Char::class` is read once: mirroring and shaping must not change a character's class.
#[test]
fn mirroring_and_shaping_keep_the_class() {
    for &(from, to, _) in &tables::PAIRS {
        assert_eq!(class(from.into()), class(to.into()), "U+{from:04X}");
    }
    let letters = (0x0621..=0x064A).chain(
        tables::JOINING
            .iter()
            .flat_map(|&(first, last, _)| u32::from(first)..=u32::from(last)),
    );
    for cp in letters {
        if matches!(shape::joining(cp), Join::Right | Join::Dual) {
            for f in 0..4 {
                assert_eq!(class(shape::form(cp, f)), class(cp), "U+{cp:04X} form {f}");
            }
        }
    }
    for ligature in 0xFEF5..=0xFEFC {
        assert_eq!(class(ligature), class(0x0644));
    }
}

#[test]
fn hebrew_reverses_and_latin_stays() {
    let (r, out) = visual_of("abc שלום", Level::Auto);
    assert_eq!(r, Visual::Done);
    let expected: [u32; 8] = [
        'a' as u32, 'b' as u32, 'c' as u32, 0x20, 0x05DD, 0x05D5, 0x05DC, 0x05E9,
    ];
    assert_eq!(out.as_slice(), expected);
}

#[test]
fn empty_and_too_long() {
    assert_eq!(visual_of("", Level::Auto).0, Visual::Empty);
    let long = [b'a'; MAX_LINE + 1];
    let mut ws = Workspace::new();
    assert_eq!(
        visual(&long, Level::Auto, &mut ws, |_| panic!()),
        Visual::TooLong
    );
    assert_eq!(
        visual(&long[..MAX_LINE], Level::Auto, &mut ws, |_| {}),
        Visual::Done
    );
}

#[test]
fn first_strong() {
    assert_eq!(first_strong_is_rtl("123 ש".as_bytes(), 64), Some(true));
    assert_eq!(first_strong_is_rtl("12ש".as_bytes(), 2), None);
    assert_eq!(first_strong_is_rtl(b"...", 64), None);
    assert_eq!(first_strong_is_rtl(b"!a", 64), Some(false));
    assert_eq!(first_strong_is_rtl("ש".as_bytes(), 0), None);
}

#[test]
fn word_order_reverses_a_hebrew_line() {
    let words: [&[u8]; 2] = ["שלום".as_bytes(), "עולם".as_bytes()];
    let mut ws = Workspace::new();
    let mut order = [9u16; 2];
    assert_eq!(
        word_order(2, |i| words[i], true, &mut ws, &mut order),
        Some(true)
    );
    assert_eq!(order, [1, 0]);
    assert_eq!(word_order(1, |i| words[i], true, &mut ws, &mut order), None);
}

/// Every truncation and single-byte change of mixed text: no panic, and never more codepoints
/// out than characters in (the bound `bb_bidi_visual`'s buffer relies on).
#[test]
fn never_panics() {
    let text = "abc (שָׁלוֹם) 12,5% \u{200F}سلام لا ١٢ \u{202A}x\u{202C} \u{2067}יהוה\u{2069} \u{200D}ب\u{200C}ب".as_bytes();
    let mut ws = Workspace::new();
    let mut bytes = [0u8; 128];
    for len in 0..=text.len() {
        let chars = codepoints(&text[..len]).count();
        for level in [Level::Auto, Level::Ltr, Level::Rtl] {
            let mut n = 0;
            let _ = visual(&text[..len], level, &mut ws, |_| n += 1);
            assert!(n <= chars);
        }
    }
    bytes[..text.len()].copy_from_slice(text);
    for i in 0..text.len() {
        for value in [0x00, 0x80, 0xFF, 0x28, 0x29, 0x0A] {
            let original = bytes[i];
            bytes[i] = value;
            let _ = visual(&bytes[..text.len()], Level::Auto, &mut ws, |_| {});
            let words: [&[u8]; 3] = [&bytes[..i], &bytes[i..text.len()], b"x"];
            let mut order = [0u16; 3];
            let _ = word_order(3, |w| words[w], true, &mut ws, &mut order);
            bytes[i] = original;
        }
    }
}

/// L4: a character mirrors by its own level. In a right-to-left paragraph `(a` puts the bracket
/// at level 1 and `a` at level 2: drawn `a)`. (The C version tested the level of the logical
/// position each glyph landed on, and drew `a(`.)
#[test]
fn mirrors_by_the_characters_own_level() {
    let (r, out) = visual_of("(a", Level::Rtl);
    assert_eq!(r, Visual::Done);
    assert_eq!(out.as_slice(), [u32::from('a'), u32::from(')')]);
}
