//! Arabic contextual shaping on a line already in visual order, as mintty's minibidi does it,
//! with CrossPoint's changes: marks (NSM) are transparent for joining, the Alef a Lam-Alef
//! ligature absorbs becomes [`LIGATURE_PLACEHOLDER`] instead of a space, and the Perso-Arabic
//! letters outside U+0621..U+064A shape too, and ZWNJ/ZWJ in the text count as non-joining and
//! join-causing (their ArabicShaping.txt types).
//!
//! Visual order runs left to right, so a right-to-left word's next letter is on the left: the
//! visually previous character is the logically next one.

use crate::{Char, Class, Join, LIGATURE_PLACEHOLDER, ZWJ, ZWNJ, tables};
use core::cmp::Ordering;

const ISOLATED: u8 = 0;
const FINAL: u8 = 1;
const INITIAL: u8 = 2;
const MEDIAL: u8 = 3;

fn arabic(cp: u32) -> Option<(Join, u8)> {
    let i = usize::try_from(cp.checked_sub(0x0621)?).ok()?;
    tables::ARABIC.get(i).copied()
}

pub(crate) fn joining(cp: u32) -> Join {
    if let Some((join, _)) = arabic(cp) {
        return join;
    }
    let Ok(cp) = u16::try_from(cp) else {
        return Join::None;
    };
    tables::JOINING
        .binary_search_by(|&(first, last, _)| {
            if last < cp {
                Ordering::Less
            } else if first > cp {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .ok()
        .and_then(|i| tables::JOINING.get(i))
        .map_or(Join::None, |&(_, _, join)| join)
}

/// The presentation form of `cp` for `form`, or `cp` when it has none.
pub(crate) fn form(cp: u32, form: u8) -> u32 {
    if let Some((_, isolated)) = arabic(cp) {
        return if isolated == 0 {
            cp
        } else {
            0xFE00 + u32::from(isolated) + u32::from(form)
        };
    }
    let Ok(letter) = u16::try_from(cp) else {
        return cp;
    };
    tables::FORMS_A
        .binary_search_by_key(&letter, |&(letter, _, _)| letter)
        .ok()
        .and_then(|i| tables::FORMS_A.get(i))
        .filter(|&&(_, _, count)| form < count)
        .map_or(cp, |&(_, isolated, _)| {
            u32::from(isolated) + u32::from(form)
        })
}

/// Connects on its left side: as the right-hand neighbour, it joins the letter being shaped.
fn joins_left(join: Join) -> bool {
    matches!(join, Join::Dual | Join::Causing)
}

/// Connects on its right side: as the left-hand neighbour, it joins the letter being shaped.
fn joins_right(join: Join) -> bool {
    matches!(join, Join::Right | Join::Dual | Join::Causing)
}

/// Writes the shaped codepoint of each character of `line` (visual order) to `out`.
pub(crate) fn shape(line: &[Char], out: &mut [u32]) {
    let cp_at = |i: usize| line.get(i).map_or(0, |c| c.cp);
    let not_mark = |i: &usize| line.get(*i).is_some_and(|c| c.class != Class::NSM);
    for i in 0..line.len().min(out.len()) {
        let cp = cp_at(i);
        if let Some(slot) = out.get_mut(i) {
            *slot = cp;
        }
        let join = joining(cp);
        if !matches!(join, Join::Right | Join::Dual) {
            continue;
        }
        // The neighbours on screen, looking through marks: the one on the right comes before it
        // in a right-to-left word.
        let right = (i + 1..line.len())
            .find(not_mark)
            .map_or(Join::None, |j| joining(cp_at(j)));
        let left_at = (0..i).rev().find(not_mark);
        let left = left_at.map_or(Join::None, |j| joining(cp_at(j)));

        let shaped = if join == Join::Right {
            form(cp, if joins_left(right) { FINAL } else { ISOLATED })
        } else {
            // Lam with an Alef on its left is a ligature; the Alef is absorbed.
            let ligature = match left_at.map(cp_at) {
                _ if cp != 0x0644 => None,
                Some(0x0622) => Some((0xFEF6, 0xFEF5)),
                Some(0x0623) => Some((0xFEF8, 0xFEF7)),
                Some(0x0625) => Some((0xFEFA, 0xFEF9)),
                Some(0x0627) => Some((0xFEFC, 0xFEFB)),
                _ => None,
            };
            if let (Some((joined, alone)), Some(alef)) = (ligature, left_at) {
                if let Some(slot) = out.get_mut(alef) {
                    *slot = LIGATURE_PLACEHOLDER;
                }
                if joins_left(right) { joined } else { alone }
            } else {
                let after = line.get(i).map_or(0, |c| c.joiners & 0xF);
                let before = line.get(i).map_or(0, |c| c.joiners >> 4);
                let f = if before == ZWNJ {
                    if joins_right(left) { INITIAL } else { ISOLATED }
                } else if before == ZWJ | ZWNJ {
                    MEDIAL
                } else if before == ZWJ {
                    FINAL
                } else if after & ZWNJ != 0 {
                    if joins_left(right) { FINAL } else { ISOLATED }
                } else if joins_left(right) {
                    if joins_right(left) { MEDIAL } else { FINAL }
                } else if joins_right(left) {
                    INITIAL
                } else {
                    ISOLATED
                };
                form(cp, f)
            }
        };
        if let Some(slot) = out.get_mut(i) {
            *slot = shaped;
        }
    }
}
