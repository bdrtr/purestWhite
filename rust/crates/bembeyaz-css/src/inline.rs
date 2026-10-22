//! The chapter parser's inline style stack (lib/Epub's `ChapterHtmlSlimParser`): which of an
//! element's declarations it pushes as an [`Entry`], and how the open entries fold, over the
//! block's own style, into the style its words are set in.

use crate::{
    Decoration, Defined, Direction, FontStyle, FontWeight, LengthField, Style, TextAlign, Unit,
    VerticalAlign,
};

/// One open element's inline style (`bb_css_inline_entry`): each value applies only with its
/// `has_` flag.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(clippy::struct_excessive_bools, reason = "the C ABI's flags")]
pub struct Entry {
    /// The element's depth, matched when it closes.
    pub depth: i32,
    pub has_bold: bool,
    pub bold: bool,
    pub has_italic: bool,
    pub italic: bool,
    pub has_decoration: bool,
    pub decoration: Decoration,
    pub has_direction: bool,
    pub direction: Direction,
    /// The direction is the paragraph's too (a block, or `html` and `body`).
    pub sets_paragraph_direction: bool,
    pub has_text_align: bool,
    pub text_align: TextAlign,
    pub has_sup: bool,
    pub sup: bool,
    pub has_sub: bool,
    pub sub: bool,
    pub has_small_caps: bool,
    pub small_caps: bool,
    pub has_white_space: bool,
    pub preserve_whitespace: bool,
    /// A size: a multiple of the parent's, or of the body's when `font_scale_rem`.
    pub has_font_scale: bool,
    pub font_scale_rem: bool,
    pub font_scale: f32,
}

impl Entry {
    /// Nothing set.
    pub const EMPTY: Self = Self {
        depth: 0,
        has_bold: false,
        bold: false,
        has_italic: false,
        italic: false,
        has_decoration: false,
        decoration: Decoration::NONE,
        has_direction: false,
        direction: Direction::Ltr,
        sets_paragraph_direction: false,
        has_text_align: false,
        text_align: TextAlign::Left,
        has_sup: false,
        sup: false,
        has_sub: false,
        sub: false,
        has_small_caps: false,
        small_caps: false,
        has_white_space: false,
        preserve_whitespace: false,
        has_font_scale: false,
        font_scale_rem: false,
        font_scale: 1.0,
    };
}

/// The element an entry is for, with what its declarations alone do not say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A block: pushed only when it sets a text property. `sized_ancestor`: an open entry has a
    /// size, so the block's resolved `scale` is pushed too, as a rem size.
    Block {
        sized_ancestor: bool,
        scale: f32,
    },
    /// `u`, `s` and the like: their line unless the style sets another.
    Decoration(Decoration),
    /// A footnote link: underlined.
    Link,
    /// `b`, `strong`: bold.
    Bold,
    /// `i`, `em`: italic.
    Italic,
    Sup,
    Sub,
    /// A `span` or other inline element: pushed only when it sets a text property.
    /// `sets_paragraph`: `html` or `body`. `in_table`: its text-align counts. `tag_scale`: the
    /// tag's own size (`small`, `big`), 0 for none.
    Inline {
        sets_paragraph: bool,
        in_table: bool,
        tag_scale: f32,
    },
}

fn has(css: &Style, bits: u32) -> bool {
    css.defined.has(bits)
}

#[inline(never)]
fn weight(css: &Style, e: &mut Entry) {
    if has(css, Defined::FONT_WEIGHT) {
        e.has_bold = true;
        e.bold = css.font_weight == FontWeight::Bold;
    }
}

#[inline(never)]
fn style(css: &Style, e: &mut Entry) {
    if has(css, Defined::FONT_STYLE) {
        e.has_italic = true;
        e.italic = css.font_style == FontStyle::Italic;
    }
}

#[inline(never)]
fn decoration(css: &Style, e: &mut Entry) {
    if has(css, Defined::TEXT_DECORATION) {
        e.has_decoration = true;
        e.decoration = css.decoration;
    }
}

fn sized(css: &Style) -> bool {
    has(css, Defined::length(LengthField::FontSize))
}

// Size, white-space and small caps.
#[inline(never)]
fn presentation(css: &Style, e: &mut Entry) {
    if sized(css) {
        let size = css.length(LengthField::FontSize);
        e.has_font_scale = true;
        e.font_scale_rem = size.unit == Unit::Rem;
        e.font_scale = size.value;
    }
    if has(css, Defined::WHITE_SPACE) {
        e.has_white_space = true;
        e.preserve_whitespace = css.preserve_whitespace;
    }
    if has(css, Defined::SMALL_CAPS) {
        e.has_small_caps = true;
        e.small_caps = css.small_caps;
    }
}

#[inline(never)]
fn direction(css: &Style, e: &mut Entry) {
    if has(css, Defined::DIRECTION) {
        e.has_direction = true;
        e.direction = css.direction;
    }
}

#[inline(never)]
fn vertical_align(css: &Style, e: &mut Entry) {
    if !has(css, Defined::VERTICAL_ALIGN) {
        return;
    }
    match css.vertical_align {
        VerticalAlign::Super => {
            e.has_sup = true;
            e.sup = true;
        }
        VerticalAlign::Sub => {
            e.has_sub = true;
            e.sub = true;
        }
        VerticalAlign::Baseline => {}
    }
}

// A block's entry into `e`; false when it sets no text property.
fn block(css: &Style, e: &mut Entry, sized_ancestor: bool, scale: f32) -> bool {
    let sized = sized(css) || sized_ancestor;
    let sets = Defined::FONT_WEIGHT
        | Defined::FONT_STYLE
        | Defined::TEXT_DECORATION
        | Defined::SMALL_CAPS
        | Defined::WHITE_SPACE
        | Defined::DIRECTION
        | Defined::TEXT_ALIGN;
    if !has(css, sets) && !sized {
        return false;
    }
    weight(css, e);
    style(css, e);
    decoration(css, e);
    presentation(css, e);
    if sized {
        // The block has already resolved its ancestors and its own declaration.
        e.has_font_scale = true;
        e.font_scale_rem = true;
        e.font_scale = scale;
    }
    direction(css, e);
    e.sets_paragraph_direction = true;
    if has(css, Defined::TEXT_ALIGN) {
        e.has_text_align = true;
        e.text_align = css.text_align;
    }
    true
}

// A span's (or other inline element's) entry into `e`; false when it sets no text property.
fn inline(
    css: &Style,
    e: &mut Entry,
    sets_paragraph: bool,
    in_table: bool,
    tag_scale: f32,
) -> bool {
    let table_align = in_table && has(css, Defined::TEXT_ALIGN);
    let own_size = sized(css);
    let sets = Defined::FONT_WEIGHT
        | Defined::FONT_STYLE
        | Defined::TEXT_DECORATION
        | Defined::SMALL_CAPS
        | Defined::WHITE_SPACE
        | Defined::DIRECTION
        | Defined::VERTICAL_ALIGN;
    if !own_size && tag_scale <= 0.0 && !has(css, sets) && !table_align {
        return false;
    }
    weight(css, e);
    style(css, e);
    decoration(css, e);
    presentation(css, e);
    direction(css, e);
    e.sets_paragraph_direction = sets_paragraph;
    if table_align {
        e.has_text_align = true;
        e.text_align = css.text_align;
    }
    vertical_align(css, e);
    if own_size || tag_scale > 0.0 {
        let size = css.length(LengthField::FontSize);
        e.has_font_scale = true;
        e.font_scale_rem = own_size && size.unit == Unit::Rem;
        e.font_scale = if own_size { size.value } else { tag_scale };
    }
    true
}

/// The entry an element at `depth` pushes, if any.
#[must_use]
pub fn entry(kind: Kind, css: &Style, depth: i32) -> Option<Entry> {
    let mut e = Entry {
        depth,
        ..Entry::EMPTY
    };
    match kind {
        Kind::Block {
            sized_ancestor,
            scale,
        } => {
            if !block(css, &mut e, sized_ancestor, scale) {
                return None;
            }
        }
        Kind::Decoration(line) => {
            e.has_decoration = true;
            e.decoration = if has(css, Defined::TEXT_DECORATION) {
                css.decoration
            } else {
                line
            };
            weight(css, &mut e);
            style(css, &mut e);
            presentation(css, &mut e);
            direction(css, &mut e);
        }
        Kind::Link => {
            e.has_decoration = true;
            e.decoration = Decoration::UNDERLINE;
            presentation(css, &mut e);
            direction(css, &mut e);
            vertical_align(css, &mut e);
        }
        Kind::Bold => {
            e.has_bold = true;
            e.bold = true;
            style(css, &mut e);
            decoration(css, &mut e);
            presentation(css, &mut e);
            direction(css, &mut e);
        }
        Kind::Italic => {
            e.has_italic = true;
            e.italic = true;
            weight(css, &mut e);
            decoration(css, &mut e);
            presentation(css, &mut e);
            direction(css, &mut e);
        }
        Kind::Sup | Kind::Sub => {
            if kind == Kind::Sup {
                e.has_sup = true;
                e.sup = true;
            } else {
                e.has_sub = true;
                e.sub = true;
            }
            presentation(css, &mut e);
        }
        Kind::Inline {
            sets_paragraph,
            in_table,
            tag_scale,
        } => {
            if !inline(css, &mut e, sets_paragraph, in_table, tag_scale) {
                return None;
            }
        }
    }
    Some(e)
}

/// The style words are set in (`bb_css_effective`), and the paragraph's direction.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools, reason = "the C ABI's flags")]
pub struct Effective {
    pub bold: bool,
    pub italic: bool,
    pub decoration: Decoration,
    pub small_caps: bool,
    pub preserve_whitespace: bool,
    pub direction_defined: bool,
    pub direction: Direction,
    pub text_align_defined: bool,
    pub text_align: TextAlign,
    pub sup: bool,
    pub sub: bool,
    pub paragraph_direction_defined: bool,
    pub paragraph_rtl: bool,
}

/// Folds the open entries, in order, over the block's style `css` and the paragraph direction it
/// starts from. A line decoration, once on, stays on; sup and sub replace each other.
#[must_use]
pub fn fold(
    css: &Style,
    block_direction_defined: bool,
    block_rtl: bool,
    stack: &[Entry],
) -> Effective {
    let mut out = Effective {
        bold: has(css, Defined::FONT_WEIGHT) && css.font_weight == FontWeight::Bold,
        italic: has(css, Defined::FONT_STYLE) && css.font_style == FontStyle::Italic,
        decoration: if has(css, Defined::TEXT_DECORATION) {
            css.decoration
        } else {
            Decoration::NONE
        },
        small_caps: has(css, Defined::SMALL_CAPS) && css.small_caps,
        preserve_whitespace: has(css, Defined::WHITE_SPACE) && css.preserve_whitespace,
        direction_defined: block_direction_defined,
        direction: if block_rtl {
            Direction::Rtl
        } else {
            Direction::Ltr
        },
        text_align_defined: has(css, Defined::TEXT_ALIGN),
        text_align: css.text_align,
        sup: false,
        sub: false,
        paragraph_direction_defined: block_direction_defined,
        paragraph_rtl: block_rtl,
    };
    for e in stack {
        if e.has_bold {
            out.bold = e.bold;
        }
        if e.has_italic {
            out.italic = e.italic;
        }
        // Line decorations propagate: a child adds its lines but cannot cancel an ancestor's.
        if e.has_decoration {
            out.decoration = Decoration(out.decoration.0 | e.decoration.0);
        }
        if e.has_white_space {
            out.preserve_whitespace = e.preserve_whitespace;
        }
        if e.has_small_caps {
            out.small_caps = e.small_caps;
        }
        if e.has_direction {
            out.direction_defined = true;
            out.direction = e.direction;
            if e.sets_paragraph_direction {
                out.paragraph_direction_defined = true;
                out.paragraph_rtl = e.direction == Direction::Rtl;
            }
        }
        if e.has_text_align {
            out.text_align_defined = true;
            out.text_align = e.text_align;
        }
        if e.has_sup {
            out.sup = e.sup;
            if e.sup {
                out.sub = false;
            }
        }
        if e.has_sub {
            out.sub = e.sub;
            if e.sub {
                out.sup = false;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]

    use super::{Effective, Entry, Kind, entry, fold};
    use crate::{
        Decoration, Defined, Direction, FontWeight, Length, LengthField, Style, Unit, VerticalAlign,
    };

    fn css(set: impl FnOnce(&mut Style)) -> Style {
        let mut s = Style::DEFAULT;
        set(&mut s);
        s
    }

    #[test]
    fn blocks_and_spans_push_only_what_they_set() {
        let plain = Style::DEFAULT;
        let block = Kind::Block {
            sized_ancestor: false,
            scale: 1.2,
        };
        let span = Kind::Inline {
            sets_paragraph: false,
            in_table: false,
            tag_scale: 0.0,
        };
        assert_eq!(entry(block, &plain, 3), None);
        assert_eq!(entry(span, &plain, 3), None);
        let small = entry(
            Kind::Inline {
                sets_paragraph: false,
                in_table: false,
                tag_scale: 0.83,
            },
            &plain,
            4,
        )
        .unwrap();
        assert_eq!(
            (small.depth, small.has_font_scale, small.font_scale),
            (4, true, 0.83)
        );
        assert!(!small.font_scale_rem);
        let sized_block = entry(
            Kind::Block {
                sized_ancestor: true,
                scale: 1.2,
            },
            &plain,
            2,
        )
        .unwrap();
        assert!(sized_block.font_scale_rem && sized_block.sets_paragraph_direction);
        assert_eq!(sized_block.font_scale, 1.2);
        let rem = css(|s| {
            s.set_length(
                LengthField::FontSize,
                Length {
                    value: 2.0,
                    unit: Unit::Rem,
                },
            );
            s.defined.set(Defined::length(LengthField::FontSize));
        });
        let e = entry(span, &rem, 1).unwrap();
        assert!(e.has_font_scale && e.font_scale_rem && e.font_scale == 2.0);
        // Text-align counts for a span only inside a table.
        let right = css(|s| s.defined.set(Defined::TEXT_ALIGN));
        assert_eq!(entry(span, &right, 1), None);
        assert!(
            entry(
                Kind::Inline {
                    sets_paragraph: false,
                    in_table: true,
                    tag_scale: 0.0
                },
                &right,
                1
            )
            .unwrap()
            .has_text_align
        );
    }

    #[test]
    fn tags_force_their_own_property() {
        let normal = css(|s| {
            s.font_weight = FontWeight::Normal;
            s.defined.set(Defined::FONT_WEIGHT);
        });
        let i = entry(Kind::Italic, &normal, 1).unwrap();
        assert!(i.italic && i.has_bold && !i.bold);
        let b = entry(Kind::Bold, &normal, 1).unwrap();
        assert!(b.bold);
        let u = entry(Kind::Decoration(Decoration::UNDERLINE), &Style::DEFAULT, 1).unwrap();
        assert_eq!(u.decoration, Decoration::UNDERLINE);
        let raised = css(|s| {
            s.vertical_align = VerticalAlign::Super;
            s.defined.set(Defined::VERTICAL_ALIGN);
        });
        let link = entry(Kind::Link, &raised, 1).unwrap();
        assert!(link.sup && link.decoration == Decoration::UNDERLINE);
        assert!(entry(Kind::Sub, &Style::DEFAULT, 1).unwrap().sub);
    }

    #[test]
    fn the_stack_folds_in_order() {
        let push = |set: fn(&mut Entry)| {
            let mut e = Entry::EMPTY;
            set(&mut e);
            e
        };
        let stack = [
            push(|e| {
                e.has_decoration = true;
                e.decoration = Decoration::UNDERLINE;
            }),
            push(|e| {
                e.has_decoration = true;
                e.decoration = Decoration::NONE;
                e.has_sup = true;
                e.sup = true;
            }),
            push(|e| {
                e.has_sub = true;
                e.sub = true;
                e.has_direction = true;
                e.direction = Direction::Rtl;
            }),
        ];
        let out: Effective = fold(&Style::DEFAULT, false, false, &stack);
        assert_eq!(out.decoration, Decoration::UNDERLINE);
        assert!(out.sub && !out.sup);
        assert!(out.direction_defined && out.direction == Direction::Rtl);
        assert!(!out.paragraph_direction_defined);
        let out = fold(&Style::DEFAULT, true, true, &stack[..1]);
        assert!(out.paragraph_rtl && out.direction == Direction::Rtl);
    }
}
