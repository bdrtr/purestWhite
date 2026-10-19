//! The rule store: selectors sorted for binary search, their text in one pool in source order,
//! and deduplicated styles. Every store grows by doubling, through fallible allocation, up to a
//! fixed cap.

use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::ascii::{strip_suffix, tokens, words};
use crate::selector::{
    Ancestor, Compound, ancestors_match, context_specificity, pop_combinator, pop_compound,
    specificity,
};
use crate::style::Style;

/// Rules kept: the index is 12 KB at the cap.
pub(crate) const MAX_RULES: usize = 1500;
/// Bytes of selector text kept.
pub(crate) const SELECTOR_POOL_CAP: usize = 32 * 1024;
/// Distinct styles kept: about 32 KB at the cap.
const MAX_STYLES: usize = 256;
/// Longest selector read; longer ones are skipped.
const MAX_SELECTOR: usize = 256;
/// Longest stored key: a selector, reordered, and the context separator.
pub(crate) const MAX_STORED_KEY: usize = MAX_SELECTOR + 1;

/// Separates a key's subject from its ancestor part (`div.poem > `). It sorts below every
/// selector byte, so the contextual rules of one subject follow the subject's own rule.
const SEPARATOR: u8 = 0x1F;
const CONTEXT_SEPARATOR: &[u8] = &[SEPARATOR];
const FIRST_LETTER: &[u8] = b"::first-letter";

/// Classes of an element looked at; combinations of classes use only the first four.
const MAX_ELEMENT_CLASSES: usize = 8;
const MAX_SUBSET_CLASSES: usize = 4;
/// Matches merged per pass; more take further passes.
const MAX_MATCHES: usize = 16;

/// Why the store stopped taking new rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// A store reached its cap.
    Limit,
    /// The heap could not grow a store.
    OutOfMemory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Insert {
    Inserted,
    /// Merged into the rule with the same key that was stored last.
    Merged,
}

#[derive(Clone, Copy)]
struct Entry {
    /// Where the key starts in the pool.
    offset: u32,
    style: u16,
    len: u16,
}

/// The rules of a book's stylesheets.
#[derive(Default)]
pub struct Rules {
    /// Sorted by key; equal keys newest first.
    entries: Vec<Entry>,
    /// Keys, lowercase, in the order they were added: source order.
    pool: Vec<u8>,
    styles: Vec<Style>,
    stop: Option<Stop>,
    shapes: Shapes,
}

/// Which kinds of rule are stored, so lookups skip what none can match.
#[derive(Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools, reason = "independent flags")]
struct Shapes {
    id: bool,
    /// Two classes or more on the subject.
    compound: bool,
    contextual: bool,
    first_letter: bool,
}

/// An element to find the style of.
pub struct Query<'a> {
    pub tag: &'a [u8],
    /// The `class` attribute: space-separated classes.
    pub classes: &'a [u8],
    /// The `id` attribute; empty for none.
    pub id: &'a [u8],
    /// Open ancestors, outermost first: the last is the parent.
    pub ancestors: &'a [Ancestor],
    /// Resolve the element's `::first-letter` rules instead of its own.
    pub first_letter: bool,
}

/// A lookup key as views, so it is compared without being copied.
#[derive(Clone, Copy)]
struct Key<'a> {
    pieces: [&'a [u8]; 14],
    count: usize,
    len: usize,
}

impl<'a> Key<'a> {
    const fn new() -> Self {
        Key {
            pieces: [&[]; 14],
            count: 0,
            len: 0,
        }
    }

    /// False when the key is full; an empty piece takes no room.
    fn add(&mut self, piece: &'a [u8]) -> bool {
        if piece.is_empty() {
            return true;
        }
        let Some(slot) = self.pieces.get_mut(self.count) else {
            return false;
        };
        *slot = piece;
        self.count += 1;
        self.len += piece.len();
        true
    }

    fn pop(&mut self) {
        if let Some(count) = self.count.checked_sub(1) {
            self.len -= self.pieces.get(count).map_or(0, |p| p.len());
            self.count = count;
        }
    }

    fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.pieces
            .iter()
            .take(self.count)
            .flat_map(|p| p.iter().map(u8::to_ascii_lowercase))
    }
}

fn cmp_ignoring_case(a: &[u8], b: &[u8]) -> Ordering {
    a.iter()
        .map(u8::to_ascii_lowercase)
        .cmp(b.iter().map(u8::to_ascii_lowercase))
}

/// Insertion sort: a handful of classes.
fn sort_ignoring_case(items: &mut [&[u8]]) {
    for i in 1..items.len() {
        let mut j = i;
        while j > 0
            && items
                .get(j - 1)
                .zip(items.get(j))
                .is_some_and(|(a, b)| cmp_ignoring_case(a, b).is_gt())
        {
            items.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// Grows `v` to hold `more` items past its length, doubling from `first` up to `cap`.
fn reserve<T>(v: &mut Vec<T>, more: usize, first: usize, cap: usize) -> Result<(), Stop> {
    let needed = v.len() + more;
    if needed <= v.capacity() {
        return Ok(());
    }
    if needed > cap {
        return Err(Stop::Limit);
    }
    let mut capacity = if v.capacity() == 0 {
        first
    } else {
        v.capacity() * 2
    };
    while capacity < needed {
        capacity *= 2;
    }
    v.try_reserve_exact(capacity.min(cap) - v.len())
        .map_err(|_| Stop::OutOfMemory)
}

impl Rules {
    #[must_use]
    pub const fn new() -> Self {
        Rules {
            entries: Vec::new(),
            pool: Vec::new(),
            styles: Vec::new(),
            stop: None,
            shapes: Shapes {
                id: false,
                compound: false,
                contextual: false,
                first_letter: false,
            },
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// True when a `::first-letter` rule is stored, so callers can skip that lookup.
    #[must_use]
    pub fn has_first_letter_rules(&self) -> bool {
        self.shapes.first_letter
    }

    /// Why new rules stopped being stored, if they did.
    #[must_use]
    pub fn stopped(&self) -> Option<Stop> {
        self.stop
    }

    pub fn clear(&mut self) {
        *self = Rules::new();
    }

    fn key_at(&self, entry: Entry) -> &[u8] {
        let start = entry.offset as usize;
        self.pool
            .get(start..start + usize::from(entry.len))
            .unwrap_or_default()
    }

    /// How a stored key sorts against `key`; with `prefix`, a stored key that starts with `key`
    /// is equal.
    fn compare(&self, entry: Entry, key: &Key<'_>, prefix: bool) -> Ordering {
        let mut stored = self.key_at(entry).iter();
        for probe in key.bytes() {
            match stored.next() {
                None => return Ordering::Less,
                Some(&b) if b != probe => return b.cmp(&probe),
                Some(_) => {}
            }
        }
        if prefix || stored.len() == 0 {
            Ordering::Equal
        } else {
            Ordering::Greater
        }
    }

    /// The first entry not below `key`, and whether it equals `key`.
    fn lower_bound(&self, key: &Key<'_>) -> (usize, bool) {
        let at = self
            .entries
            .partition_point(|&e| self.compare(e, key, false).is_lt());
        let exact = self
            .entries
            .get(at)
            .is_some_and(|&e| self.compare(e, key, false).is_eq());
        (at, exact)
    }

    fn intern(&mut self, style: &Style) -> Result<u16, Stop> {
        if let Some(index) = self.styles.iter().position(|s| s == style) {
            return u16::try_from(index).map_err(|_| Stop::Limit);
        }
        reserve(&mut self.styles, 1, 16, MAX_STYLES)?;
        self.styles.push(*style);
        u16::try_from(self.styles.len() - 1).map_err(|_| Stop::Limit)
    }

    fn insert(&mut self, key: &Key<'_>, style: &Style) -> Result<Insert, Stop> {
        let (position, exact) = self.lower_bound(key);
        // Only the rule stored last can merge without moving its declarations past others.
        if exact
            && let Some(&entry) = self.entries.get(position)
            && entry.offset as usize + usize::from(entry.len) == self.pool.len()
        {
            let mut merged = self
                .styles
                .get(usize::from(entry.style))
                .copied()
                .unwrap_or_default();
            merged.apply(style);
            let shared = self
                .entries
                .iter()
                .enumerate()
                .any(|(i, e)| i != position && e.style == entry.style);
            if shared {
                let index = self.intern(&merged)?;
                if let Some(e) = self.entries.get_mut(position) {
                    e.style = index;
                }
            } else if let Some(slot) = self.styles.get_mut(usize::from(entry.style)) {
                *slot = merged;
            }
            return Ok(Insert::Merged);
        }

        reserve(&mut self.entries, 1, 32, MAX_RULES)?;
        reserve(&mut self.pool, key.len, 512, SELECTOR_POOL_CAP)?;
        let style = self.intern(style)?;
        let offset = u32::try_from(self.pool.len()).map_err(|_| Stop::Limit)?;
        let len = u16::try_from(key.len).map_err(|_| Stop::Limit)?;
        self.pool.extend(key.bytes());
        self.entries.insert(position, Entry { offset, style, len });
        self.note_shape(position);
        Ok(Insert::Inserted)
    }

    fn note_shape(&mut self, position: usize) {
        let Some(&entry) = self.entries.get(position) else {
            return;
        };
        let key = self.key_at(entry);
        let separator = key.iter().position(|&b| b == SEPARATOR);
        let subject = key
            .get(..separator.unwrap_or(key.len()))
            .unwrap_or_default();
        let contextual = separator.is_some();
        let id = subject.contains(&b'#');
        let compound = subject.iter().filter(|&&b| b == b'.').nth(1).is_some();
        let first_letter = subject.contains(&b':');
        let shapes = &mut self.shapes;
        shapes.contextual |= contextual;
        shapes.id |= id;
        shapes.compound |= compound;
        shapes.first_letter |= first_letter;
    }

    /// Stores `style` under each selector of a comma-separated group.
    pub(crate) fn add_rule(&mut self, selectors: &[u8], style: &Style) {
        // Rules without a property this reader uses take no memory.
        if style.defined.0 == 0 {
            return;
        }
        for selector in tokens(selectors, |b| b == b',') {
            self.add_selector(selector, style);
        }
    }

    fn add_selector(&mut self, mut selector: &[u8], style: &Style) {
        if selector.len() > MAX_SELECTOR {
            return;
        }
        // ::first-letter (or the older :first-letter) rules have their own key.
        let mut first_letter = false;
        for suffix in ["::first-letter", ":first-letter"] {
            if !first_letter
                && selector.len() > suffix.len()
                && let Some(rest) = strip_suffix(selector, suffix)
            {
                selector = rest;
                first_letter = true;
            }
        }
        // Sibling combinators, attributes and other pseudo-classes are not supported.
        if selector.iter().any(|b| b"+~[:".contains(b)) {
            return;
        }

        let mut context = selector;
        let Some(mut subject) =
            Compound::parse(pop_compound(&mut context)).filter(|c| !c.is_empty())
        else {
            return;
        };
        let mut has_ancestor = false;
        let mut rest = context;
        loop {
            pop_combinator(&mut rest);
            if rest.is_empty() {
                break;
            }
            if Compound::parse(pop_compound(&mut rest)).is_none() {
                return;
            }
            has_ancestor = true;
        }
        if !context.is_empty() && !has_ancestor {
            return;
        }

        // The subject in canonical form, the order lookups build keys in: element, #id, then
        // the classes sorted.
        let count = subject.class_count;
        if let Some(classes) = subject.classes.get_mut(..count) {
            sort_ignoring_case(classes);
        }
        let mut key = Key::new();
        key.add(subject.tag);
        if !subject.id.is_empty() {
            key.add(b"#");
            key.add(subject.id);
        }
        for class in subject.classes() {
            key.add(b".");
            key.add(class);
        }
        if first_letter {
            key.add(FIRST_LETTER);
        }
        if has_ancestor {
            key.add(CONTEXT_SEPARATOR);
            key.add(context);
        }

        // Past a cap, only a selector already stored is tried: it merges when it is the rule
        // stored last (see insert).
        if self.stop.is_some() && !self.lower_bound(&key).1 {
            return;
        }
        if let Err(stop) = self.insert(&key, style) {
            // The first cause is the one worth reporting.
            self.stop.get_or_insert(stop);
        }
    }

    /// The merged style of every rule that matches, in specificity order, then source order.
    #[must_use]
    pub fn resolve(&self, query: &Query<'_>) -> Style {
        let mut result = Style::DEFAULT;
        if self.entries.is_empty() || (query.first_letter && !self.shapes.first_letter) {
            return result;
        }
        let mut classes = [&[][..]; MAX_ELEMENT_CLASSES];
        let count = element_classes(query.classes, &mut classes);
        let classes = classes.get(..count).unwrap_or_default();

        let mut matcher = Matcher {
            rules: self,
            ancestors: query.ancestors,
            matches: [Match::default(); MAX_MATCHES],
            count: 0,
            last: None,
        };
        // Further passes take the matches past the first MAX_MATCHES.
        loop {
            matcher.count = 0;
            matcher.collect(query, classes);
            for m in matcher.matches.iter().take(matcher.count) {
                let style = self
                    .entries
                    .get(usize::from(m.entry))
                    .and_then(|e| self.styles.get(usize::from(e.style)));
                if let Some(style) = style {
                    result.apply(style);
                }
            }
            if matcher.count > 0 {
                matcher.last = matcher.matches.get(matcher.count - 1).copied();
            }
            if matcher.count != MAX_MATCHES {
                return result;
            }
        }
    }

    // ---- Cache ----

    /// Makes room for `count` more rules at once.
    pub(crate) fn reserve_rules(&mut self, count: usize) -> Result<(), Stop> {
        reserve(&mut self.entries, count, 32, MAX_RULES)
    }

    /// Stores a rule read back from the cache.
    pub(crate) fn insert_stored(&mut self, key: &[u8], style: &Style) -> Result<Insert, Stop> {
        let mut k = Key::new();
        k.add(key);
        self.insert(&k, style)
    }

    /// The rules in source order, as keys and styles; `None` when the order does not fit in
    /// memory.
    pub(crate) fn in_source_order(&self) -> Option<impl Iterator<Item = (&[u8], &Style)>> {
        let mut order = Vec::new();
        order.try_reserve_exact(self.entries.len()).ok()?;
        order.extend(0..u16::try_from(self.entries.len()).ok()?);
        let offset = |i: &u16| self.entries.get(usize::from(*i)).map_or(0, |e| e.offset);
        heap_sort_by_key(&mut order, offset);
        Some(order.into_iter().filter_map(move |i| {
            let entry = self.entries.get(usize::from(i))?;
            Some((
                self.key_at(*entry),
                self.styles.get(usize::from(entry.style))?,
            ))
        }))
    }
}

/// The first classes of a `class` attribute, sorted without case and without repeats; their
/// count.
fn element_classes<'q>(attr: &'q [u8], out: &mut [&'q [u8]; MAX_ELEMENT_CLASSES]) -> usize {
    let mut count = 0;
    for (slot, class) in out.iter_mut().zip(words(attr)) {
        *slot = class;
        count += 1;
    }
    let classes = out.get_mut(..count).unwrap_or_default();
    sort_ignoring_case(classes);
    let mut unique = 0;
    for i in 0..classes.len() {
        let class = classes.get(i).copied().unwrap_or_default();
        let repeat = unique > 0
            && classes
                .get(unique - 1)
                .is_some_and(|last| cmp_ignoring_case(last, class).is_eq());
        if !repeat {
            if let Some(slot) = classes.get_mut(unique) {
                *slot = class;
            }
            unique += 1;
        }
    }
    unique
}

/// Heap sort: small code, no recursion, n log n for the cap's 1500 rules.
fn heap_sort_by_key(v: &mut [u16], key: impl Fn(&u16) -> u32) {
    let k = |v: &[u16], i: usize| v.get(i).map_or(0, &key);
    let sift = |v: &mut [u16], mut root: usize, end: usize| loop {
        let mut child = 2 * root + 1;
        if child >= end {
            return;
        }
        if child + 1 < end && k(v, child) < k(v, child + 1) {
            child += 1;
        }
        if k(v, root) >= k(v, child) {
            return;
        }
        v.swap(root, child);
        root = child;
    };
    for root in (0..v.len() / 2).rev() {
        sift(v, root, v.len());
    }
    for end in (1..v.len()).rev() {
        v.swap(0, end);
        sift(v, 0, end);
    }
}

#[derive(Clone, Copy, Default)]
struct Match {
    specificity: u32,
    entry: u16,
}

/// Collects one pass's matches in the order they apply.
struct Matcher<'a> {
    rules: &'a Rules,
    ancestors: &'a [Ancestor],
    matches: [Match; MAX_MATCHES],
    count: usize,
    /// The last match an earlier pass applied.
    last: Option<Match>,
}

impl Matcher<'_> {
    /// Specificity, then source order (the pool keeps selectors as they came, across stylesheets).
    fn precedes(&self, a: Match, b: Match) -> bool {
        let offset = |m: Match| {
            self.rules
                .entries
                .get(usize::from(m.entry))
                .map_or(0, |e| e.offset)
        };
        a.specificity < b.specificity || (a.specificity == b.specificity && offset(a) < offset(b))
    }

    fn add(&mut self, specificity: u32, entry: usize) {
        let Ok(entry) = u16::try_from(entry) else {
            return;
        };
        let m = Match { specificity, entry };
        if self.last.is_some_and(|last| !self.precedes(last, m)) {
            return;
        }
        let position = self
            .matches
            .iter()
            .take(self.count)
            .take_while(|&&x| self.precedes(x, m))
            .count();
        if position == MAX_MATCHES {
            return;
        }
        self.count = (self.count + 1).min(MAX_MATCHES);
        if let Some(tail) = self.matches.get_mut(position..self.count) {
            tail.rotate_right(1);
            if let Some(first) = tail.first_mut() {
                *first = m;
            }
        }
    }

    /// One pass over the keys an element can match: element and id with each class, and with
    /// each combination of its first four classes.
    fn collect(&mut self, query: &Query<'_>, classes: &[&[u8]]) {
        let rules = self.rules;
        let subset = classes.len().min(MAX_SUBSET_CLASSES);
        let with_ids = u32::from(rules.shapes.id && !query.id.is_empty());
        let pseudo = |key: &mut Key<'_>| {
            if query.first_letter {
                key.add(FIRST_LETTER);
            }
        };
        for with_id in 0..=with_ids {
            for with_tag in 0..=1 {
                let mut base = Key::new();
                if with_tag == 1 {
                    base.add(query.tag);
                }
                if with_id == 1 {
                    base.add(b"#");
                    base.add(query.id);
                }
                if with_tag == 1 || with_id == 1 {
                    let mut key = base;
                    pseudo(&mut key);
                    self.lookup(&mut key, specificity(with_id, 0, with_tag));
                }
                for &class in classes {
                    let mut key = base;
                    key.add(b".");
                    key.add(class);
                    pseudo(&mut key);
                    self.lookup(&mut key, specificity(with_id, 1, with_tag));
                }
                if !rules.shapes.compound {
                    continue;
                }
                for mask in 1u32..1 << subset {
                    let bits = mask.count_ones();
                    if bits < 2 {
                        continue;
                    }
                    let mut key = base;
                    for (i, &class) in classes.iter().take(subset).enumerate() {
                        if mask & 1 << i != 0 {
                            key.add(b".");
                            key.add(class);
                        }
                    }
                    pseudo(&mut key);
                    self.lookup(&mut key, specificity(with_id, bits, with_tag));
                }
            }
        }
    }

    fn lookup(&mut self, key: &mut Key<'_>, specificity: u32) {
        let rules = self.rules;
        let (start, _) = rules.lower_bound(key);
        for (i, entry) in rules.entries.iter().enumerate().skip(start) {
            if !rules.compare(*entry, key, false).is_eq() {
                break;
            }
            self.add(specificity, i);
        }
        if !rules.shapes.contextual || self.ancestors.is_empty() || !key.add(CONTEXT_SEPARATOR) {
            return;
        }
        let (start, _) = rules.lower_bound(key);
        for (i, entry) in rules.entries.iter().enumerate().skip(start) {
            if !rules.compare(*entry, key, true).is_eq() {
                break;
            }
            let context = rules.key_at(*entry).get(key.len..).unwrap_or_default();
            if ancestors_match(context, self.ancestors) {
                self.add(specificity.wrapping_add(context_specificity(context)), i);
            }
        }
        key.pop();
    }
}
