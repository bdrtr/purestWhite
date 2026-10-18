//! UAX #9 (Unicode 17.0.0) on one line that is one paragraph: the paragraph level (P2, P3),
//! explicit levels (X1-X8), isolating run sequences (X9, X10), weak types (W1-W7), paired
//! brackets and neutrals (N0-N2), implicit levels (I1, I2) and trailing whitespace (L1); then
//! reordering (L2) and mirroring (L4). The conformance tests (`conformance.rs`) check it.
//!
//! Each step keeps its arrays in a frame of its own, so no stack frame grows past what
//! `scripts/build_rust.py` allows.

use crate::{Char, Class, MAX_LINE, bracket, mirror};
use Class::{
    AL, AN, B, BN, CS, EN, ES, ET, FSI, L, LRE, LRI, LRO, NSM, ON, PDF, PDI, R, RLE, RLI, RLO, S,
    WS,
};

/// The deepest explicit embedding level (BD2).
const MAX_DEPTH: u8 = 125;
/// BD16 tracks this many open brackets; at the next one it stops looking for pairs.
const MAX_BRACKETS: usize = 63;
/// No index.
const NONE: u8 = u8::MAX;

/// The direction of an embedding level.
fn direction(level: u8) -> Class {
    if level & 1 == 1 { R } else { L }
}

fn is_isolate_initiator(class: Class) -> bool {
    matches!(class, LRI | RLI | FSI)
}

/// Removed by X9: embedding controls and boundary neutrals.
fn removed(class: Class) -> bool {
    matches!(class, RLE | LRE | RLO | LRO | PDF | BN)
}

/// Neutral or isolate formatting character (N1, N2).
fn is_ni(class: Class) -> bool {
    matches!(class, B | S | WS | ON | LRI | RLI | FSI | PDI)
}

/// L or R, with numbers counting as R (N0, N1); `None` for anything else.
fn strong(class: Class) -> Option<Class> {
    match class {
        L => Some(L),
        R | AL | EN | AN => Some(R),
        _ => None,
    }
}

fn get<T: Copy>(slice: &[T], i: usize, default: T) -> T {
    slice.get(i).copied().unwrap_or(default)
}

fn set<T>(slice: &mut [T], i: usize, value: T) {
    if let Some(slot) = slice.get_mut(i) {
        *slot = value;
    }
}

/// BD9: for each isolate initiator the index of its matching PDI, for each matched PDI that of
/// its initiator, `NONE` otherwise.
fn match_isolates(line: &[Char], pair: &mut [u8; MAX_LINE]) {
    let mut open = [0u8; MAX_LINE];
    let mut depth = 0;
    for (i, c) in (0u8..).zip(line) {
        match c.class {
            LRI | RLI | FSI => {
                set(&mut open, depth, i);
                depth += 1;
            }
            PDI if depth > 0 => {
                depth -= 1;
                let initiator = get(&open, depth, NONE);
                set(pair, usize::from(initiator), i);
                set(pair, usize::from(i), initiator);
            }
            _ => {}
        }
    }
}

/// P2: whether the first strong character of `line[from..to]` is right-to-left, skipping
/// isolates (to their matching PDI, or to the end when there is none).
fn first_strong(line: &[Char], from: usize, to: usize, pair: &[u8; MAX_LINE]) -> Option<bool> {
    let mut i = from;
    while i < to {
        match line.get(i)?.class {
            L => return Some(false),
            R | AL => return Some(true),
            LRI | RLI | FSI => match get(pair, i, NONE) {
                NONE => return None,
                pdi => i = usize::from(pdi),
            },
            _ => {}
        }
        i += 1;
    }
    None
}

/// An entry of the directional status stack (X1).
#[derive(Clone, Copy)]
struct Status {
    level: u8,
    /// `ON` for no override, else `L` or `R`.
    over: Class,
    isolate: bool,
}

/// X1-X8: the explicit level and the type (overrides applied) of each character.
#[inline(never)]
#[allow(clippy::too_many_lines, reason = "one arm per rule, X2 to X8")]
fn explicit(
    line: &[Char],
    paragraph: u8,
    pair: &[u8; MAX_LINE],
    types: &mut [Class; MAX_LINE],
    levels: &mut [u8; MAX_LINE],
) {
    let base = Status {
        level: paragraph,
        over: ON,
        isolate: false,
    };
    let mut stack = [base; MAX_DEPTH as usize + 2];
    let mut depth = 1;
    let (mut overflow_isolates, mut overflow_embeddings, mut valid_isolates) = (0u32, 0u32, 0u32);
    for (i, c) in line.iter().enumerate() {
        let top = get(&stack, depth - 1, base);
        let overridden = |class: Class| if top.over == ON { class } else { top.over };
        match c.class {
            RLE | LRE | RLO | LRO => {
                let rtl = matches!(c.class, RLE | RLO);
                let level = if rtl {
                    (top.level + 1) | 1
                } else {
                    (top.level + 2) & !1
                };
                if level <= MAX_DEPTH && overflow_isolates == 0 && overflow_embeddings == 0 {
                    let over = match c.class {
                        RLO => R,
                        LRO => L,
                        _ => ON,
                    };
                    set(
                        &mut stack,
                        depth,
                        Status {
                            level,
                            over,
                            isolate: false,
                        },
                    );
                    depth += 1;
                } else if overflow_isolates == 0 {
                    overflow_embeddings += 1;
                }
                set(types, i, c.class);
                set(levels, i, top.level);
            }
            RLI | LRI | FSI => {
                set(types, i, overridden(c.class));
                set(levels, i, top.level);
                let end = match get(pair, i, NONE) {
                    NONE => line.len(),
                    pdi => usize::from(pdi),
                };
                let rtl = match c.class {
                    RLI => true,
                    LRI => false,
                    _ => first_strong(line, i + 1, end, pair) == Some(true),
                };
                let level = if rtl {
                    (top.level + 1) | 1
                } else {
                    (top.level + 2) & !1
                };
                if level <= MAX_DEPTH && overflow_isolates == 0 && overflow_embeddings == 0 {
                    valid_isolates += 1;
                    set(
                        &mut stack,
                        depth,
                        Status {
                            level,
                            over: ON,
                            isolate: true,
                        },
                    );
                    depth += 1;
                } else {
                    overflow_isolates += 1;
                }
            }
            PDI => {
                if overflow_isolates > 0 {
                    overflow_isolates -= 1;
                } else if valid_isolates > 0 {
                    overflow_embeddings = 0;
                    while depth > 1 && !get(&stack, depth - 1, base).isolate {
                        depth -= 1;
                    }
                    depth = depth.saturating_sub(1).max(1);
                    valid_isolates -= 1;
                }
                let top = get(&stack, depth - 1, base);
                set(types, i, if top.over == ON { PDI } else { top.over });
                set(levels, i, top.level);
            }
            PDF => {
                if overflow_isolates > 0 {
                } else if overflow_embeddings > 0 {
                    overflow_embeddings -= 1;
                } else if !top.isolate && depth >= 2 {
                    depth -= 1;
                }
                set(types, i, PDF);
                set(levels, i, top.level);
            }
            B => {
                // X8: a paragraph separator ends every embedding and isolate.
                set(types, i, B);
                set(levels, i, paragraph);
                depth = 1;
                (overflow_isolates, overflow_embeddings, valid_isolates) = (0, 0, 0);
            }
            BN => {
                set(types, i, BN);
                set(levels, i, top.level);
            }
            class => {
                set(types, i, overridden(class));
                set(levels, i, top.level);
            }
        }
    }
}

/// X10, W1-W7, N0-N2 and I1-I2 on each isolating run sequence (BD13).
#[inline(never)]
fn sequences(
    line: &[Char],
    paragraph: u8,
    pair: &[u8; MAX_LINE],
    types: &mut [Class; MAX_LINE],
    levels: &mut [u8; MAX_LINE],
) {
    // The characters X9 keeps, and their level runs (BD7) as ranges of `kept`.
    let mut kept = [0u8; MAX_LINE];
    let mut count = 0;
    for (i, c) in (0u8..).zip(line) {
        if !removed(c.class) {
            set(&mut kept, count, i);
            count += 1;
        }
    }
    let kept = kept.get(..count).unwrap_or_default();
    // The explicit levels: runs, sos and eos come from them, while I1/I2 raise `levels`.
    let explicit = *levels;
    let level_at = |k: usize| get(&explicit, usize::from(get(kept, k, 0)), paragraph);
    // The level run each kept character starts, by character index.
    let mut run_end = [NONE; MAX_LINE];
    let mut k = 0;
    while k < kept.len() {
        let mut end = k + 1;
        while end < kept.len() && level_at(end) == level_at(k) {
            end += 1;
        }
        #[allow(clippy::cast_possible_truncation, reason = "at most 128")]
        set(&mut run_end, usize::from(get(kept, k, 0)), end as u8);
        k = end;
    }

    let mut seq = [0u8; MAX_LINE];
    let mut k = 0;
    while k < kept.len() {
        let first = usize::from(get(kept, k, 0));
        let end = usize::from(get(&run_end, first, NONE));
        let start = k;
        k = end;
        // A run starting with a PDI whose initiator ends another run continues that sequence.
        if line.get(first).is_some_and(|c| c.class == PDI) {
            let initiator = get(pair, first, NONE);
            if initiator != NONE && ends_a_run(kept, &run_end, usize::from(initiator)) {
                continue;
            }
        }
        // Gather the sequence: this run, then the run at each matching PDI.
        let mut len = 0;
        let (mut from, mut to) = (start, end);
        loop {
            for kk in from..to {
                set(&mut seq, len, get(kept, kk, 0));
                len += 1;
            }
            let last = usize::from(get(kept, to - 1, 0));
            let pdi = get(pair, last, NONE);
            if !line
                .get(last)
                .is_some_and(|c| is_isolate_initiator(c.class))
                || pdi == NONE
            {
                break;
            }
            let Some(pos) = kept.iter().position(|&i| i == pdi) else {
                break;
            };
            let next_end = usize::from(get(&run_end, usize::from(pdi), NONE));
            if next_end == usize::from(NONE) {
                break;
            }
            (from, to) = (pos, next_end);
        }
        let Some(seq) = seq.get(..len) else {
            continue;
        };
        // sos and eos: the higher of the sequence's level and its neighbour's (or the paragraph's).
        let level = get(&explicit, usize::from(get(seq, 0, 0)), paragraph);
        let before = start.checked_sub(1).map_or(paragraph, &level_at);
        let last = usize::from(get(seq, len - 1, 0));
        let last_kept = kept
            .iter()
            .position(|&i| usize::from(i) == last)
            .unwrap_or(0);
        let after = if line
            .get(last)
            .is_some_and(|c| is_isolate_initiator(c.class))
            || last_kept + 1 >= kept.len()
        {
            paragraph
        } else {
            level_at(last_kept + 1)
        };
        let sos = direction(level.max(before));
        let eos = direction(level.max(after));
        resolve_sequence(line, seq, level, sos, eos, types);
        for &i in seq {
            let i = usize::from(i);
            let t = get(types, i, ON);
            let l = get(levels, i, level);
            let raised = match (l & 1 == 1, t) {
                (false, R) | (true, L | EN | AN) => l + 1,
                (false, AN | EN) => l + 2,
                _ => l,
            };
            set(levels, i, raised);
        }
    }
}

/// Whether the character at index `i` is the last of its level run.
fn ends_a_run(kept: &[u8], run_end: &[u8; MAX_LINE], i: usize) -> bool {
    let Some(k) = kept.iter().position(|&c| usize::from(c) == i) else {
        return false;
    };
    // Its run starts at the latest run start at or before it.
    (0..=k)
        .rev()
        .find_map(|start| {
            let end = get(run_end, usize::from(get(kept, start, 0)), NONE);
            (end != NONE).then_some(usize::from(end))
        })
        .is_some_and(|end| end == k + 1)
}

/// W1-W7 and N0-N2 on one isolating run sequence (character indices in `seq`).
fn resolve_sequence(
    line: &[Char],
    seq: &[u8],
    level: u8,
    sos: Class,
    eos: Class,
    types: &mut [Class; MAX_LINE],
) {
    let at = |types: &[Class; MAX_LINE], k: usize| get(types, usize::from(get(seq, k, 0)), ON);
    let put = |types: &mut [Class; MAX_LINE], k: usize, class: Class| {
        set(types, usize::from(get(seq, k, 0)), class);
    };
    let n = seq.len();

    // W1: NSM takes the type before it (sos at the start; ON after an isolate control).
    for k in 0..n {
        if at(types, k) == NSM {
            let before = match k.checked_sub(1).map(|j| at(types, j)) {
                None => sos,
                Some(LRI | RLI | FSI | PDI) => ON,
                Some(t) => t,
            };
            put(types, k, before);
        }
    }
    // W2: EN after AL (back to the last strong type) is AN.
    let mut last = sos;
    for k in 0..n {
        match at(types, k) {
            t @ (L | R | AL) => last = t,
            EN if last == AL => put(types, k, AN),
            _ => {}
        }
    }
    // W3: AL is R.
    for k in 0..n {
        if at(types, k) == AL {
            put(types, k, R);
        }
    }
    // W4: one separator between two numbers of a type takes it.
    for k in 1..n.saturating_sub(1) {
        let (before, t, after) = (at(types, k - 1), at(types, k), at(types, k + 1));
        if (t == ES && before == EN && after == EN)
            || (t == CS && before == after && matches!(before, EN | AN))
        {
            put(types, k, before);
        }
    }
    // W5: a sequence of ET next to EN is EN.
    let mut k = 0;
    while k < n {
        if at(types, k) != ET {
            k += 1;
            continue;
        }
        let mut end = k;
        while end < n && at(types, end) == ET {
            end += 1;
        }
        let touches_en = (k > 0 && at(types, k - 1) == EN) || (end < n && at(types, end) == EN);
        if touches_en {
            for j in k..end {
                put(types, j, EN);
            }
        }
        k = end;
    }
    // W6: the separators and terminators left are ON.
    for k in 0..n {
        if matches!(at(types, k), ES | ET | CS) {
            put(types, k, ON);
        }
    }
    // W7: EN after L (back to sos) is L.
    let mut last = sos;
    for k in 0..n {
        match at(types, k) {
            t @ (L | R) => last = t,
            EN if last == L => put(types, k, L),
            _ => {}
        }
    }

    brackets(line, seq, level, sos, types);

    // N1, N2: a run of neutrals takes the direction around it when both sides agree, else the
    // embedding direction.
    let embedding = direction(level);
    let mut k = 0;
    while k < n {
        if !is_ni(at(types, k)) {
            k += 1;
            continue;
        }
        let mut end = k;
        while end < n && is_ni(at(types, end)) {
            end += 1;
        }
        let before = k.checked_sub(1).map_or(Some(sos), |j| strong(at(types, j)));
        let after = if end < n {
            strong(at(types, end))
        } else {
            Some(eos)
        };
        let resolved = match (before, after) {
            (Some(b), Some(a)) if a == b => b,
            _ => embedding,
        };
        for j in k..end {
            put(types, j, resolved);
        }
        k = end;
    }
}

/// N0: paired brackets (BD16) take the embedding direction when a strong type of it is inside,
/// else the other direction when it is inside and also before them.
#[inline(never)]
fn brackets(line: &[Char], seq: &[u8], level: u8, sos: Class, types: &mut [Class; MAX_LINE]) {
    let at = |types: &[Class; MAX_LINE], k: usize| get(types, usize::from(get(seq, k, 0)), ON);
    let cp_at = |k: usize| line.get(usize::from(get(seq, k, 0))).map_or(0, |c| c.cp);
    let class_at = |k: usize| {
        line.get(usize::from(get(seq, k, 0)))
            .map_or(ON, |c| c.class)
    };

    // BD16: pairs of sequence positions, found in closing order.
    let mut open = [(0u32, 0u8); MAX_BRACKETS];
    let mut depth = 0;
    let mut pairs = [(0u8, 0u8); MAX_LINE / 2];
    let mut count = 0;
    for (k, pos) in (0..seq.len()).zip(0u8..) {
        if at(types, k) != ON {
            continue;
        }
        let cp = cp_at(k);
        match bracket(cp) {
            Some(opener) if opener == cp => {
                if depth == MAX_BRACKETS {
                    break;
                }
                set(&mut open, depth, (cp, pos));
                depth += 1;
            }
            Some(opener) => {
                if let Some(d) = (0..depth)
                    .rev()
                    .find(|&d| get(&open, d, (0, 0)).0 == opener)
                {
                    set(&mut pairs, count, (get(&open, d, (0, 0)).1, pos));
                    count += 1;
                    depth = d;
                }
            }
            None => {}
        }
    }
    let Some(pairs) = pairs.get_mut(..count) else {
        return;
    };
    // By opening position; found in closing order, so nearly sorted already (insertion sort keeps
    // the general-purpose sort out of the firmware).
    for i in 1..pairs.len() {
        let mut j = i;
        while j > 0 && get(pairs, j - 1, (0, 0)).0 > get(pairs, j, (0, 0)).0 {
            pairs.swap(j - 1, j);
            j -= 1;
        }
    }

    let embedding = direction(level);
    let opposite = if embedding == L { R } else { L };
    for &(opening, closing) in pairs.iter() {
        let (opening, closing) = (usize::from(opening), usize::from(closing));
        let inside = (opening + 1..closing).filter_map(|k| strong(at(types, k)));
        let (mut same, mut other) = (false, false);
        for t in inside {
            if t == embedding {
                same = true;
            } else {
                other = true;
            }
        }
        let resolved = if same {
            embedding
        } else if other {
            let context = (0..opening)
                .rev()
                .find_map(|k| strong(at(types, k)))
                .unwrap_or(sos);
            if context == opposite {
                opposite
            } else {
                embedding
            }
        } else {
            continue;
        };
        for bracket in [opening, closing] {
            set(types, usize::from(get(seq, bracket, 0)), resolved);
            // Marks right after a bracket follow it.
            let mut k = bracket + 1;
            while k < seq.len() && class_at(k) == NSM {
                set(types, usize::from(get(seq, k, 0)), resolved);
                k += 1;
            }
        }
    }
}

/// L1, and a level for the characters X9 removed: that of the character before (the paragraph's
/// at the start), unless L1 resets it.
fn finish(line: &[Char], paragraph: u8, levels: &mut [u8; MAX_LINE]) {
    let mut previous = paragraph;
    for (i, c) in line.iter().enumerate() {
        if removed(c.class) {
            set(levels, i, previous);
        } else {
            previous = get(levels, i, paragraph);
        }
    }
    // Separators, and whitespace (with isolate controls and removed characters) before one or at
    // the end of the line, go back to the paragraph level.
    let mut trailing = true;
    for (i, c) in line.iter().enumerate().rev() {
        match c.class {
            S | B => {
                set(levels, i, paragraph);
                trailing = true;
            }
            WS | FSI | LRI | RLI | PDI => {
                if trailing {
                    set(levels, i, paragraph);
                }
            }
            class if removed(class) => {
                if trailing {
                    set(levels, i, paragraph);
                }
            }
            _ => trailing = false,
        }
    }
}

/// Resolves the embedding level of each character of `line` into `levels` (rules P2-L1).
/// `auto` takes the paragraph level from the first strong character, falling back to `level`.
/// Returns the paragraph level, or `None` when the whole line stays at level 0 untouched.
pub(crate) fn resolve(
    line: &[Char],
    auto: bool,
    level: u8,
    levels: &mut [u8; MAX_LINE],
) -> Option<u8> {
    let n = line.len().min(MAX_LINE);
    let line = line.get(..n)?;
    // Nothing right-to-left and nothing explicit in a left-to-right paragraph: all level 0.
    let simple = line.iter().all(|c| {
        !matches!(
            c.class,
            R | AL | AN | RLE | LRE | RLO | LRO | PDF | RLI | LRI | FSI | PDI
        )
    });
    let mut pair = [NONE; MAX_LINE];
    match_isolates(line, &mut pair);
    let paragraph = if auto {
        first_strong(line, 0, n, &pair).map_or(level, u8::from)
    } else {
        level
    };
    if simple && paragraph == 0 {
        return None;
    }
    let mut types = [ON; MAX_LINE];
    explicit(line, paragraph, &pair, &mut types, levels);
    sequences(line, paragraph, &pair, &mut types, levels);
    finish(line, paragraph, levels);
    Some(paragraph)
}

/// Mirrors the right-to-left glyphs of `line` and reorders it into visual order (rules L4, L2),
/// with the levels [`resolve`] gives. Returns the paragraph level.
pub(crate) fn reorder(line: &mut [Char], auto: bool, level: u8) -> u8 {
    let mut levels = [0u8; MAX_LINE];
    let n = line.len().min(MAX_LINE);
    let Some(line) = line.get_mut(..n) else {
        return level;
    };
    let Some(level) = resolve(line, auto, level, &mut levels) else {
        return 0;
    };
    let Some(levels) = levels.get(..n) else {
        return level;
    };

    // L4: a character at an odd level shows its mirrored glyph; done before L2 moves it.
    for (c, &l) in line.iter_mut().zip(levels.iter()) {
        if l & 1 == 1 {
            c.cp = mirror(c.cp);
        }
    }

    // L2: reverse every run at or above each level, from the highest down to the lowest odd one.
    // The levels stay in logical order: the runs at a level are the same either way.
    let highest = levels.iter().copied().fold(level, u8::max);
    let lowest_odd = levels
        .iter()
        .copied()
        .filter(|l| l & 1 == 1)
        .fold(u8::MAX, u8::min);
    for at in (lowest_odd..=highest).rev() {
        let mut i = 0;
        while i < n {
            let Some(start) = (i..n).find(|&j| levels.get(j).is_some_and(|&l| l >= at)) else {
                break;
            };
            let end = (start..n)
                .find(|&j| levels.get(j).is_some_and(|&l| l < at))
                .unwrap_or(n);
            if let Some(run) = line.get_mut(start..end) {
                run.reverse();
            }
            i = end;
        }
    }

    level
}
