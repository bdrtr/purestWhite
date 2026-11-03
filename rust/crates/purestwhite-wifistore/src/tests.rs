// The tests index what must be there and fail on what is not: a failure fails the test.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use super::base64::{DecodeError, decode};
use super::{JSON_MOST, KEY_LEN, NETWORKS_MOST, ReadError, Saved, read};
use bembeyaz_store::wifi_credentials::crc32;
use std::format;
use std::string::String;
use std::vec::Vec;

/// A made-up device key: no real device's MAC address.
const KEY: [u8; KEY_LEN] = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];

fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(DIGITS[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `password` as the C++ firmware writes it (`obfuscateToBase64`).
fn obfuscated(password: &str) -> String {
    let xored: Vec<u8> = password
        .bytes()
        .zip(KEY.iter().cycle())
        .map(|(b, k)| b ^ k)
        .collect();
    base64(&xored)
}

/// A network's object as the C++ firmware writes it (`WifiCredentialStore::toJson`).
fn network(ssid: &str, password: &str) -> String {
    format!(
        r#"{{"ssid":"{ssid}","password_obf":"{}","password_len":{},"password_crc32":{}}}"#,
        obfuscated(password),
        password.len(),
        crc32(password.as_bytes())
    )
}

fn file(last: &str, networks: &[String]) -> String {
    format!(
        r#"{{"lastConnectedSsid":"{last}","credentials":[{}]}}"#,
        networks.join(",")
    )
}

fn names(saved: &Saved) -> Vec<&str> {
    saved
        .networks()
        .map(|n| n.ssid().as_str().unwrap())
        .collect()
}

#[test]
fn passwords_are_decoded_and_the_last_joined_is_the_first_choice() {
    let json = file(
        "Office",
        &[
            network("Home", "correct horse"),
            network("Office", "b@ttery staple!"),
        ],
    );
    let saved = read(json.as_bytes(), &KEY).unwrap();
    assert_eq!(names(&saved), ["Home", "Office"]);
    let first = saved.first_choice().unwrap();
    assert_eq!(first.ssid().as_str(), Some("Office"));
    assert_eq!(first.password().as_str(), Some("b@ttery staple!"));
}

#[test]
fn without_the_last_joined_among_them_the_first_is_the_choice() {
    let json = file("Gone", &[network("Home", "pass1234"), network("Cafe", "")]);
    let saved = read(json.as_bytes(), &KEY).unwrap();
    assert_eq!(saved.first_choice().unwrap().ssid().as_str(), Some("Home"));
    let empty = read(file("", &[]).as_bytes(), &KEY).unwrap();
    assert!(empty.is_empty() && empty.first_choice().is_none());
}

#[test]
fn a_password_of_another_length_or_checksum_is_left_out() {
    let wrong_len =
        network("Len", "secret12").replace(r#""password_len":8"#, r#""password_len":9"#);
    let wrong_crc = format!(
        r#"{{"ssid":"Crc","password_obf":"{}","password_len":8,"password_crc32":1}}"#,
        obfuscated("secret12")
    );
    let as_string =
        network("Str", "secret12").replace(r#""password_len":8"#, r#""password_len":"8""#);
    let json = file(
        "",
        &[wrong_len, wrong_crc, as_string, network("Good", "secret12")],
    );
    let saved = read(json.as_bytes(), &KEY).unwrap();
    assert_eq!(names(&saved), ["Good"]);
}

#[test]
fn another_devices_key_gives_passwords_whose_checksum_fails() {
    let json = file("", &[network("Home", "secret12")]);
    let other = [0u8; KEY_LEN];
    assert!(read(json.as_bytes(), &other).unwrap().is_empty());
}

#[test]
fn an_older_file_with_a_plain_password_or_no_checks_is_read() {
    let plain = r#"{"ssid":"Old","password":"plain pass"}"#;
    let unchecked = format!(
        r#"{{"ssid":"Mid","password_obf":"{}"}}"#,
        obfuscated("mid pass")
    );
    let not_base64 = r#"{"ssid":"Bad","password_obf":"!!!!","password":"fallback"}"#;
    let open = r#"{"ssid":"Open","password_obf":"","password_len":0,"password_crc32":0}"#;
    let json = file(
        "",
        &[plain.into(), unchecked, not_base64.into(), open.into()],
    );
    let saved = read(json.as_bytes(), &KEY).unwrap();
    let passwords: Vec<&str> = saved
        .networks()
        .map(|n| n.password().as_str().unwrap())
        .collect();
    assert_eq!(passwords, ["plain pass", "mid pass", "fallback", ""]);
}

#[test]
fn a_password_longer_than_the_firmware_keeps_is_left_out() {
    let long = "x".repeat(65);
    let json = file(
        "",
        &[network("Long", &long), network("Max", &"y".repeat(64))],
    );
    let saved = read(json.as_bytes(), &KEY).unwrap();
    assert_eq!(names(&saved), ["Max"]);
    let legacy = format!(r#"{{"ssid":"Legacy","password":"{long}"}}"#);
    assert!(
        read(file("", &[legacy]).as_bytes(), &KEY)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn at_most_eight_networks_are_kept_and_the_left_out_do_not_count() {
    let mut networks = std::vec![network("Broken", "x").replace("password_crc32", "password_len")];
    networks.extend((0..10).map(|i| network(&format!("Net{i}"), "password")));
    let saved = read(file("Net9", &networks).as_bytes(), &KEY).unwrap();
    assert_eq!(saved.len(), NETWORKS_MOST);
    assert_eq!(names(&saved)[0], "Net0");
    // The last joined was not kept: the first is the choice.
    assert_eq!(saved.first_choice().unwrap().ssid().as_str(), Some("Net0"));
}

#[test]
fn a_broken_file_is_an_error_and_other_fields_are_skipped() {
    assert_eq!(
        read(br#"{"credentials":[tru]}"#, &KEY),
        Err(ReadError::Json)
    );
    let large = format!(r#"{{"credentials":[],"x":"{}"}}"#, "a".repeat(JSON_MOST));
    assert_eq!(read(large.as_bytes(), &KEY), Err(ReadError::TooLarge));
    let extra = network("Home", "secret12").replace('{', r#"{"note":{"a":[1,2]},"#);
    let json =
        format!(r#"{{"version":3,"credentials":[{extra}],"lastConnectedSsid":"Home","x":null}}"#);
    let saved = read(json.as_bytes(), &KEY).unwrap();
    assert_eq!(
        saved.first_choice().unwrap().password().as_str(),
        Some("secret12")
    );
}

#[test]
fn debug_shows_neither_a_password_nor_a_name() {
    let json = file("Home", &[network("HomeNet", "hunter22")]);
    let shown = format!("{:?}", read(json.as_bytes(), &KEY).unwrap());
    assert!(!shown.contains("hunter22") && !shown.contains("HomeNet"));
    assert!(shown.contains("has_password: true"));
}

#[test]
fn base64_decodes_rfc_4648s_vectors_and_refuses_the_rest() {
    let vectors = [
        ("", ""),
        ("Zg==", "f"),
        ("Zm8=", "fo"),
        ("Zm9v", "foo"),
        ("Zm9vYg==", "foob"),
        ("Zm9vYmE=", "fooba"),
        ("Zm9vYmFy", "foobar"),
    ];
    for (encoded, decoded) in vectors {
        let text = decode::<8>(encoded.as_bytes()).unwrap();
        assert_eq!(text.as_bytes(), decoded.as_bytes(), "{encoded}");
    }
    // mbedtls 3.6 skips line breaks anywhere, and spaces before one or at the end.
    let spaced = [
        ("Zm9v\n", "foo"),
        ("Zm\r\n9v", "foo"),
        ("Zm9v  ", "foo"),
        ("Zm  \n9v", "foo"),
        ("Zg=\n=", "f"),
        ("  ", ""),
        ("\n", ""),
    ];
    for (encoded, decoded) in spaced {
        let text = decode::<8>(encoded.as_bytes()).unwrap();
        assert_eq!(text.as_bytes(), decoded.as_bytes(), "{encoded:?}");
    }
    for bad in [
        "Zg=",
        "Z===",
        "Zg=a",
        "Z=g=",
        "Zm 9v",
        " Zm9v",
        "Zm9v\r",
        "Zm\r9v",
        "Z\u{80}9v",
        "Zm9vY",
    ] {
        assert_eq!(
            decode::<8>(bad.as_bytes()),
            Err(DecodeError::Invalid),
            "{bad:?}"
        );
    }
    assert_eq!(decode::<5>(b"Zm9vYmFy"), Err(DecodeError::TooLong));
}

#[test]
fn a_file_cut_short_is_broken_and_text_after_it_is_ignored_as_for_the_cpp_firmware() {
    let whole = file(
        "Home",
        &[network("Home", "secret12"), network("Cafe", "latte123")],
    );
    let cut = &whole[..whole.len() - 20];
    assert_eq!(read(cut.as_bytes(), &KEY), Err(ReadError::Json));
    // ArduinoJson stops at the end of an object root: what follows it is not read.
    let more = format!("{whole} {{}}");
    assert_eq!(read(more.as_bytes(), &KEY).map(|s| s.len()), Ok(2));
    assert!(read(format!("{whole}\n").as_bytes(), &KEY).is_ok());
}

#[test]
fn a_later_key_wins_and_a_null_takes_the_earlier_value_away() {
    // A null `password_len` leaves no length to check, as `ArduinoJson`'s `isNull()` sees it.
    let nulled = network("Home", "secret12").replace(
        r#""password_len":8"#,
        r#""password_len":9,"password_len":null"#,
    );
    let first = file("Home", &[network("Old", "oldpass1")]);
    let twice = first.replace(
        "]}",
        &format!(r#"],"credentials":[{nulled}],"lastConnectedSsid":null}}"#),
    );
    let saved = read(twice.as_bytes(), &KEY).unwrap();
    assert_eq!(
        names(&saved),
        ["Home"],
        "the last credentials array is the one kept"
    );
    assert_eq!(
        saved.first_choice().unwrap().password().as_str(),
        Some("secret12")
    );
}

#[test]
fn strings_end_at_a_nul_and_a_name_too_long_for_wi_fi_is_left_out() {
    let nul = network("Home", "secret12").replace(r#""ssid":"Home""#, r#""ssid":"Home\u0000x""#);
    let long = network(&"n".repeat(3000), "secret12");
    let zero = network("Zero", "").replace(r#""password_len":0"#, r#""password_len":-0"#);
    let saved = read(file("Home", &[nul, long, zero]).as_bytes(), &KEY).unwrap();
    assert_eq!(names(&saved), ["Home", "Zero"]);
    assert_eq!(saved.first_choice().unwrap().ssid().as_str(), Some("Home"));
}

#[test]
fn a_long_text_that_is_not_base64_falls_back_to_the_plain_password() {
    let not_base64 = format!("!!!!{}", "A".repeat(84));
    let entry = format!(r#"{{"ssid":"Old","password_obf":"{not_base64}","password":"plain"}}"#);
    let saved = read(file("", &[entry]).as_bytes(), &KEY).unwrap();
    assert_eq!(
        saved.first_choice().unwrap().password().as_str(),
        Some("plain")
    );
    assert_eq!(
        decode::<4>(not_base64.as_bytes()),
        Err(DecodeError::Invalid)
    );
}
