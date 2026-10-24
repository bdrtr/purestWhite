//! The links and footnotes that go with the lines a chapter's page break moves to the next page
//! (lib/Epub's `ChapterHtmlSlimParser`): links that reach below the moved lines' top move, and so
//! do the footnotes they refer to, which also stay where a remaining link refers to them.

use crate::page::{Footnote, Link};

/// `bytes` up to its first NUL.
fn c_str(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap_or_default()
}

fn refers_to(links: &[Link], note: &Footnote) -> bool {
    let href = c_str(&note.href);
    links.iter().any(|link| c_str(&link.href) == href)
}

/// Moves `items` for which `stays` is false to the end, keeping the order of both groups, and
/// returns how many stay. Pages hold few links and footnotes, so it shifts rather than buffers.
fn partition<T>(items: &mut [T], mut stays: impl FnMut(&T) -> bool) -> usize {
    let mut kept = 0;
    for i in 0..items.len() {
        if items.get(i).is_some_and(&mut stays) {
            if let Some(run) = items.get_mut(kept..=i) {
                run.rotate_right(1);
            }
            kept += 1;
        }
    }
    kept
}

/// Moves the links that reach below `top` (the moved lines' top) to the end of `links`, both
/// groups in order, and returns how many stay.
pub fn split_links(links: &mut [Link], top: i16) -> usize {
    partition(links, |link| {
        i32::from(link.y) + i32::from(link.height) <= i32::from(top)
    })
}

/// Hands `moved`, in order, each of `notes` that a `carried` link refers to, and keeps at the
/// front, in order, the notes that a `staying` link refers to or no carried one does; returns how
/// many stay.
pub fn split_footnotes(
    notes: &mut [Footnote],
    staying: &[Link],
    carried: &[Link],
    mut moved: impl FnMut(&Footnote),
) -> usize {
    partition(notes, |note| {
        if !refers_to(carried, note) {
            return true;
        }
        moved(note);
        refers_to(staying, note)
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]

    use super::*;
    use crate::page::{HREF_LEN, NUMBER_LEN};
    use std::vec::Vec;

    fn link(href: &str, y: i16, height: i16) -> Link {
        let mut l = Link {
            href: [0; HREF_LEN],
            x: 0,
            y,
            width: 10,
            height,
        };
        l.href[..href.len()].copy_from_slice(href.as_bytes());
        l
    }

    fn note(number: &str, href: &str) -> Footnote {
        let mut n = Footnote {
            number: [0; NUMBER_LEN],
            href: [0; HREF_LEN],
        };
        n.number[..number.len()].copy_from_slice(number.as_bytes());
        n.href[..href.len()].copy_from_slice(href.as_bytes());
        n
    }

    fn hrefs(links: &[Link]) -> Vec<&[u8]> {
        links.iter().map(|l| c_str(&l.href)).collect()
    }

    #[test]
    fn links_below_the_top_move_in_order() {
        let mut links = [
            link("#a", 0, 20),
            link("#b", 90, 20),
            link("#c", 70, 30),
            link("#d", 80, 20),
            link("#e", 40, 20),
        ];
        // Ending exactly at the top stays; ending below it moves.
        let kept = split_links(&mut links, 100);
        assert_eq!(kept, 4);
        assert_eq!(hrefs(&links), [&b"#a"[..], b"#c", b"#d", b"#e", b"#b"]);
        let mut low = [link("#x", 95, 10), link("#y", 99, 1), link("#z", 0, 10)];
        assert_eq!(split_links(&mut low, 100), 2);
        assert_eq!(hrefs(&low), [&b"#y"[..], b"#z", b"#x"]);
        // Moved links ahead of staying ones keep their order behind them.
        let mut mixed = [
            link("#1", 95, 10),
            link("#2", 96, 10),
            link("#3", 0, 10),
            link("#4", 97, 10),
        ];
        assert_eq!(split_links(&mut mixed, 100), 1);
        assert_eq!(hrefs(&mixed), [&b"#3"[..], b"#1", b"#2", b"#4"]);
    }

    #[test]
    fn footnotes_follow_their_links() {
        let staying = [link("#1", 0, 10), link("#3", 0, 10)];
        let carried = [link("#2", 90, 10), link("#3", 95, 10), link("#9", 99, 10)];
        let mut notes = [
            note("1", "#1"),
            note("2", "#2"),
            note("3", "#3"),
            note("4", "#4"),
        ];
        let mut next = Vec::new();
        let kept = split_footnotes(&mut notes, &staying, &carried, |n| {
            next.push(c_str(&n.number).to_vec());
        });
        // 2 moves, 3 is on both pages, 1 and 4 stay.
        assert_eq!(next, [b"2".to_vec(), b"3".to_vec()]);
        assert_eq!(kept, 3);
        let numbers: Vec<_> = notes.iter().map(|n| c_str(&n.number)).collect();
        assert_eq!(numbers, [&b"1"[..], b"3", b"4", b"2"]);
    }
}
