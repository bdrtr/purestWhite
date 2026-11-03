//! `WifiCredentialStore`'s stored-text layers replayed: for every step of
//! testdata/wifi-credentials-corpus.txt (written by `test/wifi_credentials/make_corpus.py`), what
//! the file held (`read`) and what the store wrote (`write`, its lengths and CRC-32s too), as the
//! driver worked them out; for every load, the networks [`keep`] keeps, the resave, and the C++
//! store's log lines for the ones dropped; then the drops, the limit and running out of memory, by
//! hand. The C++ store's answers through the shim are checked by `check_shim.sh`.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::corpus::{bytes, deobfuscate, fuzz, hex, steps};
use crate::field::Number;
use crate::password::{Decoded, Secret};
use crate::wifi_credentials::{
    Checksum, Limits, MOST_NETWORKS, MOST_PASSWORD, Saved, Why, crc32, keep, read, write,
};
use core::fmt::Write as _;
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/wifi-credentials-corpus.txt");

fn number(n: Number) -> String {
    format!("{}:{}", n.kind as u8, n.value)
}

/// The driver's read layer for `file`.
fn read_layer(file: &[u8]) -> String {
    let stored = match read(file) {
        Ok(stored) => stored,
        Err(e) => return format!("err {}", e.name()),
    };
    let mut out = format!(
        "{} {}",
        hex(&stored.last_connected_ssid),
        stored.credentials.len()
    );
    for c in &stored.credentials {
        let texts = [&c.ssid, &c.password_obf, &c.password].map(|t| hex(t));
        let _ = write!(
            out,
            " {},{},{}",
            texts.join(","),
            number(c.password_len),
            number(c.password_crc32)
        );
    }
    out
}

/// `wifi.json`'s text for the driver's save layer.
fn written(layer: &str) -> String {
    let mut items = layer.split(' ');
    let last = bytes(items.next().unwrap());
    let saved: Vec<Saved<Vec<u8>>> = items
        .map(|item| {
            let f: Vec<&str> = item.split(',').collect();
            // The stand-in's obfuscation undone: Rust works the length and CRC-32 out of it, which
            // must be the ones the C++ store saved.
            let password = deobfuscate(&bytes(f[1])).unwrap_or_default();
            assert_eq!(f[2], format!("{}", password.len()), "{item}");
            assert_eq!(f[3], format!("{}", crc32(&password)), "{item}");
            Saved {
                ssid: bytes(f[0]),
                password_obf: bytes(f[1]),
                password,
            }
        })
        .collect();
    String::from_utf8(write(&last, &saved).unwrap()).unwrap()
}

#[test]
fn every_layer_of_the_corpus_is_the_stores() {
    let (mut reads, mut saves) = (0, 0);
    for (line, _, answer) in steps(CORPUS) {
        let (Some(at_read), Some(at_save)) = (answer.rfind(" | read "), answer.rfind(" | save "))
        else {
            continue; // a long step
        };
        let read_want = &answer[at_read + 8..at_save];
        let save_want = &answer[at_save + 8..];
        if let Some(file) = line.strip_prefix("load ") {
            assert_eq!(read_layer(&bytes(file)), read_want, "{line}");
            reads += 1;
        }
        if save_want != "-" {
            let json_at = answer.find(" | ").unwrap() + 3;
            assert_eq!(written(save_want), answer[json_at..at_read], "{line}");
            saves += 1;
        }
    }
    assert!(reads > 700 && saves > 200, "{reads} reads, {saves} saves");
}

#[test]
fn broken_wifi_files_are_read_without_a_fault() {
    fuzz(CORPUS, 20, |input| {
        let _ = read(input);
    });
}

/// The host stand-in's decoding (`test/json_oracle/stubs/ObfuscationFake.cpp`): `~` and the text
/// in hex.
#[allow(clippy::unnecessary_wraps, reason = "a decoder's shape")]
fn fake_decode(text: &[u8], most: usize) -> Result<Decoded, crate::Error> {
    Ok(match deobfuscate(text) {
        None => Decoded::default(),
        Some(text) if text.len() > most => Decoded {
            too_long: true,
            ..Decoded::default()
        },
        Some(text) => Decoded {
            text: Secret::new(text),
            ok: true,
            too_long: false,
        },
    })
}

/// The log lines the C++ store wrote for a network dropped.
fn log_lines(ssid: &[u8], why: Why) -> Vec<Vec<u8>> {
    let line = |what: &str| {
        let (kind, rest) = what.split_once('|').unwrap();
        [
            format!("WCS: Discarding {kind} password for ").as_bytes(),
            ssid,
            rest.as_bytes(),
        ]
        .concat()
    };
    match why {
        Why::InvalidLength => std::vec![line("corrupted| (invalid length)")],
        Why::OversizedLength(n) => std::vec![line(&format!("oversized| ({n} bytes)"))],
        Why::OversizedPassword => std::vec![line("oversized|")],
        Why::Corrupted { length, checksum } => {
            let mut lines = Vec::new();
            if let Some((stored, decoded)) = length {
                lines.push(line(&format!(
                    "corrupted| (expected {stored} bytes, decoded {decoded})"
                )));
            }
            match checksum {
                Some(Checksum::Mismatch) => lines.push(line("corrupted| (checksum mismatch)")),
                Some(Checksum::Invalid) => lines.push(line("corrupted| (invalid checksum)")),
                None => {}
            }
            lines
        }
    }
}

#[test]
fn every_load_keeps_the_networks_and_logs_the_drops_the_cpp_store_did() {
    let limits = Limits {
        networks: MOST_NETWORKS,
        password: MOST_PASSWORD,
    };
    let (mut loads, mut logged) = (0, 0);
    for (line, logs, answer) in steps(CORPUS) {
        let (Some(file), Some(networks)) = (line.strip_prefix("load "), answer.strip_prefix("ok "))
        else {
            continue;
        };
        let json_at = answer.find(" | ").unwrap();
        let resaved = !answer[json_at + 3..].starts_with("- | read ");
        let mut dropped = Vec::new();
        let got = keep(&bytes(file), limits, fake_decode, |d| {
            dropped.extend(log_lines(d.ssid, d.why));
        })
        .unwrap();
        let mut kept = format!("{} {}", hex(&got.last_connected_ssid), got.kept.len());
        for k in &got.kept {
            let _ = write!(kept, " {},{}", hex(&k.ssid), hex(&k.password));
        }
        assert_eq!(kept, networks[..json_at - 3], "{line}");
        assert_eq!(got.resave, resaved, "{line}");
        let store_logs: Vec<Vec<u8>> = logs
            .into_iter()
            .filter(|l| l.starts_with(b"WCS: "))
            .collect();
        assert_eq!(dropped, store_logs, "{line}");
        loads += 1;
        logged += store_logs.len();
    }
    assert!(
        loads > 600 && logged > 1000,
        "{loads} loads, {logged} log lines"
    );
}

#[test]
fn crc32_is_ieees() {
    assert_eq!(crc32(b""), 0);
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

/// The names kept, the names dropped with why, and the resave.
type Judged = (Vec<Vec<u8>>, Vec<(Vec<u8>, Why)>, bool);

/// The networks `file` keeps under `limits` and the ones it drops, with why.
fn kept_and_dropped(file: &[u8], limits: Limits) -> Judged {
    let mut dropped = Vec::new();
    let got = keep(file, limits, fake_decode, |d| {
        dropped.push((d.ssid.to_vec(), d.why));
    })
    .unwrap();
    let kept = got.kept.iter().map(|k| k.ssid.clone()).collect();
    (kept, dropped, got.resave)
}

#[test]
fn each_network_dropped_comes_with_why_in_the_files_order() {
    let limits = Limits {
        networks: 8,
        password: 4,
    };
    let crc = crc32(b"abc");
    let file = format!(
        r#"{{"credentials":[
            {{"ssid":"a","password_obf":"~616263","password_len":"3","password_crc32":{crc}}},
            {{"ssid":"b","password_obf":"~616263","password_len":9,"password_crc32":{crc}}},
            {{"ssid":"c","password_obf":"~6162636465","password_len":3,"password_crc32":{crc}}},
            {{"ssid":"d","password_obf":"~616263","password_len":2,"password_crc32":1}},
            {{"ssid":"e","password_obf":"~616263","password_len":3,"password_crc32":"x"}},
            {{"ssid":"g","password_obf":"~616263","password_len":2,"password_crc32":{crc}}},
            {{"ssid":"h","password_obf":"~616263","password_len":3,"password_crc32":7}},
            {{"ssid":"i","password":"abcde"}},
            {{"ssid":"k","password_obf":"~616263","password_len":4294967296,"password_crc32":{crc}}},
            {{"ssid":"j","password_obf":"~61626364","password_len":4,"password_crc32":{}}},
            {{"ssid":"f","password_obf":"~616263","password_len":3,"password_crc32":{crc}}}
        ]}}"#,
        crc32(b"abcd")
    );
    let (kept, dropped, resave) = kept_and_dropped(file.as_bytes(), limits);
    assert_eq!(kept, [b"j".to_vec(), b"f".to_vec()]);
    let corrupted = |length, checksum| Why::Corrupted { length, checksum };
    let want = [
        (&b"a"[..], Why::InvalidLength),
        (b"b", Why::OversizedLength(9)),
        (b"c", Why::OversizedPassword),
        (b"d", corrupted(Some((2, 3)), Some(Checksum::Mismatch))),
        (b"e", corrupted(None, Some(Checksum::Invalid))),
        (b"g", corrupted(Some((2, 3)), None)),
        (b"h", corrupted(None, Some(Checksum::Mismatch))),
        (b"i", Why::OversizedPassword),
        // Past u32, as the device reads a size_t (the host's oracle would call it oversized).
        (b"k", Why::InvalidLength),
    ];
    let want: Vec<(Vec<u8>, Why)> = want.iter().map(|(s, w)| (s.to_vec(), *w)).collect();
    assert_eq!(dropped, want);
    assert!(resave);
}

#[test]
fn only_kept_networks_count_toward_the_limit_and_none_past_it_is_judged() {
    let limits = Limits {
        networks: 2,
        password: 64,
    };
    let good = |ssid: &str| {
        format!(
            r#"{{"ssid":"{ssid}","password_obf":"~61","password_len":1,"password_crc32":{}}}"#,
            crc32(b"a")
        )
    };
    let bad = |ssid: &str| format!(r#"{{"ssid":"{ssid}","password_len":"x"}}"#);
    let items = [
        bad("1"),
        good("2"),
        bad("3"),
        good("4"),
        bad("5"),
        good("6"),
    ]
    .join(",");
    let file = format!(r#"{{"lastConnectedSsid":"4","credentials":[{items}]}}"#);
    let (kept, dropped, _) = kept_and_dropped(file.as_bytes(), limits);
    assert_eq!(kept, [b"2".to_vec(), b"4".to_vec()]);
    let names: Vec<Vec<u8>> = dropped.into_iter().map(|(s, _)| s).collect();
    assert_eq!(names, [b"1".to_vec(), b"3".to_vec()]);
}

#[test]
fn a_decoder_out_of_memory_is_an_error_not_a_dropped_network() {
    let limits = Limits {
        networks: 8,
        password: 64,
    };
    let failing = |_: &[u8], _: usize| Err(crate::Error::NoMemory);
    let file = br#"{"credentials":[{"ssid":"a","password_obf":"~61"}]}"#;
    let mut dropped = 0;
    assert_eq!(
        keep(file, limits, failing, |_| dropped += 1),
        Err(crate::Error::NoMemory)
    );
    assert_eq!(dropped, 0);
}
