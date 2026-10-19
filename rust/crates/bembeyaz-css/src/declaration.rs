//! Declarations (`name: value`) and the values each property takes.

use crate::ascii::{is, strip_prefix, strip_suffix, trim, trim_end, words};
use crate::number::{parse_f32, parse_i32};
use crate::style::{
    BorderSide, BorderStyle, Decoration, Defined, Direction, Display, FontStyle, FontWeight,
    Length, LengthField, ListStyle, MAX_INITIAL_LETTER, Style, TextAlign, Unit, VerticalAlign,
};

/// A `style=""` attribute: declarations separated by `;`.
#[must_use]
pub fn parse_inline(declarations: &[u8]) -> Style {
    let mut style = Style::DEFAULT;
    for declaration in declarations.split(|&b| b == b';').filter(|d| !d.is_empty()) {
        apply_declaration(declaration, &mut style);
    }
    style
}

/// A number, after one optional `+`.
fn number(s: &[u8]) -> Option<f32> {
    parse_f32(s.strip_prefix(b"+").unwrap_or(s))
}

/// Splits `s` before the first byte that is not a digit or one of `extra`.
fn split_number<'a>(s: &'a [u8], extra: &[u8]) -> (&'a [u8], &'a [u8]) {
    let at = s
        .iter()
        .position(|b| !b.is_ascii_digit() && !extra.contains(b))
        .unwrap_or(s.len());
    s.split_at(at)
}

/// Up to four whitespace-separated values, for the box-edge shorthands.
fn edge_values(s: &[u8]) -> ([&[u8]; 4], usize) {
    let mut values = [&[][..]; 4];
    let mut count = 0;
    for (slot, word) in values.iter_mut().zip(words(s)) {
        *slot = word;
        count += 1;
    }
    (values, count)
}

/// Which of four edge values each side takes, top, right, bottom, left, when `count` are given.
fn edge_sources(count: usize) -> [usize; 4] {
    let right = usize::from(count >= 2);
    [
        0,
        right,
        if count >= 3 { 2 } else { 0 },
        if count >= 4 { 3 } else { right },
    ]
}

/// `!important` at the end, and the whitespace around it, removed.
fn strip_important(value: &[u8]) -> &[u8] {
    let value = trim_end(value);
    strip_suffix(value, "!important").map_or(value, trim_end)
}

pub(crate) fn apply_declaration(declaration: &[u8], style: &mut Style) {
    let Some(colon) = declaration.iter().position(|&b| b == b':') else {
        return;
    };
    if colon == 0 {
        return;
    }
    let (name, value) = declaration.split_at(colon);
    let name = trim(name);
    let value = trim(value.get(1..).unwrap_or_default());
    if name.is_empty() || value.is_empty() {
        return;
    }
    let value = strip_important(value);

    if let Some(field) = length_property(name) {
        style.set_length(field, length(value).unwrap_or(Length::ZERO));
    } else if is(name, "margin") || is(name, "padding") {
        let fields = if is(name, "margin") {
            MARGINS
        } else {
            PADDINGS
        };
        let (values, count) = edge_values(value);
        if count > 0 {
            for (field, source) in fields.into_iter().zip(edge_sources(count)) {
                let value = values.get(source).copied().unwrap_or_default();
                style.set_length(field, length(value).unwrap_or(Length::ZERO));
            }
        }
    } else if is(name, "height") || is(name, "width") {
        if let Some(length) = length(value) {
            let field = if is(name, "height") {
                LengthField::ImageHeight
            } else {
                LengthField::ImageWidth
            };
            style.set_length(field, length);
        }
    } else if is(name, "font-size") {
        if let Some(size) = font_size(value) {
            style.set_length(LengthField::FontSize, size);
        }
    } else if name.len() >= 6 && strip_prefix(name, "border").is_some() {
        border(name, value, style);
    } else {
        keyword_property(name, value, style);
    }
}

const MARGINS: [LengthField; 4] = [
    LengthField::MarginTop,
    LengthField::MarginRight,
    LengthField::MarginBottom,
    LengthField::MarginLeft,
];
const PADDINGS: [LengthField; 4] = [
    LengthField::PaddingTop,
    LengthField::PaddingRight,
    LengthField::PaddingBottom,
    LengthField::PaddingLeft,
];

/// Properties that take one length, or zero when the value is not one.
fn length_property(name: &[u8]) -> Option<LengthField> {
    const NAMES: [(&str, LengthField); 9] = [
        ("text-indent", LengthField::TextIndent),
        ("margin-top", LengthField::MarginTop),
        ("margin-bottom", LengthField::MarginBottom),
        ("margin-left", LengthField::MarginLeft),
        ("margin-right", LengthField::MarginRight),
        ("padding-top", LengthField::PaddingTop),
        ("padding-bottom", LengthField::PaddingBottom),
        ("padding-left", LengthField::PaddingLeft),
        ("padding-right", LengthField::PaddingRight),
    ];
    NAMES
        .iter()
        .find(|(n, _)| is(name, n))
        .map(|&(_, field)| field)
}

fn keyword_property(name: &[u8], value: &[u8], style: &mut Style) {
    if is(name, "white-space") {
        let preserve = is(value, "pre-wrap") || is(value, "pre") || is(value, "break-spaces");
        if preserve || is(value, "normal") {
            style.preserve_whitespace = preserve;
            style.defined.set(Defined::WHITE_SPACE);
        }
    } else if is(name, "text-align") {
        style.text_align = text_align(value);
        style.defined.set(Defined::TEXT_ALIGN);
    } else if is(name, "font-style") {
        let value = trim(value);
        style.font_style = if is(value, "italic") || is(value, "oblique") {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        style.defined.set(Defined::FONT_STYLE);
    } else if is(name, "font-weight") {
        style.font_weight = font_weight(value);
        style.defined.set(Defined::FONT_WEIGHT);
    } else if is(name, "text-decoration") || is(name, "text-decoration-line") {
        style.decoration = decoration(value);
        style.defined.set(Defined::TEXT_DECORATION);
    } else if is(name, "display") {
        style.display = if is(value, "none") {
            Display::None
        } else {
            Display::Block
        };
        style.defined.set(Defined::DISPLAY);
    } else if is(name, "direction") {
        let direction = [("rtl", Direction::Rtl), ("ltr", Direction::Ltr)]
            .into_iter()
            .find(|(k, _)| is(value, k));
        if let Some((_, direction)) = direction {
            style.direction = direction;
            style.defined.set(Defined::DIRECTION);
        }
    } else if is(name, "vertical-align") {
        let align = [("super", VerticalAlign::Super), ("sub", VerticalAlign::Sub)]
            .into_iter()
            .find(|(k, _)| is(value, k));
        if let Some((_, align)) = align {
            style.vertical_align = align;
            style.defined.set(Defined::VERTICAL_ALIGN);
        }
    } else if is(name, "font-variant") || is(name, "font-variant-caps") {
        style.small_caps = words(value).any(|w| is(w, "small-caps") || is(w, "all-small-caps"));
        style.defined.set(Defined::SMALL_CAPS);
    } else if is(name, "page-break-before") || is(name, "break-before") {
        style.page_break_before = forced_break(value);
        style.defined.set(Defined::PAGE_BREAK_BEFORE);
    } else if is(name, "page-break-after") || is(name, "break-after") {
        style.page_break_after = forced_break(value);
        style.defined.set(Defined::PAGE_BREAK_AFTER);
    } else if is(name, "background-color") || is(name, "background") {
        style.shaded = luminance(value).is_some_and(|l| (128..240).contains(&l));
        style.defined.set(Defined::SHADED);
    } else if is(name, "float") {
        style.float_left = is(value, "left");
        style.defined.set(Defined::FLOAT_LEFT);
    } else if is(name, "initial-letter") {
        style.initial_letter = initial_letter(value);
        style.defined.set(Defined::INITIAL_LETTER);
    } else if is(name, "list-style-type") {
        style.list_style = if is(strip_important(value), "none") {
            ListStyle::None
        } else {
            ListStyle::Disc
        };
        style.defined.set(Defined::LIST_STYLE);
    }
}

fn text_align(value: &[u8]) -> TextAlign {
    let value = trim(value);
    if is(value, "right") || is(value, "end") {
        TextAlign::Right
    } else if is(value, "center") {
        TextAlign::Center
    } else if is(value, "justify") {
        TextAlign::Justify
    } else {
        TextAlign::Left
    }
}

/// `bold`, `bolder` or a number of at least 700; everything else is normal.
fn font_weight(value: &[u8]) -> FontWeight {
    let value = trim(value);
    let bold = is(value, "bold")
        || is(value, "bolder")
        || (!is(value, "normal")
            && !is(value, "lighter")
            && parse_i32(value.strip_prefix(b"+").unwrap_or(value)).is_some_and(|w| w >= 700));
    if bold {
        FontWeight::Bold
    } else {
        FontWeight::Normal
    }
}

/// Words, so that `notunderline` draws nothing; `none` anywhere clears the rest.
fn decoration(value: &[u8]) -> Decoration {
    let mut lines = 0;
    for word in words(value) {
        if is(word, "none") {
            return Decoration::NONE;
        } else if is(word, "underline") {
            lines |= Decoration::UNDERLINE.0;
        } else if is(word, "line-through") {
            lines |= Decoration::LINE_THROUGH.0;
        }
    }
    Decoration(lines)
}

/// The `(page-)break-before/after` values that force a new page.
fn forced_break(value: &[u8]) -> bool {
    ["always", "page", "left", "right", "recto", "verso"]
        .iter()
        .any(|k| is(value, k))
}

/// A number with an optional unit: an unknown unit counts as pixels. `None` without a number
/// (`auto`, `inherit`, `initial`).
fn length(value: &[u8]) -> Option<Length> {
    let (digits, unit) = split_number(trim(value), b".-+");
    let value = number(digits)?;
    let unit = if is(unit, "em") {
        Unit::Em
    } else if is(unit, "rem") {
        Unit::Rem
    } else if is(unit, "pt") {
        Unit::Points
    } else if unit == b"%" {
        Unit::Percent
    } else {
        Unit::Pixels
    };
    Some(Length { value, unit })
}

/// A size relative to the parent (Em) or to the reader's body size (Rem). Absolute sizes count
/// from the CSS default body size: 16px = 12pt = 1rem.
fn font_size(value: &[u8]) -> Option<Length> {
    const KEYWORDS: [(&str, f32, Unit); 10] = [
        ("xx-small", 0.6, Unit::Rem),
        ("x-small", 0.75, Unit::Rem),
        ("small", 0.89, Unit::Rem),
        ("medium", 1.0, Unit::Rem),
        ("large", 1.2, Unit::Rem),
        ("x-large", 1.5, Unit::Rem),
        ("xx-large", 2.0, Unit::Rem),
        ("xxx-large", 3.0, Unit::Rem),
        ("smaller", 0.83, Unit::Em),
        ("larger", 1.2, Unit::Em),
    ];
    let value = trim(value);
    if let Some(&(_, value, unit)) = KEYWORDS.iter().find(|(k, _, _)| is(value, k)) {
        return Some(Length { value, unit });
    }
    let (digits, unit) = split_number(value, b".+-");
    let n = number(digits).filter(|&n| n > 0.0)?;
    let (value, unit) = if unit.is_empty() || is(unit, "px") {
        (n / 16.0, Unit::Rem)
    } else if is(unit, "pt") {
        (n / 12.0, Unit::Rem)
    } else if is(unit, "rem") {
        (n, Unit::Rem)
    } else if is(unit, "em") {
        (n, Unit::Em)
    } else if unit == b"%" {
        (n / 100.0, Unit::Em)
    } else if is(unit, "ex") || is(unit, "ch") {
        (n / 2.0, Unit::Em)
    } else {
        return None;
    };
    Some(Length { value, unit })
}

/// Lines of the first word that is a number of at least 1, rounded, up to the largest kept.
fn initial_letter(value: &[u8]) -> u8 {
    let lines = words(value)
        .find_map(|w| number(w).filter(|&n| n >= 1.0))
        .map_or(0, |n| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "saturates, as the device does"
            )]
            let lines = (n + 0.5) as u32;
            lines
        });
    #[allow(
        clippy::cast_possible_truncation,
        reason = "at most MAX_INITIAL_LETTER"
    )]
    let lines = lines.min(u32::from(MAX_INITIAL_LETTER)) as u8;
    lines
}

// ---- Borders ----

/// Marks a side's width or style, or both, as set.
fn mark(style: &mut Style, side: usize, width: bool, line: bool) {
    if width {
        style.border_widths |= 1 << side;
    }
    if line {
        style.border_styles |= 1 << side;
    }
    style
        .defined
        .set(Defined::BORDER.get(side).copied().unwrap_or(0));
}

fn border(name: &[u8], value: &[u8], style: &mut Style) {
    if is(name, "border") {
        let side = border_shorthand(value);
        for i in 0..4 {
            if let Some(slot) = style.borders.get_mut(i) {
                *slot = side;
            }
            mark(style, i, true, true);
        }
        return;
    }

    // border-width and border-style take one to four edge values.
    let widths = is(name, "border-width");
    if widths || is(name, "border-style") {
        let (values, count) = edge_values(value);
        if count == 0 {
            return;
        }
        for (i, source) in edge_sources(count).into_iter().enumerate() {
            let token = values.get(source).copied().unwrap_or_default();
            let Some(side) = style.borders.get_mut(i) else {
                continue;
            };
            let read = if widths {
                border_width(token).map(|w| side.width = w)
            } else {
                border_style(token).map(|s| side.style = s)
            };
            if read.is_some() {
                mark(style, i, widths, !widths);
            }
        }
        return;
    }

    // border-<side>, border-<side>-width, border-<side>-style
    let Some(rest) = name
        .get(7..)
        .filter(|r| name.get(6) == Some(&b'-') && !r.is_empty())
    else {
        return;
    };
    for (i, side_name) in ["top", "right", "bottom", "left"].into_iter().enumerate() {
        let Some(suffix) = strip_prefix(rest, side_name) else {
            continue;
        };
        let Some(side) = style.borders.get_mut(i) else {
            return;
        };
        if suffix.is_empty() {
            *side = border_shorthand(value);
            mark(style, i, true, true);
        } else if is(suffix, "-width")
            && let Some(width) = border_width(trim(value))
        {
            side.width = width;
            mark(style, i, true, false);
        } else if is(suffix, "-style")
            && let Some(line) = border_style(trim(value))
        {
            side.style = line;
            mark(style, i, false, true);
        }
        return;
    }
}

/// `1px solid #000`: the width and the style; colors are ignored (borders draw black).
fn border_shorthand(value: &[u8]) -> BorderSide {
    let mut side = BorderSide {
        width: 0,
        style: BorderStyle::None,
    };
    let mut has_width = false;
    for word in words(value) {
        if let Some(width) = border_width(word) {
            side.width = width;
            has_width = true;
        } else if let Some(line) = border_style(word) {
            side.style = line;
        }
    }
    if !has_width {
        side.width = 2; // medium
    }
    side
}

/// CSS pixels for `thin`, `medium`, `thick` or a length; hairlines stay visible and very thick
/// borders are capped.
fn border_width(token: &[u8]) -> Option<u8> {
    if let Some(i) = ["thin", "medium", "thick"]
        .iter()
        .position(|k| is(token, k))
    {
        return u8::try_from(i + 1).ok();
    }
    let (digits, unit) = split_number(token, b".");
    if digits.is_empty() {
        return None;
    }
    let n = number(digits)?;
    let n = if is(unit, "pt") {
        n * 4.0 / 3.0
    } else if is(unit, "em") || is(unit, "rem") {
        n * 16.0
    } else if unit.is_empty() || is(unit, "px") {
        n
    } else {
        return None;
    };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 1..=16"
    )]
    let width = if n <= 0.0 {
        0
    } else {
        (n + 0.5).clamp(1.0, 16.0) as u8
    };
    Some(width)
}

fn border_style(token: &[u8]) -> Option<BorderStyle> {
    const STYLES: [(&str, BorderStyle); 10] = [
        ("none", BorderStyle::None),
        ("hidden", BorderStyle::None),
        ("solid", BorderStyle::Solid),
        ("groove", BorderStyle::Solid),
        ("ridge", BorderStyle::Solid),
        ("inset", BorderStyle::Solid),
        ("outset", BorderStyle::Solid),
        ("double", BorderStyle::Double),
        ("dotted", BorderStyle::Dotted),
        ("dashed", BorderStyle::Dashed),
    ];
    STYLES.iter().find(|(k, _)| is(token, k)).map(|&(_, s)| s)
}

// ---- Background ----

/// Perceived brightness of the first color in a `background` value. Integer overflow wraps,
/// as it does on the device.
fn luminance(value: &[u8]) -> Option<i32> {
    let luminance = |[r, g, b]: [i32; 3]| {
        r.wrapping_mul(299)
            .wrapping_add(g.wrapping_mul(587))
            .wrapping_add(b.wrapping_mul(114))
            / 1000
    };
    if let Some(rgb) = value.windows(3).position(|w| w == b"rgb") {
        let mut channels = [0i32; 3];
        let mut count = 0;
        let mut current = -1i32;
        let after = value.get(rgb..).unwrap_or_default();
        let from = after.iter().position(|&b| b == b'(').map(|p| rgb + p);
        for &b in from.and_then(|f| value.get(f..)).unwrap_or_default() {
            if count == 3 {
                break;
            }
            if b.is_ascii_digit() {
                current = if current < 0 {
                    0
                } else {
                    current.wrapping_mul(10)
                }
                .wrapping_add(i32::from(b - b'0'));
            } else if current >= 0 {
                if let Some(channel) = channels.get_mut(count) {
                    *channel = if b == b'%' {
                        current.wrapping_mul(255) / 100
                    } else {
                        current.min(255)
                    };
                }
                count += 1;
                current = -1;
            }
        }
        return (count == 3).then(|| luminance(channels));
    }

    // The first word that is a color.
    for word in words(value) {
        if let [b'#', hex @ ..] = word
            && !hex.is_empty()
        {
            let nibble = |i: usize| hex.get(i).map_or(-1, |&b| hex_nibble(b));
            let channels: [i32; 3] = match hex.len() {
                3 | 4 => core::array::from_fn(|i| nibble(i) * 17),
                6 | 8 => core::array::from_fn(|i| nibble(2 * i) * 16 + nibble(2 * i + 1)),
                _ => [-1; 3],
            };
            if channels.iter().all(|&c| c >= 0) {
                return Some(luminance(channels));
            }
            continue;
        }
        if let Some(&(_, l)) = NAMED.iter().find(|(name, _)| is(word, name)) {
            return Some(l);
        }
    }
    None
}

fn hex_nibble(b: u8) -> i32 {
    match b.to_ascii_lowercase() {
        d @ b'0'..=b'9' => i32::from(d - b'0'),
        h @ b'a'..=b'f' => i32::from(h - b'a' + 10),
        _ => -1,
    }
}

const NAMED: [(&str, i32); 18] = [
    ("white", 255),
    ("whitesmoke", 245),
    ("gainsboro", 220),
    ("lightgray", 211),
    ("lightgrey", 211),
    ("silver", 192),
    ("darkgray", 169),
    ("darkgrey", 169),
    ("gray", 128),
    ("grey", 128),
    ("dimgray", 105),
    ("dimgrey", 105),
    ("black", 0),
    ("ivory", 254),
    ("beige", 243),
    ("linen", 244),
    ("lavender", 235),
    ("transparent", 255),
];
