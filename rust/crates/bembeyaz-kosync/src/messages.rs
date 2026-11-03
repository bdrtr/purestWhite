//! The JSON the `KOReader` sync client sends and reads (`lib/KOReaderSync/KOReaderSyncMessages`),
//! written and read through `bembeyaz-jsondoc` as the C++ wrote and read it with ArduinoJson: the
//! same members in the same order, the same numbers, the same conversions of a field of an
//! unexpected type.

use alloc::vec::Vec;
pub use bembeyaz_jsondoc::Error;
use bembeyaz_jsondoc::{Value, as_number, as_str, as_string, is_i64, is_number, parse};

/// The device name and id sent with every progress update.
pub const DEVICE_NAME: &[u8] = b"CrossPoint";
pub const DEVICE_ID: &[u8] = b"crosspoint-reader";
/// The longest `XPath` a rich position carries (the server refuses the whole position past it).
pub const XPATH_MOST: usize = 120;

/// A book's metadata sent with a progress update.
#[derive(Clone, Copy, Debug, Default)]
pub struct Metadata<'a> {
    pub filename: &'a [u8],
    pub title: &'a [u8],
    pub authors: &'a [u8],
    /// A plugin's sidecar JSON: its flat members go in too, the fields above winning.
    pub extra_json: &'a [u8],
    pub isbn: &'a [u8],
    pub asin: &'a [u8],
    pub series: &'a [u8],
    pub series_index: Option<f32>,
}

/// A CrossPoint position (the crosspoint-sync servers' `position`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position<'a> {
    pub pct_q: u32,
    pub spine: u16,
    pub page: u16,
    pub pages: u16,
    pub para: Option<u16>,
    pub xpath: &'a [u8],
}

/// A progress update to send.
#[derive(Clone, Copy, Debug, Default)]
pub struct Update<'a> {
    pub document: &'a [u8],
    pub progress: &'a [u8],
    pub percentage: f32,
    pub metadata: Option<Metadata<'a>>,
    pub position: Option<Position<'a>>,
}

/// The progress a server gave.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Received {
    pub progress: Vec<u8>,
    pub percentage: f32,
    pub device: Vec<u8>,
    pub device_id: Vec<u8>,
    pub timestamp: i64,
    pub position: Option<ReceivedPosition>,
}

/// A rich position a server gave (its pages at least 1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReceivedPosition {
    pub pct_q: u32,
    pub spine: u16,
    pub page: u16,
    pub pages: u16,
    pub para: Option<u16>,
    pub xpath: Vec<u8>,
}

/// POST /users/create's body: the username and the MD5 auth key.
#[must_use]
pub fn create_user_body(username: &[u8], md5_password: &[u8]) -> Vec<u8> {
    let mut doc = Value::Object(Vec::new());
    doc.set(b"username", Value::string(username));
    doc.set(b"password", Value::string(md5_password));
    doc.to_json()
}

/// PUT /syncs/progress's body. `extended_metadata` adds the bibliographic fields (custom
/// servers); `rich_progress` the CrossPoint position (any server but strict `KOSync`).
#[must_use]
pub fn progress_body(update: &Update<'_>, extended_metadata: bool, rich_progress: bool) -> Vec<u8> {
    let mut doc = Value::Object(Vec::new());
    doc.set(b"document", Value::string(update.document));
    if let Some(metadata) = &update.metadata {
        doc.set(b"metadata", metadata_of(metadata, extended_metadata));
    }
    doc.set(b"progress", Value::string(update.progress));
    doc.set(b"percentage", Value::F32(update.percentage));
    doc.set(b"device", Value::string(DEVICE_NAME));
    doc.set(b"device_id", Value::string(DEVICE_ID));
    if let (Some(position), true) = (&update.position, rich_progress) {
        doc.set(b"position", position_of(position));
    }
    doc.to_json()
}

fn metadata_of(metadata: &Metadata<'_>, extended: bool) -> Value {
    let mut meta = Value::Object(Vec::new());
    meta.set(b"filename", Value::string(metadata.filename));
    meta.set(b"title", Value::string(metadata.title));
    meta.set(b"authors", Value::string(metadata.authors));
    if extended {
        for (key, text) in [
            (&b"isbn"[..], metadata.isbn),
            (b"asin", metadata.asin),
            (b"series", metadata.series),
        ] {
            if !text.is_empty() {
                meta.set(key, Value::string(text));
            }
        }
        if let Some(index) = metadata.series_index {
            meta.set(b"series_index", Value::F32(index));
        }
    }
    if !metadata.extra_json.is_empty() {
        add_extra(&mut meta, metadata.extra_json);
    }
    meta
}

/// The sidecar's flat members (strings, numbers and booleans, their JSON types kept) where the
/// metadata has none of that name yet. Its keys are taken as C strings (up to a NUL).
fn add_extra(meta: &mut Value, extra_json: &[u8]) {
    let Ok(Value::Object(members)) = parse(extra_json) else {
        return;
    };
    for (key, value) in members {
        let value = Some(&value);
        let flat = as_str(value).is_some()
            || matches!(value, Some(Value::Bool(_)))
            || is_i64(value)
            || is_number(value);
        let key = key.split(|b| *b == 0).next().unwrap_or_default();
        if !flat || !Value::is_null(meta.get(key)) {
            continue;
        }
        if let Some(value) = value {
            meta.set(key, value.clone());
        }
    }
}

fn position_of(p: &Position<'_>) -> Value {
    let mut pos = Value::Object(Vec::new());
    pos.set(b"pctQ", Value::unsigned(u64::from(p.pct_q)));
    pos.set(b"spine", Value::unsigned(u64::from(p.spine)));
    pos.set(b"page", Value::unsigned(u64::from(p.page)));
    pos.set(b"pages", Value::unsigned(u64::from(p.pages)));
    if let Some(para) = p.para {
        pos.set(b"para", Value::unsigned(u64::from(para)));
    }
    if !p.xpath.is_empty() && p.xpath.len() <= XPATH_MOST {
        pos.set(b"xpath", Value::string(p.xpath));
    }
    pos
}

/// GET /syncs/progress's answer (a C string: it ends at the first NUL), each field read as the C++
/// read it (`as<std::string>()` gives a non-string field's JSON, `null` for a missing one), the
/// rich position only when `rich_progress`.
///
/// # Errors
/// The answer is not JSON (ArduinoJson's reason).
pub fn parse_progress(body: &[u8], rich_progress: bool) -> Result<Received, Error> {
    let doc = parse(body)?;
    let position = match doc.get(b"position") {
        Some(Value::Object(_)) if rich_progress => doc.get(b"position").map(received_position),
        _ => None,
    };
    Ok(Received {
        progress: as_string(doc.get(b"progress")),
        percentage: as_number(doc.get(b"percentage")),
        device: as_string(doc.get(b"device")),
        device_id: as_string(doc.get(b"device_id")),
        timestamp: as_number(doc.get(b"timestamp")),
        position,
    })
}

fn received_position(pos: &Value) -> ReceivedPosition {
    let pages: u16 = as_number(pos.get(b"pages"));
    let para: u16 = as_number(pos.get(b"para"));
    ReceivedPosition {
        pct_q: as_number(pos.get(b"pctQ")),
        spine: as_number(pos.get(b"spine")),
        page: as_number(pos.get(b"page")),
        pages: pages.max(1),
        para: (para > 0).then_some(para),
        // Taken as a C string (`as<const char*>()` into a std::string): up to a NUL.
        xpath: as_str(pos.get(b"xpath"))
            .and_then(|x| x.split(|b| *b == 0).next())
            .unwrap_or_default()
            .to_vec(),
    }
}
