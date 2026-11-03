//! The C++ rules' golden corpus replayed (testdata/binding-corpus.txt, written by
//! `test/ble_key_binding/make_corpus.py`: each step and the C++ answer after it), and the C++
//! firmware's own cases (lib/BlePageTurner/test/KeyBindingTest.cpp).
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use super::{
    Action, Binding, HoldWait, MAX_BINDINGS, PageKeys, RemoteTable, Remotes, default_table, usage,
};
use crate::profiles::profile_index;
use core::fmt::Write as _;
use std::format;
use std::string::{String, ToString};
use std::vec::Vec;

const CORPUS: &str = include_str!("../../testdata/binding-corpus.txt");

/// The table the link picked: saved (its index) or a profile's default.
#[derive(Clone, Copy)]
enum Pick {
    Saved(usize),
    Default(usize),
}

#[derive(Default)]
struct Replay {
    remotes: Remotes,
    pick: Option<Pick>,
    wait: HoldWait,
}

/// The bytes of a hex word, up to the first NUL, as the C++ reads its `const char*` (the cut is the
/// caller's in Rust: the radio and the settings file give no NUL).
fn text(hex: &str) -> Vec<u8> {
    if hex == "-" {
        return Vec::new();
    }
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .take_while(|b| *b != 0)
        .collect()
}

fn hex(word: &str) -> u32 {
    u32::from_str_radix(word, 16).unwrap()
}

fn dec(word: &str) -> u32 {
    word.parse().unwrap()
}

fn action(word: &str) -> Action {
    Action::from_number(u8::try_from(dec(word)).unwrap()).unwrap()
}

fn number(a: Action) -> String {
    (a as u8).to_string()
}

fn dump(t: &RemoteTable) -> String {
    let mut addr = String::new();
    for b in t.addr() {
        write!(addr, "{b:02x}").unwrap();
    }
    if addr.is_empty() {
        addr.push('-');
    }
    write!(addr, " {}", t.bindings().len()).unwrap();
    for b in t.bindings() {
        write!(addr, " {:08x}", b.bits()).unwrap();
    }
    addr
}

impl Replay {
    fn changed(&mut self) {
        self.pick = None;
        self.wait = HoldWait::default();
    }

    fn picked(&self) -> Option<&RemoteTable> {
        match self.pick? {
            Pick::Saved(i) => self.remotes.tables().get(i),
            Pick::Default(j) => super::table::default_tables().get(j),
        }
    }

    fn index_of(&self, table: *const RemoteTable) -> usize {
        let tables = self.remotes.tables();
        tables.iter().position(|t| core::ptr::eq(t, table)).unwrap()
    }

    fn pick(&mut self, addr: &[u8], name: &[u8]) -> String {
        self.wait = HoldWait::default();
        let found = self.remotes.table_for(addr, name).map(core::ptr::from_ref);
        let saved = found.and_then(|t| {
            let tables = self.remotes.tables();
            tables.iter().position(|s| core::ptr::eq(s, t))
        });
        self.pick = match (found, saved) {
            (None, _) => None,
            (Some(_), Some(i)) => Some(Pick::Saved(i)),
            (Some(_), None) => Some(Pick::Default(profile_index(name).unwrap())),
        };
        let routes = u8::from(self.picked().is_some_and(RemoteTable::routes));
        match self.pick {
            None => String::from("none 0"),
            Some(Pick::Saved(i)) => format!("saved {i} {routes}"),
            Some(Pick::Default(j)) => format!("default {j} {routes}"),
        }
    }

    /// Saved tables read back as the C++ loader reads them, replacing the tables.
    fn load(&mut self, a: &[&str]) -> String {
        self.remotes = Remotes::default();
        let n = usize::try_from(dec(a[0])).unwrap();
        for i in 0..n {
            let slots = a[2 + 2 * i].split(',').filter(|s| *s != "-").map(hex);
            let Some(table) = RemoteTable::saved(&text(a[1 + 2 * i]), slots) else {
                continue;
            };
            if !self.remotes.push_saved(table) {
                break;
            }
        }
        self.changed();
        self.remotes.tables().len().to_string()
    }

    fn table_step(&mut self, cmd: &str, a: &[&str]) -> String {
        let i = usize::try_from(dec(a[0])).unwrap();
        if i >= self.remotes.tables().len() {
            return String::from("no table");
        }
        let t = self.remotes.table_mut(i);
        let out = match cmd {
            "learn" => u8::from(t.learn(action(a[1]), hex(a[2]), dec(a[3]) != 0)).to_string(),
            "clear" => u8::from(t.clear_action(action(a[1]))).to_string(),
            "find" => match t.find(action(a[1])) {
                Some(b) => format!("1 {:08x}", b.bits()),
                None => String::from("0 00000000"),
            },
            "dump" => return dump(t),
            other => panic!("step {other}"),
        };
        if cmd != "find" {
            self.changed();
        }
        out
    }

    fn other_step(&mut self, cmd: &str, a: &[&str]) -> String {
        match cmd {
            "lookup" => number(
                self.picked()
                    .map_or(Action::None, |t| t.lookup(hex(a[0]), dec(a[1]) != 0)),
            ),
            "edge" => {
                let Some(t) = self.picked().copied() else {
                    return number(Action::None);
                };
                let (code, pressed, at) = (hex(a[0]), dec(a[1]) != 0, dec(a[2]));
                number(self.wait.on_raw_edge(&t, code, pressed, at, action(a[3])))
            }
            "hold" => number(self.wait.poll(dec(a[0]))),
            "valid" => u8::from(Binding::from_bits(hex(a[0])).is_some()).to_string(),
            "format" => Binding(hex(a[0])).to_string(),
            "name" => {
                let n = u8::try_from(dec(a[0])).unwrap();
                String::from(Action::from_number(n).unwrap_or_default().name())
            }
            "page" => {
                let keys = PageKeys {
                    enabled: dec(a[0]) != 0,
                    prev: u8::try_from(hex(a[1])).unwrap(),
                    next: u8::try_from(hex(a[2])).unwrap(),
                };
                let (key, mods) = (hex(a[3]), hex(a[4]));
                number(keys.action(u8::try_from(key).unwrap(), u8::try_from(mods).unwrap()))
            }
            _ => self.table_step(cmd, a),
        }
    }

    fn step(&mut self, line: &str) -> String {
        let words: Vec<&str> = line.split(' ').collect();
        let (cmd, a) = (words[0], &words[1..]);
        match cmd {
            "edit" => {
                let found = self
                    .remotes
                    .editable(&text(a[0]), &text(a[1]))
                    .map(|t| core::ptr::from_mut(t).cast_const());
                let at = found.map_or(-1, |t| i32::try_from(self.index_of(t)).unwrap());
                self.changed();
                format!("{at} {}", self.remotes.tables().len())
            }
            "forget" => {
                let gone = self.remotes.forget(&text(a[0]));
                self.changed();
                format!("{} {}", u8::from(gone), self.remotes.tables().len())
            }
            "pick" => self.pick(&text(a[0]), &text(a[1])),
            "load" => self.load(a),
            _ => self.other_step(cmd, a),
        }
    }
}

#[test]
fn every_case_of_the_c_corpus_gives_the_same_answers() {
    let mut cases = 0;
    let mut replay = Replay::default();
    let mut lines = CORPUS.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with("case ") {
            replay = Replay::default();
            cases += 1;
            continue;
        }
        let want = lines.next().unwrap().strip_prefix("  = ").unwrap();
        assert_eq!(replay.step(line), want, "case {cases}: {line}");
    }
    assert!(cases >= 350, "{cases} cases");
}

// --- The C++ firmware's cases (KeyBindingTest.cpp) ---------------------------------------------

#[test]
fn action_numbers_never_change() {
    for (n, a) in Action::ALL.iter().enumerate() {
        assert_eq!(usize::from(*a as u8), n);
    }
    assert_eq!(Action::Back as u8, 8);
    assert_eq!(
        Binding::new(0x03_0102, false, Action::NextChapter).bits(),
        0x3003_0102
    );
    assert_eq!(
        Binding::new(0x03_0008, true, Action::PrevChapter).bits(),
        0x4103_0008
    );
}

#[test]
fn a_key_identity_reads_as_the_key_code() {
    let key = Binding::new(0xFF_FF43, false, Action::NextPage);
    assert_eq!(key.to_string(), "0x43");
    let button = Binding::new(0x03_0102, false, Action::NextPage);
    assert_eq!(button.to_string(), "3:1=02");
}

#[test]
fn a_full_table_refuses_and_garbage_is_not_valid() {
    let mut t = RemoteTable::default();
    let actions = [
        Action::NextPage,
        Action::PrevPage,
        Action::NextChapter,
        Action::PrevChapter,
    ];
    // Four actions, each learned on a new button: four slots, the old ones replaced.
    for round in 0..3 {
        for (i, a) in (1..).zip(actions) {
            assert!(t.learn(a, 0x03_0000 + round * 16 + i, false));
        }
    }
    assert_eq!(t.bindings().len(), 4);
    // The same button gesture for a second action moves it, never doubles it.
    assert!(t.learn(Action::NextPage, 0x03_0000 + 32 + 2, false));
    assert_eq!(t.lookup(0x03_0000 + 32 + 2, false), Action::NextPage);
    assert_eq!(t.bindings().len(), 3);
    // A full table (eight slots read back from the card) says so instead of overwriting.
    let full =
        (0..10).map(|i| Binding::new(0x01_0000 + i + 1, i & 1 != 0, Action::NextPage).bits());
    let mut t = RemoteTable::saved(b"01:02:03:04:05:06", full).unwrap();
    assert!(!t.learn(Action::PrevPage, 0x7F_0001, false));
    assert_eq!(t.bindings().len(), MAX_BINDINGS);
    assert_eq!(t.lookup(0x7F_0001, false), Action::None);

    let valid = |b: Binding| Binding::from_bits(b.bits()).is_some();
    assert!(valid(Binding::new(0x03_0102, true, Action::PrevChapter)));
    assert!(!valid(Binding::new(0x03_0102, false, Action::None)));
    assert!(
        Binding::from_bits(0x9003_0102).is_none(),
        "action out of range"
    );
    assert!(Binding::from_bits(0x1803_0102).is_none(), "unknown bit");
    let release = Binding::new(0x03_0100, false, Action::NextPage);
    assert!(!valid(release), "a zero value is a release, not a button");
}

#[test]
fn the_address_kept_is_bounded() {
    for address in ["01", "01:02:03:04:05:06", "01:02:03:04:05:06extra"] {
        let mut remotes = Remotes::default();
        let table = remotes.editable(address.as_bytes(), b"Remote").unwrap();
        let kept = if address.len() <= 17 {
            address
        } else {
            "01:02:03:04:05:06"
        };
        assert_eq!(table.addr(), kept.as_bytes());
        assert_eq!(remotes.tables().len(), 1);
    }
}

#[test]
fn keypage_takes_both_code_sets_out_of_the_box() {
    // The same "BOOX Keypage" logged on the X3 sent "04 00 00" from the upper button and
    // "08 00 00" from the lower one, report 3. The default takes both sets.
    let none = Remotes::default();
    let t = none
        .table_for(b"AA:BB:CC:DD:EE:FF", b"BOOX Keypage")
        .unwrap();
    assert!(t.routes());
    assert_eq!(t.lookup(0x03_0004, false), Action::NextPage);
    assert_eq!(t.lookup(0x03_0008, false), Action::PrevPage);
    assert_eq!(t.lookup(0x03_0002, false), Action::NextPage);
    assert_eq!(t.lookup(0x03_0001, false), Action::PrevPage);
    assert_eq!(
        t.lookup(0x03_0004, true),
        Action::None,
        "a hold is not a tap"
    );
    assert_eq!(
        t.lookup(0x02_0004, false),
        Action::None,
        "another report id"
    );

    // A table the user bound by hand wins over the default, whatever the remote sends.
    let mut saved = Remotes::default();
    let own = saved.editable(b"AA:BB:CC:DD:EE:FF", b"").unwrap();
    assert!(own.learn(Action::NextPage, 0x03_0002, false));
    assert!(own.learn(Action::PrevPage, 0x03_0001, false));
    let own = saved
        .table_for(b"AA:BB:CC:DD:EE:FF", b"BOOX Keypage")
        .unwrap();
    assert!(core::ptr::eq(own, core::ptr::from_ref(&saved.tables()[0])));
    assert_eq!(own.lookup(0x03_0004, false), Action::None);

    // Learning a direction on a table seeded from the default replaces both of its codes.
    let mut remotes = Remotes::default();
    let edit = remotes
        .editable(b"AA:BB:CC:DD:EE:FF", b"BOOX Keypage")
        .unwrap();
    assert!(edit.learn(Action::NextPage, 0x03_0010, false));
    assert_eq!(edit.lookup(0x03_0010, false), Action::NextPage);
    assert_eq!(edit.lookup(0x03_0002, false), Action::None);
    assert_eq!(edit.lookup(0x03_0004, false), Action::None);
    assert_eq!(edit.lookup(0x03_0008, false), Action::PrevPage);

    // The other built-in default and unknown remotes are untouched.
    let free3 = default_table(b"Free3-R").unwrap();
    assert_eq!(free3.bindings().len(), 2);
    assert_eq!(free3.lookup(0x03_0004, false), Action::None);
    assert!(none.table_for(b"AA:BB:CC:DD:EE:FF", b"Other").is_none());
}

#[test]
fn mapping_one_action_keeps_other_buttons_and_remotes() {
    let mut remotes = Remotes::default();
    let first = remotes.editable(b"01:02:03:04:05:06", b"Free3").unwrap();
    assert!(first.learn(Action::NextPage, 0x03_0001, false));
    assert!(first.learn(Action::PrevPage, 0x03_0002, false));
    let second = remotes.editable(b"11:12:13:14:15:16", b"Remote").unwrap();
    assert!(second.learn(Action::NextPage, 0x03_0001, false));

    let first = remotes.editable(b"01:02:03:04:05:06", b"Free3").unwrap();
    assert!(first.learn(Action::NextChapter, 0x03_0004, false));
    assert_eq!(first.lookup(0x03_0001, false), Action::NextPage);
    assert_eq!(first.lookup(0x03_0002, false), Action::PrevPage);
    assert_eq!(first.lookup(0x03_0004, false), Action::NextChapter);
    assert_eq!(first.lookup(0x00_0008, false), Action::PrevChapter);

    assert!(first.learn(Action::PrevChapter, 0x03_0001, false));
    assert_eq!(first.lookup(0x03_0001, false), Action::PrevChapter);
    assert!(first.find(Action::NextPage).is_none());
    let second = &remotes.tables()[1];
    assert_eq!(second.lookup(0x03_0001, false), Action::NextPage);
}

#[test]
fn default_keys_turn_until_a_direction_is_learned() {
    let mut keys = PageKeys {
        enabled: true,
        ..PageKeys::default()
    };
    let prev = [
        usage::LEFT,
        usage::PAGE_UP,
        usage::UP,
        usage::BACKSPACE,
        usage::VOLUME_DOWN,
        usage::SCAN_PREV,
    ];
    for k in prev {
        assert_eq!(keys.action(k, 0), Action::PrevPage, "{k}");
    }
    let next = [
        usage::RIGHT,
        usage::PAGE_DOWN,
        usage::DOWN,
        usage::SPACE,
        usage::ENTER,
        usage::VOLUME_UP,
        usage::SCAN_NEXT,
    ];
    for k in next {
        assert_eq!(keys.action(k, 0), Action::NextPage, "{k}");
    }
    assert_eq!(
        keys.action(0x04, 0),
        Action::None,
        "plain typing turns nothing"
    );
    assert_eq!(keys.action(usage::RIGHT, 0x02), Action::None, "a modifier");
    assert_eq!(keys.action(0, 0), Action::None);

    keys.next = usage::ENTER; // a remote whose Next button is Enter
    assert_eq!(keys.action(usage::ENTER, 0), Action::NextPage);
    assert_eq!(
        keys.action(usage::RIGHT, 0),
        Action::None,
        "the learned key retires its defaults"
    );
    assert_eq!(
        keys.action(usage::LEFT, 0),
        Action::PrevPage,
        "the other direction keeps its own"
    );

    keys.enabled = false;
    assert_eq!(keys.action(usage::ENTER, 0), Action::None);
}

#[test]
fn a_held_button_waits_and_a_tapped_one_acts_now() {
    let mut t = RemoteTable::default();
    assert!(t.learn(Action::NextPage, 0x03_0002, false));
    assert!(t.learn(Action::NextChapter, 0x03_0002, true));
    assert!(t.learn(Action::PrevPage, 0x03_0001, false));
    let mut wait = HoldWait::default();
    let edge =
        |w: &mut HoldWait, code, pressed, at| w.on_raw_edge(&t, code, pressed, at, Action::None);
    // No hold slot: the press acts.
    assert_eq!(edge(&mut wait, 0x03_0001, true, 0), Action::PrevPage);
    assert_eq!(edge(&mut wait, 0x03_0001, false, 50), Action::None);
    // A hold slot: a short press is a tap on release, a long one a hold.
    assert_eq!(edge(&mut wait, 0x03_0002, true, 100), Action::None);
    assert_eq!(edge(&mut wait, 0x03_0002, false, 799), Action::NextPage);
    assert_eq!(edge(&mut wait, 0x03_0002, true, 1000), Action::None);
    assert_eq!(edge(&mut wait, 0x03_0002, false, 1700), Action::NextChapter);
    // Still down after HOLD_MS: the hold acts on the poll, the release does nothing more.
    assert_eq!(
        edge(&mut wait, 0x03_0002, true, u32::MAX - 100),
        Action::None
    );
    assert_eq!(wait.poll(598), Action::None);
    assert_eq!(
        wait.poll(599),
        Action::NextChapter,
        "across the clock's wrap"
    );
    assert_eq!(edge(&mut wait, 0x03_0002, false, 700), Action::None);
}
