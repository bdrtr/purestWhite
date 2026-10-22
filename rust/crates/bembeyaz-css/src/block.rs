//! A block's layout style (lib/Epub's `BlockStyle`): its CSS lengths resolved to pixels, and how
//! nested blocks combine. Layout (`bembeyaz-layout`) and the section file (`bembeyaz-section`)
//! keep their own parts of it.

use crate::{Defined, Direction, Length, LengthField, Style, TextAlign, Unit};

/// The C++ `BlockStyle`, field for field; both sides check the layout.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
#[allow(clippy::struct_excessive_bools)]
pub struct Block {
    /// A `TextAlign`.
    pub alignment: u8,
    pub character_spacing: i8,
    pub margin_top: i16,
    pub margin_bottom: i16,
    pub margin_left: i16,
    pub margin_right: i16,
    pub padding_top: i16,
    pub padding_bottom: i16,
    pub padding_left: i16,
    pub padding_right: i16,
    pub text_indent: i16,
    pub text_indent_defined: bool,
    pub text_align_defined: bool,
    pub is_rtl: bool,
    pub direction_defined: bool,
    pub font_scale: f32,
    pub font_scale_defined: bool,
    pub font_id: i32,
    pub page_break_before: bool,
    pub page_break_after: bool,
    pub keep_with_next: bool,
}

impl Block {
    pub const DEFAULT: Self = Self {
        alignment: TextAlign::Justify as u8,
        character_spacing: 0,
        margin_top: 0,
        margin_bottom: 0,
        margin_left: 0,
        margin_right: 0,
        padding_top: 0,
        padding_bottom: 0,
        padding_left: 0,
        padding_right: 0,
        text_indent: 0,
        text_indent_defined: false,
        text_align_defined: false,
        is_rtl: false,
        direction_defined: false,
        font_scale: 1.0,
        font_scale_defined: false,
        font_id: 0,
        page_break_before: false,
        page_break_after: false,
        keep_with_next: false,
    };
}

/// The axis two nested blocks combine along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Side margins and padding add up; the indent and alignment are inherited.
    Horizontal,
    /// Top and bottom margins collapse to the larger, padding adds up, a page break carries.
    Vertical,
}

/// No single side's horizontal margin or padding is wider than this many ems, so huge insets on
/// chapter openers keep lines of more than a word or two.
const MAX_HORIZONTAL_INSET_EM: f32 = 2.0;

/// `length` in pixels: ems and rems of `em`, points, or a percentage of `width`.
fn pixels(length: Length, em: f32, width: f32) -> f32 {
    match length.unit {
        Unit::Em | Unit::Rem => length.value * em,
        Unit::Points => length.value * 1.33,
        Unit::Percent => length.value * width / 100.0,
        Unit::Pixels => length.value,
    }
}

/// `px` toward zero, kept to 16 bits as the C++ cast does for finite values.
#[allow(clippy::cast_possible_truncation)]
fn px16(px: f32) -> i16 {
    px as i32 as i16
}

/// `css`'s `field` in whole pixels; out of line: `from_css` calls it nine times.
#[inline(never)]
fn field_px(css: &Style, field: LengthField, em: f32, width: f32) -> i16 {
    px16(pixels(css.length(field), em, width))
}

/// `css` as a block: its lengths in pixels over `em` (the font's line height) and `viewport` (0
/// when unknown, so percentages are 0 and a percentage indent is left to the paragraph), the
/// horizontal insets capped, and `paragraph_alignment` (a `TextAlign`; `None` keeps the book's).
/// The alignment stays a byte, as the C++ setting and `Block::alignment` are, so any value passes.
#[must_use]
pub fn from_css(css: &Style, em: f32, paragraph_alignment: u8, viewport: u16) -> Block {
    let width = f32::from(viewport);
    let px = |field| field_px(css, field, em, width);
    let max_inset = px16(em * MAX_HORIZONTAL_INSET_EM);
    let mut b = Block::DEFAULT;
    b.margin_top = px(LengthField::MarginTop);
    b.margin_bottom = px(LengthField::MarginBottom);
    b.margin_left = px(LengthField::MarginLeft).min(max_inset);
    b.margin_right = px(LengthField::MarginRight).min(max_inset);
    b.padding_top = px(LengthField::PaddingTop);
    b.padding_bottom = px(LengthField::PaddingBottom);
    b.padding_left = px(LengthField::PaddingLeft).min(max_inset);
    b.padding_right = px(LengthField::PaddingRight).min(max_inset);
    let indent = css.length(LengthField::TextIndent);
    if css.defined.has(Defined::length(LengthField::TextIndent))
        && (indent.unit != Unit::Percent || width > 0.0)
    {
        b.text_indent = px(LengthField::TextIndent);
        b.text_indent_defined = true;
    }
    b.text_align_defined = css.defined.has(Defined::TEXT_ALIGN);
    b.alignment = if paragraph_alignment != TextAlign::None as u8 {
        paragraph_alignment
    } else if b.text_align_defined {
        css.text_align as u8
    } else {
        TextAlign::Justify as u8
    };
    b.page_break_before = css.defined.has(Defined::PAGE_BREAK_BEFORE) && css.page_break_before;
    b.page_break_after = css.defined.has(Defined::PAGE_BREAK_AFTER) && css.page_break_after;
    if css.defined.has(Defined::DIRECTION) {
        b.is_rtl = css.direction == Direction::Rtl;
        b.direction_defined = true;
    }
    b
}

/// `child` put inside `parent` along `axis`; its fields on the other axis stay. Direction and
/// font size it leaves open come from the parent on either axis.
pub fn combine(parent: &Block, child: &mut Block, axis: Axis) {
    match axis {
        Axis::Horizontal => {
            child.margin_left = child.margin_left.wrapping_add(parent.margin_left);
            child.margin_right = child.margin_right.wrapping_add(parent.margin_right);
            child.padding_left = child.padding_left.wrapping_add(parent.padding_left);
            child.padding_right = child.padding_right.wrapping_add(parent.padding_right);
            if !child.text_indent_defined && parent.text_indent_defined {
                child.text_indent = parent.text_indent;
                child.text_indent_defined = true;
            }
            if !child.text_align_defined && parent.text_align_defined {
                child.alignment = parent.alignment;
                child.text_align_defined = true;
            }
        }
        Axis::Vertical => {
            child.margin_top = child.margin_top.max(parent.margin_top);
            child.margin_bottom = child.margin_bottom.max(parent.margin_bottom);
            child.padding_top = child.padding_top.wrapping_add(parent.padding_top);
            child.padding_bottom = child.padding_bottom.wrapping_add(parent.padding_bottom);
            // A container's break lands on its first child when they share the empty block.
            child.page_break_before |= parent.page_break_before;
        }
    }
    if !child.direction_defined && parent.direction_defined {
        child.is_rtl = parent.is_rtl;
        child.direction_defined = true;
    }
    if !child.font_scale_defined && parent.font_scale_defined {
        child.font_scale = parent.font_scale;
        child.font_scale_defined = true;
    }
}

/// `source`'s bottom collapsed into `block`'s: the larger margin, the padding added.
pub fn add_bottom(block: &mut Block, source: &Block) {
    block.margin_bottom = block.margin_bottom.max(source.margin_bottom);
    block.padding_bottom = block.padding_bottom.wrapping_add(source.padding_bottom);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]

    use super::{Axis, Block, add_bottom, combine, from_css, px16};
    use crate::{Defined, Direction, Length, LengthField, Style, TextAlign, Unit};

    fn set(css: &mut Style, field: LengthField, value: f32, unit: Unit) {
        css.set_length(field, Length { value, unit });
    }

    #[test]
    fn lengths_resolve_toward_zero_and_side_insets_are_capped() {
        let mut css = Style::DEFAULT;
        set(&mut css, LengthField::MarginTop, 1.5, Unit::Em);
        set(&mut css, LengthField::MarginBottom, 12.0, Unit::Points);
        set(&mut css, LengthField::MarginLeft, 5.0, Unit::Percent);
        set(&mut css, LengthField::MarginRight, 0.5, Unit::Rem);
        set(&mut css, LengthField::PaddingTop, -2.7, Unit::Pixels);
        set(&mut css, LengthField::PaddingLeft, 3.0, Unit::Em);
        set(&mut css, LengthField::PaddingRight, 50.0, Unit::Percent);
        let b = from_css(&css, 17.5, TextAlign::None as u8, 400);
        assert_eq!(
            (b.margin_top, b.margin_bottom, b.margin_left, b.margin_right),
            (26, 15, 20, 8)
        );
        assert_eq!(
            (b.padding_top, b.padding_left, b.padding_right),
            (-2, 35, 35)
        );
        assert_eq!(b.alignment, TextAlign::Justify as u8);
        assert_eq!(
            from_css(&css, 17.5, TextAlign::None as u8, 0).margin_left,
            0
        );
        // Beyond 16 bits the value wraps; beyond 32 it saturates first.
        assert_eq!(px16(40000.0), -25536);
        assert_eq!(px16(f32::NAN), 0);
        assert_eq!(px16(1e12), -1);
    }

    #[test]
    fn indent_alignment_breaks_and_direction() {
        let mut css = Style::DEFAULT;
        set(&mut css, LengthField::TextIndent, 5.0, Unit::Percent);
        css.text_align = TextAlign::Center;
        css.page_break_before = true;
        css.direction = Direction::Rtl;
        css.defined.set(
            Defined::length(LengthField::TextIndent)
                | Defined::TEXT_ALIGN
                | Defined::PAGE_BREAK_BEFORE
                | Defined::PAGE_BREAK_AFTER
                | Defined::DIRECTION,
        );
        let b = from_css(&css, 20.0, TextAlign::None as u8, 300);
        assert!(b.text_indent_defined && b.text_align_defined && b.direction_defined);
        assert_eq!((b.text_indent, b.alignment), (15, TextAlign::Center as u8));
        assert!(b.page_break_before && !b.page_break_after && b.is_rtl);
        let b = from_css(&css, 20.0, TextAlign::None as u8, 0);
        assert!(!b.text_indent_defined);
        assert_eq!(b.text_indent, 0);
        let b = from_css(&css, 20.0, TextAlign::Left as u8, 300);
        assert_eq!(b.alignment, TextAlign::Left as u8);
        assert!(b.text_align_defined);
        // Undefined properties keep the defaults whatever their values.
        css.defined = Defined(0);
        assert_eq!(
            from_css(&css, 20.0, TextAlign::None as u8, 300),
            Block::DEFAULT
        );
    }

    #[test]
    fn nested_blocks_combine_per_axis() {
        let parent = Block {
            margin_left: 10,
            margin_right: 4,
            padding_left: 2,
            padding_right: 1,
            margin_top: 12,
            margin_bottom: 3,
            padding_top: 5,
            padding_bottom: 6,
            text_indent: 30,
            text_indent_defined: true,
            alignment: TextAlign::Center as u8,
            text_align_defined: true,
            is_rtl: true,
            direction_defined: true,
            font_scale: 1.5,
            font_scale_defined: true,
            page_break_before: true,
            page_break_after: true,
            keep_with_next: true,
            font_id: 7,
            character_spacing: 2,
        };
        let child = Block {
            margin_left: 1,
            margin_right: 2,
            padding_left: 3,
            padding_right: 4,
            margin_top: 8,
            margin_bottom: 9,
            padding_top: 1,
            padding_bottom: 1,
            ..Block::DEFAULT
        };
        let combined = |parent: &Block, child: &Block, axis| {
            let mut b = *child;
            combine(parent, &mut b, axis);
            b
        };
        let across = combined(&parent, &child, Axis::Horizontal);
        assert_eq!(
            (
                across.margin_left,
                across.margin_right,
                across.padding_left,
                across.padding_right
            ),
            (11, 6, 5, 5)
        );
        assert_eq!((across.margin_top, across.text_indent), (8, 30));
        assert_eq!(across.alignment, TextAlign::Center as u8);
        assert!(across.is_rtl && !across.page_break_before && !across.keep_with_next);
        assert_eq!((across.font_scale, across.font_id), (1.5, 0));
        let down = combined(&parent, &child, Axis::Vertical);
        assert_eq!(
            (down.margin_left, down.margin_top, down.margin_bottom),
            (1, 12, 9)
        );
        assert_eq!((down.padding_top, down.padding_bottom), (6, 7));
        assert!(!down.text_indent_defined && down.page_break_before && !down.page_break_after);
        assert!(down.is_rtl);
        let own = Block {
            text_indent: -5,
            text_indent_defined: true,
            alignment: TextAlign::Right as u8,
            text_align_defined: true,
            direction_defined: true,
            font_scale: 0.8,
            font_scale_defined: true,
            ..child
        };
        let kept = combined(&parent, &own, Axis::Horizontal);
        assert_eq!(
            (kept.text_indent, kept.alignment),
            (-5, TextAlign::Right as u8)
        );
        assert!(!kept.is_rtl);
        assert_eq!(kept.font_scale, 0.8);
        let wide = Block {
            margin_left: 32000,
            ..Block::DEFAULT
        };
        let more = Block {
            margin_left: 1000,
            ..Block::DEFAULT
        };
        assert_eq!(combined(&wide, &more, Axis::Horizontal).margin_left, -32536);
        let mut bottom = child;
        add_bottom(&mut bottom, &parent);
        assert_eq!((bottom.margin_bottom, bottom.padding_bottom), (9, 7));
        assert_eq!(bottom.margin_top, 8);
    }
}
