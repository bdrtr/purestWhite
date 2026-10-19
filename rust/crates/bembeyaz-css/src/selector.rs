//! Simple selectors: compounds of an element, an `#id` and classes, and the descendant and
//! child combinators between them, matched right to left against an element's ancestors.

use crate::ascii::{hash, is_space, words};

/// Classes of one compound or element that can match; later ones are ignored.
pub(crate) const MAX_CLASSES: usize = 4;

/// An open element, hashed for matching, as the C++ `CssAncestor` lays it out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Ancestor {
    pub tag: u32,
    /// 0 when the element has no id.
    pub id: u32,
    pub classes: [u32; MAX_CLASSES],
    pub class_count: u8,
}

const _: () = assert!(core::mem::size_of::<Ancestor>() == 28);

impl Ancestor {
    #[must_use]
    pub fn new(tag: &[u8], classes: &[u8], id: &[u8]) -> Self {
        let mut ancestor = Ancestor {
            tag: hash(tag),
            id: if id.is_empty() { 0 } else { hash(id) },
            ..Ancestor::default()
        };
        for (slot, class) in ancestor.classes.iter_mut().zip(words(classes)) {
            *slot = hash(class);
            ancestor.class_count += 1;
        }
        ancestor
    }

    fn has_class(&self, class: u32) -> bool {
        self.classes
            .iter()
            .take(usize::from(self.class_count))
            .any(|&c| c == class)
    }
}

/// One compound selector: an element (empty for any), an id and classes, as written.
#[derive(Default)]
pub(crate) struct Compound<'a> {
    pub tag: &'a [u8],
    pub id: &'a [u8],
    pub classes: [&'a [u8]; MAX_CLASSES],
    pub class_count: usize,
}

impl<'a> Compound<'a> {
    /// Reads `p`, `*`, `.a.b`, `p#id.a`; `None` for anything else.
    pub fn parse(text: &'a [u8]) -> Option<Self> {
        let mut compound = Compound::default();
        let mut rest = match text {
            [] => return None,
            [b'*', rest @ ..] => rest,
            _ => {
                let (tag, rest) = split_ident(text);
                compound.tag = tag;
                rest
            }
        };
        while let [marker, after @ ..] = rest {
            let (name, tail) = split_ident(after);
            if name.is_empty() {
                return None;
            }
            if *marker == b'#' && compound.id.is_empty() {
                compound.id = name;
            } else if *marker == b'.' && compound.class_count < MAX_CLASSES {
                *compound.classes.get_mut(compound.class_count)? = name;
                compound.class_count += 1;
            } else {
                return None;
            }
            rest = tail;
        }
        Some(compound)
    }

    pub fn is_empty(&self) -> bool {
        self.tag.is_empty() && self.id.is_empty() && self.class_count == 0
    }

    pub fn classes(&self) -> &[&'a [u8]] {
        self.classes.get(..self.class_count).unwrap_or_default()
    }

    fn matches(&self, ancestor: &Ancestor) -> bool {
        (self.tag.is_empty() || hash(self.tag) == ancestor.tag)
            && (self.id.is_empty() || hash(self.id) == ancestor.id)
            && self
                .classes()
                .iter()
                .all(|&class| ancestor.has_class(hash(class)))
    }

    /// Ids, classes and elements, packed as [`specificity`] does.
    pub fn specificity(&self) -> u32 {
        #[allow(clippy::cast_possible_truncation, reason = "at most MAX_CLASSES")]
        let classes = self.class_count as u32;
        specificity(
            u32::from(!self.id.is_empty()),
            classes,
            u32::from(!self.tag.is_empty()),
        )
    }
}

/// Bytes of a CSS identifier; anything past ASCII counts.
fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b >= 0x80
}

fn split_ident(s: &[u8]) -> (&[u8], &[u8]) {
    s.split_at(s.iter().position(|&b| !is_ident(b)).unwrap_or(s.len()))
}

/// Specificity packed as `ids << 16 | classes << 8 | elements`.
pub(crate) const fn specificity(ids: u32, classes: u32, elements: u32) -> u32 {
    ids << 16 | classes << 8 | elements
}

/// Splits off the last compound, leaving what precedes it (with the combinator).
pub(crate) fn pop_compound<'a>(text: &mut &'a [u8]) -> &'a [u8] {
    let start = text
        .iter()
        .rposition(|&b| is_space(b) || b == b'>')
        .map_or(0, |p| p + 1);
    let (rest, compound) = text.split_at(start);
    *text = rest;
    compound
}

/// Strips the trailing combinator; true when it was a child combinator (`>`).
pub(crate) fn pop_combinator(text: &mut &[u8]) -> bool {
    let mut child = false;
    while let [rest @ .., last] = *text
        && (is_space(*last) || *last == b'>')
    {
        child |= *last == b'>';
        *text = rest;
    }
    child
}

/// Matches a chain of child combinators at one ancestor position; on success, leaves `context`
/// and `candidates` at the next descendant step.
fn match_child_chain(context: &mut &[u8], ancestors: &[Ancestor], candidates: &mut usize) -> bool {
    loop {
        let Some(compound) = Compound::parse(pop_compound(context)) else {
            return false;
        };
        let Some(parent) = candidates.checked_sub(1).and_then(|i| ancestors.get(i)) else {
            return false;
        };
        if !compound.matches(parent) {
            return false;
        }
        *candidates -= 1;
        if !pop_combinator(context) || context.is_empty() {
            return true;
        }
    }
}

/// Matches the ancestor part of a selector (`div.poem > `) right to left, retrying a whole child
/// chain at each descendant candidate. `ancestors` run from the outermost to the parent.
pub(crate) fn ancestors_match(mut context: &[u8], ancestors: &[Ancestor]) -> bool {
    let mut candidates = ancestors.len();
    let mut child = pop_combinator(&mut context);
    while !context.is_empty() {
        let mut remaining = context;
        let mut remaining_candidates = candidates;
        if match_child_chain(&mut remaining, ancestors, &mut remaining_candidates) {
            context = remaining;
            candidates = remaining_candidates;
            child = false;
        } else {
            if child || candidates == 0 {
                return false;
            }
            candidates -= 1;
        }
    }
    true
}

/// The summed specificity of the ancestor part; parsing stops at the first compound that fails.
pub(crate) fn context_specificity(mut context: &[u8]) -> u32 {
    let mut total = 0u32;
    loop {
        pop_combinator(&mut context);
        if context.is_empty() {
            return total;
        }
        let Some(compound) = Compound::parse(pop_compound(&mut context)) else {
            return total;
        };
        total = total.wrapping_add(compound.specificity());
    }
}
