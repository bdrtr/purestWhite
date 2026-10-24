//! The properties a page uses, laid out as the C++ `CssStyle` (`lib/Epub/Epub/css/CssStyle.h`),
//! which reads them in place; and their byte form in the cache.

/// Matches `PARAGRAPH_ALIGNMENT` in the reader's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TextAlign {
    Justify = 0,
    Left = 1,
    Center = 2,
    Right = 3,
    None = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Unit {
    Pixels = 0,
    Em = 1,
    Rem = 2,
    Points = 3,
    Percent = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Direction {
    Ltr = 0,
    Rtl = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FontStyle {
    Normal = 0,
    Italic = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FontWeight {
    Normal = 0,
    Bold = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Display {
    Block = 0,
    None = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VerticalAlign {
    Baseline = 0,
    Super = 1,
    Sub = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ListStyle {
    Disc = 0,
    None = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BorderStyle {
    None = 0,
    Solid = 1,
    Double = 2,
    Dotted = 3,
    Dashed = 4,
}

/// Lines drawn through or under text: a set of the bits below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct Decoration(pub u8);

impl Decoration {
    pub const NONE: Self = Self(0);
    pub const UNDERLINE: Self = Self(1);
    pub const LINE_THROUGH: Self = Self(2);
    const ALL: u8 = 3;
}

/// A length in its unit, resolved to pixels when the page is laid out.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Length {
    pub value: f32,
    pub unit: Unit,
}

impl Length {
    /// In pixels: ems and rems of `em`, points, or a percentage of `width`.
    pub(crate) fn pixels(self, em: f32, width: f32) -> f32 {
        match self.unit {
            Unit::Em | Unit::Rem => self.value * em,
            Unit::Points => self.value * 1.33,
            Unit::Percent => self.value * width / 100.0,
            Unit::Pixels => self.value,
        }
    }

    pub const ZERO: Self = Self {
        value: 0.0,
        unit: Unit::Pixels,
    };
}

/// The same bits, which the cache compares: -0 is not 0.
impl PartialEq for Length {
    fn eq(&self, other: &Self) -> bool {
        self.value.to_bits() == other.value.to_bits() && self.unit == other.unit
    }
}

/// One box edge, in CSS pixels; it draws only with both a width and a style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct BorderSide {
    pub width: u8,
    pub style: BorderStyle,
}

/// The lengths in [`Style::lengths`], in their order there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthField {
    TextIndent,
    MarginTop,
    MarginBottom,
    MarginLeft,
    MarginRight,
    PaddingTop,
    PaddingBottom,
    PaddingLeft,
    PaddingRight,
    ImageHeight,
    ImageWidth,
    /// Em is relative to the parent's size, Rem to the reader's body size.
    FontSize,
}

/// The properties a stylesheet set: bit `i` is the C++ `CssPropertyFlags` field `i`
/// (little-endian, low bits first), as CssParser.cpp checks at compile time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct Defined(pub u32);

impl Defined {
    pub const TEXT_ALIGN: u32 = 1 << 0;
    pub const FONT_STYLE: u32 = 1 << 1;
    pub const FONT_WEIGHT: u32 = 1 << 2;
    pub const TEXT_DECORATION: u32 = 1 << 3;
    pub const DISPLAY: u32 = 1 << 15;
    pub const DIRECTION: u32 = 1 << 16;
    pub const VERTICAL_ALIGN: u32 = 1 << 17;
    pub const LIST_STYLE: u32 = 1 << 18;
    pub const SMALL_CAPS: u32 = 1 << 20;
    pub const PAGE_BREAK_BEFORE: u32 = 1 << 21;
    pub const PAGE_BREAK_AFTER: u32 = 1 << 22;
    /// Top, then right, bottom and left.
    pub const BORDER: [u32; 4] = [1 << 23, 1 << 24, 1 << 25, 1 << 26];
    pub const SHADED: u32 = 1 << 27;
    pub const FLOAT_LEFT: u32 = 1 << 28;
    pub const INITIAL_LETTER: u32 = 1 << 29;
    pub const WHITE_SPACE: u32 = 1 << 30;
    const ALL: u32 = (1 << 31) - 1;

    /// The bit of a length.
    #[must_use]
    pub const fn length(field: LengthField) -> u32 {
        match field {
            LengthField::FontSize => 1 << 19,
            // TextIndent .. ImageWidth are bits 4 .. 14.
            other => 1 << (other as u32 + 4),
        }
    }

    #[must_use]
    pub const fn has(self, bits: u32) -> bool {
        self.0 & bits != 0
    }

    pub fn set(&mut self, bits: u32) {
        self.0 |= bits;
    }
}

/// The properties of one element. Fields and order are those of `CssStyle`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Style {
    pub text_align: TextAlign,
    pub font_style: FontStyle,
    pub font_weight: FontWeight,
    pub decoration: Decoration,
    pub direction: Direction,
    /// Indexed by [`LengthField`].
    pub lengths: [Length; 12],
    pub display: Display,
    pub vertical_align: VerticalAlign,
    pub list_style: ListStyle,
    pub small_caps: bool,
    pub page_break_before: bool,
    pub page_break_after: bool,
    /// Top, right, bottom, left.
    pub borders: [BorderSide; 4],
    /// One bit per side, as in `borders`: which widths and styles were set.
    pub border_widths: u8,
    pub border_styles: u8,
    /// A light background, drawn as a gray fill.
    pub shaded: bool,
    /// `float: left`, which marks drop caps.
    pub float_left: bool,
    /// Lines a drop cap spans; 0 is none.
    pub initial_letter: u8,
    /// `white-space: pre`, `pre-wrap` or `break-spaces`.
    pub preserve_whitespace: bool,
    pub defined: Defined,
}

const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<Style>() == 128);
    assert!(offset_of!(Style, lengths) == 8);
    assert!(offset_of!(Style, display) == 104);
    assert!(offset_of!(Style, borders) == 110);
    assert!(offset_of!(Style, border_widths) == 118);
    assert!(offset_of!(Style, preserve_whitespace) == 123);
    assert!(offset_of!(Style, defined) == 124);
};

impl Default for Style {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Bytes of one style in the cache.
pub const WIRE_BYTES: usize = 5 + 12 * 5 + 6 + 13 + 4;

/// Largest `initial-letter` kept.
pub(crate) const MAX_INITIAL_LETTER: u8 = 6;

impl Style {
    /// The member initializers of `CssStyle` (`default_style_matches_the_cpp_one` checks).
    pub const DEFAULT: Self = Self {
        text_align: TextAlign::Left,
        font_style: FontStyle::Normal,
        font_weight: FontWeight::Normal,
        decoration: Decoration::NONE,
        direction: Direction::Ltr,
        lengths: [Length::ZERO; 12],
        display: Display::Block,
        vertical_align: VerticalAlign::Baseline,
        list_style: ListStyle::Disc,
        small_caps: false,
        page_break_before: false,
        page_break_after: false,
        borders: [BorderSide {
            width: 0,
            style: BorderStyle::None,
        }; 4],
        border_widths: 0,
        border_styles: 0,
        shaded: false,
        float_left: false,
        initial_letter: 0,
        preserve_whitespace: false,
        defined: Defined(0),
    };

    #[must_use]
    pub fn length(&self, field: LengthField) -> Length {
        self.lengths
            .get(field as usize)
            .copied()
            .unwrap_or(Length::ZERO)
    }

    /// Sets a length and marks it defined.
    pub fn set_length(&mut self, field: LengthField, length: Length) {
        if let Some(slot) = self.lengths.get_mut(field as usize) {
            *slot = length;
        }
        self.defined.set(Defined::length(field));
    }

    /// Takes every property `over` defines. Keep in step with `CssStyle::applyOver`, which the
    /// C++ callers still use.
    pub fn apply(&mut self, over: &Style) {
        let has = |bits| over.defined.has(bits);
        if has(Defined::TEXT_ALIGN) {
            self.text_align = over.text_align;
        }
        if has(Defined::FONT_STYLE) {
            self.font_style = over.font_style;
        }
        if has(Defined::FONT_WEIGHT) {
            self.font_weight = over.font_weight;
        }
        if has(Defined::TEXT_DECORATION) {
            self.decoration = over.decoration;
        }
        for (i, (mine, theirs)) in self.lengths.iter_mut().zip(&over.lengths).enumerate() {
            if has(Defined::length(
                LENGTH_FIELDS
                    .get(i)
                    .copied()
                    .unwrap_or(LengthField::FontSize),
            )) {
                *mine = *theirs;
            }
        }
        if has(Defined::DISPLAY) {
            self.display = over.display;
        }
        if has(Defined::DIRECTION) {
            self.direction = over.direction;
        }
        if has(Defined::VERTICAL_ALIGN) {
            self.vertical_align = over.vertical_align;
        }
        if has(Defined::LIST_STYLE) {
            self.list_style = over.list_style;
        }
        if has(Defined::SMALL_CAPS) {
            self.small_caps = over.small_caps;
        }
        if has(Defined::PAGE_BREAK_BEFORE) {
            self.page_break_before = over.page_break_before;
        }
        if has(Defined::PAGE_BREAK_AFTER) {
            self.page_break_after = over.page_break_after;
        }
        self.border_widths |= over.border_widths;
        self.border_styles |= over.border_styles;
        for (side, ((mine, theirs), bit)) in self
            .borders
            .iter_mut()
            .zip(&over.borders)
            .zip(Defined::BORDER)
            .enumerate()
        {
            if has(bit) {
                if over.border_widths & 1 << side != 0 {
                    mine.width = theirs.width;
                }
                if over.border_styles & 1 << side != 0 {
                    mine.style = theirs.style;
                }
            }
        }
        if has(Defined::SHADED) {
            self.shaded = over.shaded;
        }
        if has(Defined::FLOAT_LEFT) {
            self.float_left = over.float_left;
        }
        if has(Defined::WHITE_SPACE) {
            self.preserve_whitespace = over.preserve_whitespace;
        }
        if has(Defined::INITIAL_LETTER) {
            self.initial_letter = over.initial_letter;
        }
        self.defined.set(over.defined.0);
    }

    /// The cache's form: enums and flags as bytes, lengths as little-endian `f32` and a unit.
    #[must_use]
    pub fn to_wire(&self) -> [u8; WIRE_BYTES] {
        let mut out = [0u8; WIRE_BYTES];
        let mut w = Writer {
            out: &mut out,
            at: 0,
        };
        w.bytes(&[
            self.text_align as u8,
            self.font_style as u8,
            self.font_weight as u8,
            self.decoration.0,
            self.direction as u8,
        ]);
        for length in &self.lengths {
            w.bytes(&length.value.to_le_bytes());
            w.bytes(&[length.unit as u8]);
        }
        w.bytes(&[
            self.display as u8,
            self.vertical_align as u8,
            self.list_style as u8,
            u8::from(self.small_caps),
            u8::from(self.page_break_before),
            u8::from(self.page_break_after),
        ]);
        for side in &self.borders {
            w.bytes(&[side.width, side.style as u8]);
        }
        w.bytes(&[
            self.border_widths | self.border_styles << 4,
            u8::from(self.shaded),
            u8::from(self.float_left),
            self.initial_letter,
            u8::from(self.preserve_whitespace),
        ]);
        w.bytes(&self.defined.0.to_le_bytes());
        out
    }

    /// Reads [`Style::to_wire`]'s bytes; `None` for any value a style cannot hold.
    #[must_use]
    pub fn from_wire(wire: &[u8; WIRE_BYTES]) -> Option<Self> {
        let mut r = Reader { rest: wire };
        let mut style = Style {
            text_align: text_align(r.byte()?)?,
            font_style: [FontStyle::Normal, FontStyle::Italic]
                .get(usize::from(r.byte()?))
                .copied()?,
            font_weight: [FontWeight::Normal, FontWeight::Bold]
                .get(usize::from(r.byte()?))
                .copied()?,
            decoration: Some(r.byte()?)
                .filter(|d| d & !Decoration::ALL == 0)
                .map(Decoration)?,
            direction: [Direction::Ltr, Direction::Rtl]
                .get(usize::from(r.byte()?))
                .copied()?,
            ..Style::DEFAULT
        };
        for length in &mut style.lengths {
            let value = f32::from_le_bytes(r.array()?);
            let unit = UNITS.get(usize::from(r.byte()?)).copied()?;
            if !value.is_finite() {
                return None;
            }
            *length = Length { value, unit };
        }
        style.display = [Display::Block, Display::None]
            .get(usize::from(r.byte()?))
            .copied()?;
        style.vertical_align = [
            VerticalAlign::Baseline,
            VerticalAlign::Super,
            VerticalAlign::Sub,
        ]
        .get(usize::from(r.byte()?))
        .copied()?;
        style.list_style = [ListStyle::Disc, ListStyle::None]
            .get(usize::from(r.byte()?))
            .copied()?;
        style.small_caps = r.flag()?;
        style.page_break_before = r.flag()?;
        style.page_break_after = r.flag()?;
        for side in &mut style.borders {
            side.width = r.byte()?;
            side.style = BORDER_STYLES.get(usize::from(r.byte()?)).copied()?;
        }
        let components = r.byte()?;
        style.border_widths = components & 0x0F;
        style.border_styles = components >> 4;
        style.shaded = r.flag()?;
        style.float_left = r.flag()?;
        style.initial_letter = Some(r.byte()?).filter(|&n| n <= MAX_INITIAL_LETTER)?;
        style.preserve_whitespace = r.flag()?;
        style.defined = Some(u32::from_le_bytes(r.array()?))
            .filter(|d| d & !Defined::ALL == 0)
            .map(Defined)?;
        Some(style)
    }
}

const LENGTH_FIELDS: [LengthField; 12] = [
    LengthField::TextIndent,
    LengthField::MarginTop,
    LengthField::MarginBottom,
    LengthField::MarginLeft,
    LengthField::MarginRight,
    LengthField::PaddingTop,
    LengthField::PaddingBottom,
    LengthField::PaddingLeft,
    LengthField::PaddingRight,
    LengthField::ImageHeight,
    LengthField::ImageWidth,
    LengthField::FontSize,
];
const UNITS: [Unit; 5] = [
    Unit::Pixels,
    Unit::Em,
    Unit::Rem,
    Unit::Points,
    Unit::Percent,
];
const BORDER_STYLES: [BorderStyle; 5] = [
    BorderStyle::None,
    BorderStyle::Solid,
    BorderStyle::Double,
    BorderStyle::Dotted,
    BorderStyle::Dashed,
];

fn text_align(b: u8) -> Option<TextAlign> {
    [
        TextAlign::Justify,
        TextAlign::Left,
        TextAlign::Center,
        TextAlign::Right,
        TextAlign::None,
    ]
    .get(usize::from(b))
    .copied()
}

struct Writer<'a> {
    out: &'a mut [u8; WIRE_BYTES],
    at: usize,
}

impl Writer<'_> {
    fn bytes(&mut self, bytes: &[u8]) {
        if let Some(slot) = self.out.get_mut(self.at..self.at + bytes.len()) {
            slot.copy_from_slice(bytes);
        }
        self.at += bytes.len();
    }
}

struct Reader<'a> {
    rest: &'a [u8],
}

impl Reader<'_> {
    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.rest.split_first_chunk::<N>()?;
        self.rest = rest;
        Some(*head)
    }

    fn byte(&mut self) -> Option<u8> {
        self.array::<1>().map(|[b]| b)
    }

    fn flag(&mut self) -> Option<bool> {
        match self.byte()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
}
