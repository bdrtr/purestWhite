// The tests index what must be there and fail on what is not: a failure fails the test.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use super::auth::{Answer, RequestError, answer, auth_request, request};
use super::{JSON_MOST, ReadError, read};
use purestwhite_http::request::Header;
use purestwhite_http::url::split;
use purestwhite_obfuscation::{KEY_LEN, base64};
use std::cell::RefCell;
use std::format;
use std::string::String;
use std::vec::Vec;

/// A made-up device key: no real device's MAC address.
const KEY: [u8; KEY_LEN] = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];

/// `secret` as the C++ firmware stores it: masked with the key, then base64.
fn obfuscated(secret: &[u8]) -> String {
    let masked: Vec<u8> = secret
        .iter()
        .zip(KEY.iter().cycle())
        .map(|(b, k)| b ^ k)
        .collect();
    String::from_utf8(base64::encode(&masked).unwrap().to_vec()).unwrap()
}

/// A v3 `koreader.json` for `user`, `password`, `url` and the headers' JSON.
fn account_json(user: &str, password: &[u8], url: &str, headers: &str) -> String {
    format!(
        r#"{{"cfgVersion":3,"username":"{user}","password_obf":"{}","serverUrl":"{url}","serverType":1,"syncBehavior":1,"headers":[{headers}]}}"#,
        obfuscated(password)
    )
}

/// A header's JSON, its value stored as the C++ firmware stores it.
fn header_json(name: &str, value: &[u8]) -> String {
    format!(r#"{{"name":"{name}","value_obf":"{}"}}"#, obfuscated(value))
}

/// The account check's text for `json`, and what was handed to the MD5.
fn auth_text(json: &str) -> (Result<String, RequestError>, Vec<u8>) {
    let account = read(json.as_bytes(), &KEY).unwrap();
    let url = account.base_url().unwrap();
    let server = split(&url).unwrap();
    let hashed = RefCell::new(Vec::new());
    let text = auth_request(&server, &account, |p| {
        hashed.borrow_mut().extend_from_slice(p);
        [0xAB; 16]
    });
    let text = text.map(|t| String::from_utf8_lossy(&t).into_owned());
    (text, hashed.into_inner())
}

#[test]
fn the_account_check_signs_in_as_the_cpp_client_does() {
    let headers = format!(
        "{},{}",
        header_json("X-Token", b""),
        header_json("", b"unused")
    );
    let json = account_json("okur", b"parola", "http://192.0.2.7:8081/sync//", &headers);
    let (text, hashed) = auth_text(&json);
    assert_eq!(hashed, b"parola");
    let want = format!(
        "GET /sync/users/auth HTTP/1.1\r\nHost: 192.0.2.7:8081\r\nUser-Agent: FreeInk-ESP32\r\n\
         Connection: close\r\nX-Token: \r\nAccept: application/vnd.koreader.v1+json\r\n\
         x-auth-user: okur\r\nx-auth-key: {}\r\nAuthorization: Basic b2t1cjpwYXJvbGE=\r\n\r\n",
        "ab".repeat(16)
    );
    assert_eq!(text.unwrap(), want);
}

#[test]
fn the_default_server_is_https_on_its_own_port() {
    let account = read(account_json("u", b"p", "", "").as_bytes(), &KEY).unwrap();
    let url = account.base_url().unwrap();
    let server = split(&url).unwrap();
    assert_eq!(
        (server.tls, server.host, server.port),
        (true, "sync.crosspointreader.com", 443)
    );
    let (text, _) = auth_text(&account_json("u", b"p", "", ""));
    assert!(
        text.unwrap()
            .starts_with("GET /users/auth HTTP/1.1\r\nHost: sync.crosspointreader.com\r\n")
    );
}

#[test]
fn a_password_is_hashed_and_sent_up_to_a_nul_as_cpp_hands_it_on() {
    let (text, hashed) = auth_text(&account_json("u", b"ab\0cd", "", ""));
    assert_eq!(hashed, b"ab");
    let basic = String::from_utf8(base64::encode(b"u:ab").unwrap().to_vec()).unwrap();
    assert!(text.unwrap().contains(&format!("Basic {basic}\r\n")));
}

#[test]
fn no_account_or_a_byte_that_would_break_it_writes_no_request() {
    let no_user = auth_text(&account_json("", b"p", "", "")).0;
    let no_password = auth_text(&account_json("u", b"", "", "")).0;
    assert_eq!(no_user.err(), Some(RequestError::NoCredentials));
    assert_eq!(no_password.err(), Some(RequestError::NoCredentials));
    let unsafe_cases = [
        account_json("u", b"p", "", &header_json("X", b"a\r\nEvil: 1")),
        account_json("u", b"p", "", &header_json("X Y", b"1")),
        account_json("u\\u0001", b"p", "", ""),
        account_json("u", b"p", "https://h/a b", ""),
    ];
    for json in unsafe_cases {
        assert_eq!(
            auth_text(&json).0.err(),
            Some(RequestError::Unsafe),
            "{json}"
        );
    }
}

#[test]
fn a_request_with_a_body_carries_its_length_after_the_sign_in() {
    let account = read(account_json("u", b"p", "", "").as_bytes(), &KEY).unwrap();
    let server = split(b"https://h").unwrap();
    let extra = [Header {
        name: b"Content-Type",
        value: b"application/json",
    }];
    let text = request(
        (&server, &account),
        ("PUT", b"/syncs/progress"),
        &extra,
        Some(b"{}"),
        |_| [0; 16],
    )
    .unwrap();
    let text = String::from_utf8(text.to_vec()).unwrap();
    assert!(text.starts_with("PUT /syncs/progress HTTP/1.1\r\n"));
    assert!(text.ends_with("Content-Type: application/json\r\nContent-Length: 2\r\n\r\n{}"));
}

#[test]
fn a_status_means_what_authenticate_makes_of_it() {
    let cases = [
        (199, Answer::ServerError),
        (200, Answer::Ok),
        (204, Answer::Ok),
        (299, Answer::Ok),
        (300, Answer::ServerError),
        (401, Answer::AuthFailed),
        (403, Answer::ServerError),
        (500, Answer::ServerError),
    ];
    for (status, want) in cases {
        assert_eq!(answer(status), want, "{status}");
    }
}

#[test]
fn a_broken_or_huge_file_is_an_error_and_one_at_the_limit_is_read() {
    assert_eq!(read(b"{\"username\":", &KEY).err(), Some(ReadError::Json));
    let fill = |n: usize| format!(r#"{{"x":"{}"}}"#, "a".repeat(n));
    let at_limit = fill(JSON_MOST - fill(0).len());
    assert_eq!(at_limit.len(), JSON_MOST);
    assert!(read(at_limit.as_bytes(), &KEY).is_ok());
    let over = fill(JSON_MOST);
    assert_eq!(read(over.as_bytes(), &KEY).err(), Some(ReadError::TooLarge));
}

#[test]
fn debug_shows_neither_a_name_nor_a_password() {
    let account = read(
        account_json("okur", b"parola", "https://h", "").as_bytes(),
        &KEY,
    )
    .unwrap();
    let shown = format!("{account:?}");
    assert!(!shown.contains("okur") && !shown.contains("parola") && !shown.contains("h\""));
}
