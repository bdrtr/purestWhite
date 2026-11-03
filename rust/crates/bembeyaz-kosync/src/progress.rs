//! Which of two reading positions is further on, as `lib/KOReaderSync/ProgressComparison` decides
//! it (the C++ header stays; its `.cpp` calls this). Positions the reader trusts (the chapter came
//! from local state or a content anchor) are compared by chapter, then page or text offset; when
//! that does not decide, they fall back to their percentages, within 0.001 of each other being the
//! same place.

/// A position as the comparison reads it (the fields of the C++ `CrossPointPosition` it uses).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    /// The chapter's place in the spine.
    pub spine: i32,
    /// The page within the chapter.
    pub page: i32,
    /// The zero-based visible codepoint the page starts at.
    pub offset: u32,
    /// `spine` came from local state or a content anchor, not from a percentage.
    pub spine_resolved: bool,
    /// `page` came from local layout data, not from scaling a percentage.
    pub page_mapped: bool,
    /// `offset` is known.
    pub has_offset: bool,
}

/// Which position is further on (`ProgressComparison`, in its order).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    LocalAhead = 0,
    Synchronized = 1,
    RemoteAhead = 2,
    /// No trusted place decides it and a percentage is not finite.
    Unknown = 3,
}

/// Which remote record to take (`RemoteRecordChoice`, in its order).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Primary = 0,
    Alternate = 1,
}

/// Percentages this close are the same place.
const SAME_PROGRESS: f32 = 0.001;

/// `local` (at `local_percentage`) against `remote` (at `remote_percentage`).
#[must_use]
pub fn compare(
    local: &Position,
    local_percentage: f32,
    remote: &Position,
    remote_percentage: f32,
) -> Comparison {
    if local.spine_resolved
        && remote.spine_resolved
        && let Some(by_place) = by_place(local, remote)
    {
        return by_place;
    }
    if local_percentage.is_finite() && remote_percentage.is_finite() {
        let delta = local_percentage - remote_percentage;
        if delta.abs() <= SAME_PROGRESS {
            return Comparison::Synchronized;
        }
        return if delta > 0.0 {
            Comparison::LocalAhead
        } else {
            Comparison::RemoteAhead
        };
    }
    Comparison::Unknown
}

/// Two trusted positions by chapter, then page or offset; none when neither says.
fn by_place(local: &Position, remote: &Position) -> Option<Comparison> {
    if local.spine != remote.spine {
        return Some(ordered(local.spine, remote.spine));
    }
    let pages = local.page_mapped && remote.page_mapped;
    if pages && local.page == remote.page {
        return Some(Comparison::Synchronized);
    }
    if local.has_offset && remote.has_offset {
        return Some(ordered(local.offset, remote.offset));
    }
    pages.then(|| ordered(local.page, remote.page))
}

fn ordered<T: Ord + Copy>(local: T, remote: T) -> Comparison {
    match local.cmp(&remote) {
        core::cmp::Ordering::Greater => Comparison::LocalAhead,
        core::cmp::Ordering::Less => Comparison::RemoteAhead,
        core::cmp::Ordering::Equal => Comparison::Synchronized,
    }
}

/// The alternate record only when it is strictly further on than the primary one.
#[must_use]
pub fn select_remote(
    primary: &Position,
    primary_percentage: f32,
    alternate: &Position,
    alternate_percentage: f32,
) -> Choice {
    match compare(primary, primary_percentage, alternate, alternate_percentage) {
        Comparison::RemoteAhead => Choice::Alternate,
        _ => Choice::Primary,
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, Comparison, Position, compare, select_remote};

    fn mapped(spine: i32, page: i32) -> Position {
        Position {
            spine,
            page,
            spine_resolved: true,
            page_mapped: true,
            ..Position::default()
        }
    }

    #[test]
    fn a_later_chapter_wins_whatever_the_percentages() {
        let (early, late) = (mapped(3, 18), mapped(4, 1));
        assert_eq!(compare(&late, 0.1, &early, 0.9), Comparison::LocalAhead);
        assert_eq!(compare(&early, 0.9, &late, 0.1), Comparison::RemoteAhead);
    }

    #[test]
    fn in_one_chapter_the_offset_then_the_page_decides() {
        let at = |page, offset| Position {
            offset,
            has_offset: true,
            ..mapped(3, page)
        };
        assert_eq!(
            compare(&at(4, 100), 0.9, &at(5, 90), 0.1),
            Comparison::LocalAhead
        );
        assert_eq!(
            compare(&at(4, 100), 0.1, &at(4, 900), 0.9),
            Comparison::Synchronized,
            "same page"
        );
        assert_eq!(
            compare(&mapped(3, 4), 0.17, &mapped(3, 15), 0.14),
            Comparison::RemoteAhead
        );
    }

    #[test]
    fn untrusted_positions_go_by_percentage_within_the_tolerance() {
        let guess = Position::default();
        assert_eq!(
            compare(&guess, 0.5, &guess, 0.5009),
            Comparison::Synchronized
        );
        assert_eq!(compare(&guess, 0.502, &guess, 0.5), Comparison::LocalAhead);
        assert_eq!(compare(&guess, f32::NAN, &guess, 0.5), Comparison::Unknown);
        assert_eq!(
            compare(&guess, 0.5, &guess, f32::INFINITY),
            Comparison::Unknown
        );
    }

    #[test]
    fn offsets_compare_unsigned_and_chapters_signed() {
        let at = |spine, offset| Position {
            offset,
            has_offset: true,
            ..mapped(spine, 0)
        };
        let (far, near) = (at(1, 0x8000_0000), at(1, 1));
        let (far, near) = (Position { page: 1, ..far }, Position { page: 2, ..near });
        assert_eq!(compare(&far, 0.0, &near, 0.0), Comparison::LocalAhead);
        assert_eq!(
            compare(&at(-1, 0), 0.9, &at(0, 0), 0.1),
            Comparison::RemoteAhead
        );
    }

    #[test]
    fn the_alternate_record_is_taken_only_when_strictly_ahead() {
        assert_eq!(
            select_remote(&mapped(2, 4), 0.8, &mapped(3, 1), 0.2),
            Choice::Alternate
        );
        assert_eq!(
            select_remote(&mapped(2, 4), 0.8, &mapped(2, 4), 0.2),
            Choice::Primary
        );
    }
}
