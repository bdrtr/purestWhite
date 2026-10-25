//! `BlockStyle`'s helpers, with its 16-bit arithmetic.

use bembeyaz_css::block::{self, Axis, Block};

pub(crate) trait BlockExt {
    fn left_inset(&self) -> i16;
    fn right_inset(&self) -> i16;
    fn total_horizontal_inset(&self) -> i16;
    fn top_inset(&self) -> i16;
    fn bottom_inset(&self) -> i16;
    fn without_top(&self) -> Block;
    fn without_bottom(&self) -> Block;
    /// The child combined with this, its parent, along `axis`.
    fn combined(&self, child: &Block, axis: Axis) -> Block;
    /// This with `source`'s bottom collapsed into it.
    fn with_bottom_of(&self, source: &Block) -> Block;
}

impl BlockExt for Block {
    fn left_inset(&self) -> i16 {
        self.margin_left.wrapping_add(self.padding_left)
    }
    fn right_inset(&self) -> i16 {
        self.margin_right.wrapping_add(self.padding_right)
    }
    fn total_horizontal_inset(&self) -> i16 {
        self.left_inset().wrapping_add(self.right_inset())
    }
    fn top_inset(&self) -> i16 {
        self.margin_top.wrapping_add(self.padding_top)
    }
    fn bottom_inset(&self) -> i16 {
        self.margin_bottom.wrapping_add(self.padding_bottom)
    }
    fn without_top(&self) -> Block {
        Block {
            margin_top: 0,
            padding_top: 0,
            ..*self
        }
    }
    fn without_bottom(&self) -> Block {
        Block {
            margin_bottom: 0,
            padding_bottom: 0,
            ..*self
        }
    }
    fn combined(&self, child: &Block, axis: Axis) -> Block {
        let mut result = *child;
        block::combine(self, &mut result, axis);
        result
    }
    fn with_bottom_of(&self, source: &Block) -> Block {
        let mut result = *self;
        block::add_bottom(&mut result, source);
        result
    }
}
