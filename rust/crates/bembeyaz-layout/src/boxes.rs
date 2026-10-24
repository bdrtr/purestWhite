//! The chapter parser's open boxes: elements with a CSS border or a background shade, framed on
//! every page their content spans. A page break slices a box, leaving the edges where it was cut
//! open; a box with no content at all gets the height of its padding, like a rule.

/// The most boxes open at once; more nested boxes are not framed.
pub const MAX_BOXES: usize = 8;

/// Text keeps this gap from a frame even when the book sets no padding.
const MIN_FRAME_GAP: i16 = 4;
/// Narrower boxes are not framed.
const MIN_WIDTH: i32 = 8;

/// One edge: a CSS width and style (`CssBorderStyle`, 0 for none). `CssBorderSide` in C++.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Side {
    pub width: u8,
    pub style: u8,
}

impl Side {
    fn visible(self) -> bool {
        self.width > 0 && self.style != 0
    }
}

/// The part of a box on one page, to draw as a `PageBorderBox`. Sides are top, right, bottom,
/// left.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Segment {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub sides: [Side; 4],
    pub shaded: bool,
}

#[derive(Clone, Copy, Default)]
struct Scope {
    /// The element's depth, to close it at its end tag.
    depth: i32,
    left: i16,
    right: i16,
    /// Outer edge to content, border included.
    pad_top: i16,
    pad_bottom: i16,
    sides: [Side; 4],
    shaded: bool,
    /// The content's extent on the current page, -1 for none yet.
    top: i16,
    bottom: i16,
    /// Sliced onto an earlier page.
    continued: bool,
}

impl Scope {
    fn segment(&self, closing: bool, page_height: u16) -> Option<Segment> {
        if self.top < 0 {
            return None;
        }
        let top = (i32::from(self.top) - i32::from(self.pad_top)).max(0);
        let below = if closing { self.pad_bottom } else { 0 };
        let bottom = (i32::from(self.bottom) + i32::from(below)).min(i32::from(page_height));
        if bottom <= top {
            return None;
        }
        let [top_side, right, bottom_side, left] = self.sides;
        let open = Side::default();
        // Both fit: right - left is at least MIN_WIDTH, and the page is under 32768 rows.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(Segment {
            x: self.left,
            y: top as i16,
            width: (i32::from(self.right) - i32::from(self.left)) as u16,
            height: (bottom - top) as u16,
            sides: [
                if self.continued { open } else { top_side },
                right,
                if closing { bottom_side } else { open },
                left,
            ],
            shaded: self.shaded,
        })
    }
}

/// The open boxes, outermost first.
#[derive(Default)]
pub struct Boxes {
    scopes: [Scope; MAX_BOXES],
    len: u8,
}

impl Boxes {
    fn open_scopes(&mut self) -> &mut [Scope] {
        self.scopes
            .get_mut(..usize::from(self.len))
            .unwrap_or_default()
    }

    fn innermost(&self) -> Option<&Scope> {
        self.scopes.get(usize::from(self.len).checked_sub(1)?)
    }

    /// Whether another box can open.
    #[must_use]
    pub fn is_full(&self) -> bool {
        usize::from(self.len) >= MAX_BOXES
    }

    /// Whether the innermost box belongs to the element at `depth`.
    #[must_use]
    pub fn innermost_is(&self, depth: i32) -> bool {
        self.innermost().is_some_and(|s| s.depth == depth)
    }

    /// Opens a box for the element at `depth`, from its margin edges `left` and `right`, unless no
    /// room is left or it is too narrow. Only `sides` with a width and a style are drawn. The
    /// border joins the element's `padding` (top, right, bottom, left), which grows to keep the
    /// content clear of a drawn edge or shade.
    pub fn open(
        &mut self,
        depth: i32,
        sides: [Side; 4],
        shaded: bool,
        left: i16,
        right: i16,
        padding: &mut [i16; 4],
    ) -> bool {
        if self.is_full() || i32::from(right) - i32::from(left) < MIN_WIDTH {
            return false;
        }
        let sides = sides.map(|s| if s.visible() { s } else { Side::default() });
        for (pad, side) in padding.iter_mut().zip(sides) {
            if side.width > 0 || shaded {
                *pad = (*pad)
                    .max(MIN_FRAME_GAP)
                    .wrapping_add(i16::from(side.width));
            }
        }
        let [pad_top, _, pad_bottom, _] = *padding;
        let Some(slot) = self.scopes.get_mut(usize::from(self.len)) else {
            return false;
        };
        *slot = Scope {
            depth,
            left,
            right,
            pad_top,
            pad_bottom,
            sides,
            shaded,
            top: -1,
            bottom: -1,
            continued: false,
        };
        self.len += 1;
        true
    }

    /// Content from `top` to `bottom` was placed on the page, inside every open box.
    // The extents wrap at 16 bits, as the C++ did.
    #[allow(clippy::cast_possible_truncation)]
    pub fn note(&mut self, top: i32, bottom: i32) {
        for scope in self.open_scopes() {
            if scope.top < 0 {
                scope.top = top as i16;
            }
            scope.bottom = i32::from(scope.bottom).max(bottom) as i16;
        }
    }

    /// The page, `page_height` rows tall, is complete: hands `segment` each open box's part on
    /// it, outermost first, and starts the boxes afresh on the next page.
    pub fn break_page(&mut self, page_height: u16, mut segment: impl FnMut(Segment)) {
        for scope in self.open_scopes() {
            if let Some(s) = scope.segment(false, page_height) {
                segment(s);
            }
            if scope.top >= 0 {
                scope.continued = true;
            }
            scope.top = -1;
            scope.bottom = -1;
        }
    }

    /// The height the innermost box needs of its own: its padding when it has no content and
    /// none on earlier pages, else 0.
    #[must_use]
    pub fn empty_height(&self) -> i32 {
        match self.innermost() {
            Some(s) if s.top < 0 && !s.continued => {
                (i32::from(s.pad_top) + i32::from(s.pad_bottom)).max(0)
            }
            _ => 0,
        }
    }

    /// Closes the innermost box and returns its last part, on a page `page_height` rows tall.
    /// `placed` is where an empty box was given its [`empty_height`](Self::empty_height), if it
    /// was.
    // The extent wraps at 16 bits, as the C++ did.
    #[allow(clippy::cast_possible_truncation)]
    pub fn close(&mut self, placed: Option<i32>, page_height: u16) -> Option<Segment> {
        self.len = self.len.checked_sub(1)?;
        let mut scope = *self.scopes.get(usize::from(self.len))?;
        if let Some(top) = placed {
            scope.top = top.wrapping_add(i32::from(scope.pad_top)) as i16;
            scope.bottom = scope.top;
        }
        scope.segment(true, page_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    const SOLID: Side = Side { width: 2, style: 1 };

    fn open(boxes: &mut Boxes, depth: i32, sides: [Side; 4], shaded: bool) -> [i16; 4] {
        let mut padding = [0, 1, 6, 0];
        assert!(boxes.open(depth, sides, shaded, 10, 200, &mut padding));
        padding
    }

    #[test]
    fn padding_takes_the_border_and_a_gap() {
        let mut boxes = Boxes::default();
        let none = Side { width: 3, style: 0 };
        assert_eq!(
            open(&mut boxes, 1, [SOLID, none, SOLID, Side::default()], false),
            [6, 1, 8, 0]
        );
        assert_eq!(
            open(&mut boxes, 2, [Side::default(); 4], true),
            [4, 4, 6, 4]
        );
    }

    #[test]
    fn refuses_narrow_boxes_and_a_ninth() {
        let mut boxes = Boxes::default();
        let mut padding = [0; 4];
        assert!(!boxes.open(1, [SOLID; 4], false, 10, 17, &mut padding));
        assert_eq!(padding, [0; 4]);
        assert!(boxes.open(1, [SOLID; 4], false, 10, 18, &mut padding));
        boxes.close(None, 800);
        padding = [0; 4];
        for depth in 0..8 {
            open(&mut boxes, depth, [SOLID; 4], false);
        }
        assert!(boxes.is_full());
        assert!(!boxes.open(9, [SOLID; 4], false, 0, 100, &mut padding));
        assert!(boxes.innermost_is(7));
        assert!(!boxes.innermost_is(6) && !boxes.innermost_is(8));
        assert!(!Boxes::default().innermost_is(0));
    }

    #[test]
    fn a_break_slices_the_box() {
        let mut boxes = Boxes::default();
        open(&mut boxes, 1, [SOLID; 4], false);
        boxes.note(20, 40);
        boxes.note(40, 900);
        let mut parts = Vec::new();
        boxes.break_page(800, |s| parts.push(s));
        let first = Segment {
            x: 10,
            y: 14,
            width: 190,
            height: 786,
            sides: [SOLID, SOLID, Side::default(), SOLID],
            shaded: false,
        };
        assert_eq!(parts, [first]);
        boxes.note(0, 30);
        let last = boxes.close(None, 800);
        let sides = [Side::default(), SOLID, SOLID, SOLID];
        assert_eq!(
            last,
            Some(Segment {
                y: 0,
                height: 38,
                sides,
                ..first
            })
        );
        assert!(boxes.close(None, 800).is_none());
    }

    #[test]
    fn an_empty_box_is_its_padding() {
        let mut boxes = Boxes::default();
        open(
            &mut boxes,
            1,
            [SOLID, Side::default(), Side::default(), Side::default()],
            false,
        );
        assert_eq!(boxes.empty_height(), 12);
        let segment = boxes.close(Some(50), 800).unwrap_or_default();
        assert_eq!((segment.y, segment.height), (50, 12));
        // A box continued from an earlier page has no height of its own.
        open(&mut boxes, 1, [SOLID; 4], false);
        boxes.note(5, 10);
        boxes.break_page(800, |_| {});
        assert_eq!(boxes.empty_height(), 0);
        assert!(boxes.close(None, 800).is_none());
    }

    #[test]
    fn an_empty_box_waits_out_a_break() {
        // The parser may complete the page before it places an empty box.
        let mut boxes = Boxes::default();
        open(&mut boxes, 1, [SOLID; 4], false);
        let mut parts = 0;
        boxes.break_page(800, |_| parts += 1);
        assert_eq!((parts, boxes.empty_height()), (0, 14));
        let segment = boxes.close(Some(0), 800).unwrap_or_default();
        assert_eq!(
            (segment.y, segment.height, segment.sides),
            (0, 14, [SOLID; 4])
        );
    }
}
