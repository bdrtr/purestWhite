//! The request as written, and responses read whole or piece by piece, at every split.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use super::{HEAD_MOST, Response, ResponseError, write_get};
use std::string::String;
use std::vec::Vec;

const RESPONSE: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 11\r\n\r\nhello world";

/// The body `bytes` give, fed in pieces of `piece` bytes.
fn read(bytes: &[u8], piece: usize) -> (Result<(), ResponseError>, Response, Vec<u8>) {
    let mut response = Response::new();
    let mut body = Vec::new();
    for chunk in bytes.chunks(piece) {
        if let Err(e) = response.feed(chunk, |b| body.extend_from_slice(b)) {
            return (Err(e), response, body);
        }
    }
    (Ok(()), response, body)
}

#[test]
fn a_get_request_is_http_1_0_with_the_host() {
    let mut out = String::new();
    write_get(&mut out, "raw.githubusercontent.com", "/a/b.md").unwrap();
    assert_eq!(
        out,
        "GET /a/b.md HTTP/1.0\r\nHost: raw.githubusercontent.com\r\nUser-Agent: purestWhite\r\nAccept: */*\r\n\r\n"
    );
    for (host, path) in [("a b", "/"), ("a", "/x\r\n"), ("a", "x"), ("", "/")] {
        assert!(
            write_get(&mut String::new(), host, path).is_err(),
            "{host:?} {path:?}"
        );
    }
}

#[test]
fn the_status_and_the_body_come_out_however_the_bytes_are_split() {
    for piece in 1..=RESPONSE.len() {
        let (result, response, body) = read(RESPONSE, piece);
        assert_eq!(result, Ok(()), "piece {piece}");
        assert_eq!(response.status(), Some(200));
        assert_eq!(body, b"hello world", "piece {piece}");
        assert_eq!(response.body_len(), 11);
    }
}

#[test]
fn a_status_line_that_is_not_http_is_refused() {
    for bad in [
        &b"HTTP/2 200 OK\r\n\r\n"[..],
        b"ICY 200 OK\r\n\r\n",
        b"HTTP/1.1 20 OK\r\n\r\n",
        b"HTTP/1.1 +20 OK\r\n\r\n",
    ] {
        assert_eq!(read(bad, 64).0, Err(ResponseError::StatusLine));
    }
    let (_, response, _) = read(b"HTTP/1.0 404 Not Found\r\n\r\n", 7);
    assert_eq!(response.status(), Some(404));
}

#[test]
fn a_head_longer_than_its_room_is_refused() {
    let mut long = b"HTTP/1.1 200 OK\r\nX: ".to_vec();
    long.resize(HEAD_MOST + 10, b'a');
    long.extend_from_slice(b"\r\n\r\nbody");
    assert_eq!(read(&long, 100).0, Err(ResponseError::HeadTooLong));
    assert_eq!(read(&long, long.len()).0, Err(ResponseError::HeadTooLong));
}

#[test]
fn a_body_that_comes_with_the_head_in_one_piece_past_the_heads_room_is_whole() {
    let mut big = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
    big.extend(std::iter::repeat_n(b'z', 3 * HEAD_MOST));
    let (result, response, body) = read(&big, big.len());
    assert_eq!(result, Ok(()));
    assert_eq!(body.len(), 3 * HEAD_MOST);
    assert_eq!(response.body_len(), 3 * HEAD_MOST);
}

#[test]
fn a_status_line_without_a_reason_is_read() {
    for head in [
        &b"HTTP/1.1 200\r\n\r\nx"[..],
        b"HTTP/1.0 204\r\nA: b\r\n\r\n",
    ] {
        let (result, response, _) = read(head, 3);
        assert_eq!(result, Ok(()));
        assert!(response.status().is_some());
    }
}

#[test]
fn a_head_of_exactly_its_room_is_read_and_one_byte_more_is_not() {
    let head = |len: usize| {
        let mut head = b"HTTP/1.1 200 OK\r\nX: ".to_vec();
        head.resize(len - 4, b'a');
        head.extend_from_slice(b"\r\n\r\nbody");
        head
    };
    let (result, _, body) = read(&head(HEAD_MOST), 512);
    assert_eq!(result, Ok(()));
    assert_eq!(body, b"body");
    assert_eq!(
        read(&head(HEAD_MOST + 1), 512).0,
        Err(ResponseError::HeadTooLong)
    );
}

#[test]
fn the_content_length_tells_when_the_body_is_whole() {
    let (_, response, _) = read(RESPONSE, 5);
    assert_eq!(response.content_length(), Some(11));
    assert!(response.is_complete());
    let cut = &RESPONSE[..RESPONSE.len() - 3];
    let (_, response, _) = read(cut, 5);
    assert!(!response.is_complete(), "8 of 11 bytes");
    let lower = b"HTTP/1.1 200 OK\r\ncontent-LENGTH:  4 \r\n\r\nabcd";
    assert_eq!(read(lower, 3).1.content_length(), Some(4));
    for odd in [
        &b"HTTP/1.1 200 OK\r\n\r\nabc"[..],
        b"HTTP/1.1 200 OK\r\nContent-Length: x1\r\n\r\n",
    ] {
        let (_, response, _) = read(odd, 64);
        assert_eq!(response.content_length(), None);
        assert!(
            !response.is_complete(),
            "without a length, only the close ends the body"
        );
    }
}
