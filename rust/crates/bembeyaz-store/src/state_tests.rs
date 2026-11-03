//! `CrossPointState`'s own answers replayed: every step of testdata/state-corpus.txt (written by
//! `test/crosspoint_state/make_corpus.py`).
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::corpus::{bytes, fuzz, hex, replay};
use crate::state::{RECENT, Ring, State, read, write};
use std::format;
use std::string::{String, ToString};
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/state-corpus.txt");
fn list(images: &[u16]) -> String {
    images
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn images(csv: &str) -> [u16; RECENT as usize] {
    let mut out = [0; RECENT as usize];
    for (slot, item) in out.iter_mut().zip(csv.split(',')) {
        *slot = item.parse().unwrap();
    }
    out
}

fn state_of(s: &State) -> String {
    let r = &s.recent_sleep;
    let o = &s.recent_overlay_sleep;
    format!(
        "{} {} {} {} {} {} {} {} {} {}",
        hex(&s.open_epub_path),
        list(&r.images),
        r.pos,
        r.fill,
        list(&o.images),
        o.pos,
        o.fill,
        s.reader_activity_load_count,
        u8::from(s.last_sleep_from_reader),
        u8::from(s.show_boot_screen)
    )
}

fn json_of(s: &State) -> String {
    String::from_utf8(write(s).unwrap()).unwrap()
}

fn from(a: &[&str]) -> String {
    let mut s = match read(&bytes(a[1])) {
        Ok(s) => s,
        Err(e) => return format!("err {}", e.name()),
    };
    let (idx, count, push): (u16, u8, u16) = (
        a[2].parse().unwrap(),
        a[3].parse().unwrap(),
        a[4].parse().unwrap(),
    );
    let before = state_of(&s);
    let json = json_of(&s);
    let recent = s.recent_sleep.is_recent(idx, count);
    let overlay = s.recent_overlay_sleep.is_recent(idx, count);
    s.recent_sleep.push(push);
    s.recent_overlay_sleep.push(push);
    format!(
        "{before} | {json} | {} {} | {}",
        u8::from(recent),
        u8::from(overlay),
        state_of(&s)
    )
}

fn to(a: &[&str]) -> String {
    let ring = |images_at: usize| Ring {
        images: images(a[images_at]),
        pos: a[images_at + 1].parse().unwrap(),
        fill: a[images_at + 2].parse().unwrap(),
    };
    let s = State {
        open_epub_path: bytes(a[1]),
        recent_sleep: ring(2),
        recent_overlay_sleep: ring(5),
        reader_activity_load_count: a[8].parse().unwrap(),
        last_sleep_from_reader: a[9] == "1",
        show_boot_screen: a[10] == "1",
    };
    json_of(&s)
}

#[test]
fn every_step_of_the_corpus_is_the_stores() {
    replay(CORPUS, 1500, |a| match a[0] {
        "from" => from(a),
        "to" => to(a),
        other => panic!("unknown step {other}"),
    });
}

#[test]
fn broken_state_files_are_read_without_a_fault() {
    fuzz(CORPUS, 5, |input| {
        let _ = read(input);
    });
}
