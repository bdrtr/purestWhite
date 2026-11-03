//! ArduinoJson's own conversions replayed: every step of testdata/conversions-corpus.txt (written
//! by `test/json_conversions/make_corpus.py`) read as ArduinoJson read the member "v".
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::tests::{bytes, hex};
use crate::{
    as_number, is_i64, is_integer, is_number, items, members, or_bool, or_f32, or_integer, or_str,
    parse,
};
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/conversions-corpus.txt");

fn answer(input: &[u8]) -> String {
    let doc = match parse(input) {
        Ok(doc) => doc,
        Err(e) => return format!("err {}", e.name()),
    };
    let v = doc.get(b"v");
    let flag = |b: bool| u8::from(b);
    let is = format!(
        "is {} {} {} {} {} {} {} {}",
        flag(is_integer::<u8>(v)),
        flag(is_integer::<u16>(v)),
        flag(is_integer::<u32>(v)),
        flag(is_integer::<i32>(v)),
        flag(is_i64(v)),
        flag(matches!(v, Some(crate::Value::Bool(_)))),
        flag(is_number(v)),
        flag(matches!(v, Some(crate::Value::Str(_)))),
    );
    let or = format!(
        "or {} {} {} {} {:08x} {} {} {}",
        or_integer::<u8>(v, 7),
        or_integer::<u16>(v, 7),
        or_integer::<u32>(v, 7),
        or_integer::<i32>(v, -7),
        or_f32(v, 7.5).to_bits(),
        flag(or_bool(v, true)),
        flag(or_bool(v, false)),
        hex(or_str(v, b"d")),
    );
    let r#as = format!(
        "as {} {} {} {} {:08x}",
        as_number::<u8>(v),
        as_number::<u16>(v),
        as_number::<u32>(v),
        as_number::<i32>(v),
        as_number::<f32>(v).to_bits(),
    );
    let array = items(v);
    let shape = format!(
        "array {} {} {} object {}",
        array.len(),
        or_integer::<u16>(array.first(), 9),
        or_integer::<u16>(array.get(5), 9),
        members(v).len(),
    );
    format!("{is} {or} {as} {shape}", r#as = r#as)
}

#[test]
fn every_conversion_of_the_corpus_is_arduinojsons() {
    let mut lines = CORPUS.lines();
    let mut steps = 0;
    let mut failed = Vec::new();
    while let Some(line) = lines.next() {
        let want = lines.next().unwrap().strip_prefix("  = ").unwrap();
        let input = bytes(line.strip_prefix("v ").unwrap());
        steps += 1;
        let got = answer(&input);
        if got != want {
            failed.push(format!(
                "{}\n  want {want}\n  got  {got}",
                String::from_utf8_lossy(&input)
            ));
        }
    }
    assert!(steps > 3000, "{steps} steps");
    assert!(
        failed.is_empty(),
        "{} of {steps} differ:\n{}",
        failed.len(),
        failed[..failed.len().min(8)].join("\n")
    );
}
