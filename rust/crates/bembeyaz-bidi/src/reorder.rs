//! UAX #9 on one line, as the C minibidi (mintty's, up to fdd0780) runs it. Its departures from
//! the standard are kept here on purpose and named where they happen; each fix belongs in its own
//! change, with a test.

use crate::{Char, Class, MAX_LINE, bracket, mirror};
use Class::{
    AL, AN, B, BN, CS, EN, ES, ET, FSI, L, LRE, LRI, LRO, NSM, ON, PDF, PDI, R, RLE, RLI, RLO, S,
    WS,
};

/// Rule N0 tracks this many open brackets; more are ignored.
const MAX_BRACKETS: usize = 63;

fn is_neutral_or_isolate(t: Class) -> bool {
    matches!(t, B | S | WS | ON | FSI | LRI | RLI | PDI)
}

/// The direction of a level, or of the paragraph at its start (sos).
fn direction(level: u8) -> Class {
    if level & 1 == 1 { R } else { L }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "levels wrap like the C uchar"
)]
fn least_greater_odd(level: u8) -> u8 {
    ((u32::from(level) + 1) | 1) as u8
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "levels wrap like the C uchar"
)]
fn least_greater_even(level: u8) -> u8 {
    ((u32::from(level) + 2) & !1) as u8
}

/// The directional status stack of rules X1-X8. There is no depth limit (levels can pass 125
/// and wrap at 256); pushes past `MAX_LINE + 1` entries are dropped.
struct Stack {
    entries: [(u8, Class, bool); MAX_LINE + 1],
    len: usize,
}

impl Stack {
    fn push(&mut self, entry: (u8, Class, bool)) {
        if let Some(slot) = self.entries.get_mut(self.len) {
            *slot = entry;
            self.len += 1;
        }
    }

    /// Drops the top entry and returns the new top, or the paragraph-level defaults when empty.
    fn pop(&mut self) -> (u8, Class, bool) {
        self.len = self.len.saturating_sub(1);
        self.len
            .checked_sub(1)
            .and_then(|top| self.entries.get(top))
            .copied()
            .unwrap_or((0, ON, false))
    }
}

/// The first strong direction found scanning `indices` (EN and AN count as R), skipping the
/// characters rule X9 removed.
fn strong_from(
    types: &[Class],
    skip: &[bool],
    indices: impl Iterator<Item = usize>,
) -> Option<Class> {
    for j in indices {
        if skip.get(j).copied().unwrap_or(true) {
            continue;
        }
        match types.get(j).copied() {
            Some(EN | AN | R | AL) => return Some(R),
            Some(L) => return Some(L),
            _ => {}
        }
    }
    None
}

/// Resolves the embedding level of each character of `line` into `levels` (rules P2-I2).
/// `auto` takes the paragraph level from the first strong character, falling back to `level`.
/// Returns the paragraph level, or `None` when the whole line stays at level 0 untouched.
#[allow(
    clippy::too_many_lines,
    reason = "one pass per rule, in the standard's order"
)]
pub(crate) fn resolve(
    line: &[Char],
    auto: bool,
    mut level: u8,
    levels: &mut [u8; MAX_LINE],
) -> Option<u8> {
    let n = line.len().min(MAX_LINE);
    let line = line.get(..n)?;

    // P2, P3: the paragraph level, and whether there is anything to do.
    let mut isolates = 0u32;
    let mut found = None;
    let mut has_rtl = false;
    for c in line {
        let t = c.class;
        match t {
            LRI | RLI | FSI => {
                has_rtl = true;
                isolates += 1;
            }
            PDI => {
                has_rtl = true;
                isolates = isolates.saturating_sub(1);
            }
            _ if isolates > 0 => {}
            R | AL => {
                has_rtl = true;
                found.get_or_insert(1);
                break;
            }
            RLE | LRE | RLO | LRO | PDF => {
                has_rtl = true;
                if found.is_some() {
                    break;
                }
            }
            L => {
                found.get_or_insert(0);
            }
            AN => has_rtl = true,
            _ => {}
        }
    }
    if auto && let Some(found) = found {
        level = found;
    }
    if !has_rtl && level == 0 {
        return None;
    }
    let sos = direction(level);

    let mut types = [ON; MAX_LINE];
    let mut skip = [false; MAX_LINE];
    let (Some(types), Some(levels), Some(skip)) =
        (types.get_mut(..n), levels.get_mut(..n), skip.get_mut(..n))
    else {
        return Some(level);
    };
    let ty = |types: &[Class], i: usize| types.get(i).copied().unwrap_or(ON);

    // X1-X8: embedding levels.
    let mut embedding = level;
    let mut over = ON; // no override
    let mut isolate = false;
    let mut isolates = 0u32;
    let mut stack = Stack {
        entries: [(0, ON, false); MAX_LINE + 1],
        len: 0,
    };
    stack.push((embedding, over, isolate));
    for i in 0..n {
        let mut t = line.get(i).map_or(ON, |c| c.class);
        if let Some(l) = levels.get_mut(i) {
            *l = embedding;
        }
        if t == FSI {
            // Its direction is that of the first strong character before the matching PDI.
            t = LRI;
            let mut depth = 0u32;
            for c in line.iter().skip(i + 1) {
                match c.class {
                    FSI | RLI | LRI => depth += 1,
                    PDI if depth > 0 => depth -= 1,
                    PDI | L => break,
                    R | AL => {
                        t = RLI;
                        break;
                    }
                    _ => {}
                }
            }
        }
        match t {
            RLE | LRE | RLO | LRO => {
                embedding = if matches!(t, RLE | RLO) {
                    least_greater_odd(embedding)
                } else {
                    least_greater_even(embedding)
                };
                over = match t {
                    RLO => R,
                    LRO => L,
                    _ => ON,
                };
                isolate = false;
                stack.push((embedding, over, isolate));
            }
            RLI | LRI => {
                embedding = if t == RLI {
                    least_greater_odd(embedding)
                } else {
                    least_greater_even(embedding)
                };
                if over != ON {
                    t = over;
                }
                isolates += 1;
                over = ON;
                isolate = true;
                stack.push((embedding, over, isolate));
            }
            PDF => {
                if !isolate && stack.len >= 2 {
                    (embedding, over, isolate) = stack.pop();
                }
                if let Some(l) = levels.get_mut(i) {
                    *l = embedding;
                }
            }
            PDI => {
                if isolates > 0 {
                    while !isolate && stack.len > 0 {
                        isolate = stack.pop().2;
                    }
                    (embedding, over, isolate) = stack.pop();
                    isolates -= 1;
                }
                if over != ON {
                    t = over;
                }
                if let Some(l) = levels.get_mut(i) {
                    *l = embedding;
                }
            }
            _ => {
                if over != ON {
                    t = over;
                }
            }
        }
        if let Some(slot) = types.get_mut(i) {
            *slot = t;
        }
    }

    // X9: the embedding controls and BN stay in place as NSM, and later rules skip them.
    for (t, s) in types.iter_mut().zip(skip.iter_mut()) {
        *s = matches!(*t, RLE | LRE | RLO | LRO | PDF | BN);
        if *s {
            *t = NSM;
        }
    }

    // W1: NSM takes the type before it (sos at the start); after an isolate control, ON.
    if ty(types, 0) == NSM
        && let Some(t) = types.first_mut()
    {
        *t = sos;
    }
    for i in 1..n {
        if ty(types, i) == NSM {
            let before = match ty(types, i - 1) {
                LRI | RLI | FSI | PDI => ON,
                t => t,
            };
            if let Some(t) = types.get_mut(i) {
                *t = before;
            }
        }
    }

    // W2: EN after AL (back to the last strong type) is AN.
    for i in 0..n {
        if ty(types, i) == EN {
            for j in (0..i).rev() {
                match ty(types, j) {
                    AL => {
                        if let Some(t) = types.get_mut(i) {
                            *t = AN;
                        }
                        break;
                    }
                    R | L => break,
                    _ => {}
                }
            }
        }
    }

    // W3: AL is R.
    for t in types.iter_mut() {
        if *t == AL {
            *t = R;
        }
    }

    // W4: one separator between two numbers of a type takes it.
    for i in 1..n.saturating_sub(1) {
        let t = ty(types, i);
        if t != ES && t != CS {
            continue;
        }
        let prev = (0..i)
            .rev()
            .find(|&j| !skip.get(j).copied().unwrap_or(false));
        let next = (i + 1..n).find(|&j| !skip.get(j).copied().unwrap_or(false));
        let (Some(prev), Some(next)) = (prev, next) else {
            continue;
        };
        let (before, after) = (ty(types, prev), ty(types, next));
        let mut t = t;
        if t == ES && before == EN && after == EN {
            t = EN;
        }
        if t == CS {
            if before == EN && after == EN {
                t = EN;
            }
            if before == AN && after == AN {
                t = AN;
            }
        }
        if let Some(slot) = types.get_mut(i) {
            *slot = t;
        }
    }

    // W5: a sequence of ET next to EN is EN; a forward pass, then a backward one.
    let skipped = |skip: &[bool], j: usize| skip.get(j).copied().unwrap_or(true);
    for i in 0..n {
        if skipped(skip, i) || ty(types, i) != ET {
            continue;
        }
        if let Some(j) = (i..n).find(|&j| !skipped(skip, j) && ty(types, j) != ET)
            && ty(types, j) == EN
            && let Some(t) = types.get_mut(i)
        {
            *t = EN;
        }
    }
    for i in (0..n).rev() {
        if skipped(skip, i) || ty(types, i) != ET {
            continue;
        }
        if let Some(j) = (0..=i)
            .rev()
            .find(|&j| !skipped(skip, j) && ty(types, j) != ET)
            && ty(types, j) == EN
            && let Some(t) = types.get_mut(i)
        {
            *t = EN;
        }
    }

    // W6: the separators and terminators left are ON.
    for t in types.iter_mut() {
        if matches!(*t, ES | ET | CS) {
            *t = ON;
        }
    }

    // W7: EN after L (back to sos) is L.
    let mut last_strong = sos;
    for (t, &s) in types.iter_mut().zip(skip.iter()) {
        if s {
            continue;
        }
        if matches!(*t, L | R) {
            last_strong = *t;
        }
        if *t == EN && last_strong == L {
            *t = L;
        }
    }

    // N0: bracket pairs take the embedding direction when it is inside them, else the opposite
    // direction found inside when the context before them has it too.
    let e = sos;
    let o = if e == L { R } else { L };
    // (opening bracket, position): every bracket and position fits the smaller types.
    let mut openers = [(0u16, 0u8); MAX_BRACKETS];
    let mut open = 0;
    for i in 0..n {
        if skipped(skip, i) {
            continue;
        }
        let cp = line.get(i).map_or(0, |c| c.cp);
        let Some(opener) = bracket(cp) else {
            continue;
        };
        if opener == cp {
            if let (Some(slot), Ok(c), Ok(pos)) =
                (openers.get_mut(open), u16::try_from(cp), u8::try_from(i))
            {
                *slot = (c, pos);
                open += 1;
            }
            continue;
        }
        let Some(k) = (0..open)
            .rev()
            .find(|&k| openers.get(k).is_some_and(|&(c, _)| u32::from(c) == opener))
        else {
            continue;
        };
        let start = openers.get(k).map_or(0, |&(_, pos)| usize::from(pos));
        open = k;

        let mut found_e = false;
        let mut found_o = false;
        for m in start + 1..i {
            if skipped(skip, m) {
                continue;
            }
            match ty(types, m) {
                EN | AN | R | AL => {
                    if e == R {
                        found_e = true;
                    } else {
                        found_o = true;
                    }
                }
                L => {
                    if e == L {
                        found_e = true;
                    } else {
                        found_o = true;
                    }
                }
                _ => {}
            }
        }
        let dir = if found_e {
            e
        } else if found_o {
            let context = strong_from(types, skip, (0..start).rev()).unwrap_or(e);
            if context == o { o } else { e }
        } else {
            continue;
        };
        for m in start..=i {
            if let Some(t) = types.get_mut(m)
                && (m == start || m == i || is_neutral_or_isolate(*t))
            {
                *t = dir;
            }
        }
    }

    // N1: a run of neutrals between two strong types of one direction takes it (sos/eos at the
    // ends).
    let mut i = 0;
    while i < n {
        if skipped(skip, i) || !is_neutral_or_isolate(ty(types, i)) {
            i += 1;
            continue;
        }
        let mut end = i;
        while end + 1 < n && (skipped(skip, end + 1) || is_neutral_or_isolate(ty(types, end + 1))) {
            end += 1;
        }
        let before = strong_from(types, skip, (0..i).rev()).unwrap_or(sos);
        let after = strong_from(types, skip, end + 1..n).unwrap_or(sos);
        if before == after {
            for t in types.get_mut(i..=end).unwrap_or_default() {
                *t = before;
            }
        }
        i = end + 1;
    }

    // N2: the other neutrals take the embedding direction.
    for (t, &l) in types.iter_mut().zip(levels.iter()) {
        if is_neutral_or_isolate(*t) {
            *t = direction(l);
        }
    }

    // I1, I2: resolved levels. (Rule L1 would come next; in this version it never applied, the
    // whitespace having been resolved by N1/N2, so it is left out.)
    for ((&t, l), &s) in types.iter().zip(levels.iter_mut()).zip(skip.iter()) {
        if s {
            continue;
        }
        let raise = match (*l & 1 == 1, t) {
            (false, R) | (true, L | EN | AN) => 1,
            (false, AN | EN) => 2,
            _ => 0,
        };
        *l = l.wrapping_add(raise);
    }

    Some(level)
}

/// Reorders `line` into visual order and mirrors its right-to-left glyphs (rules L2, L4), with
/// the levels [`resolve`] gives. Returns the paragraph level.
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

    // L4: mirror the glyphs at odd levels. The C version tests the logical level of each visual
    // position; kept as it was.
    for (c, &l) in line.iter_mut().zip(levels.iter()) {
        if l & 1 == 1 {
            c.cp = mirror(c.cp);
        }
    }
    level
}
