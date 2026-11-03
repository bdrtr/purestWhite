//! The `KOReader` sync account the C++ firmware saved, checked against its server (gate 2's sync
//! step): the account read from the card's bytes (`purestwhite-kosync`, its secrets decoded with
//! the device's key), and the account check `KOReaderSyncClient::authenticate` sends, over HTTPS
//! (`https::exchange`, its status only), its MD5 from the ROM (`esp_hal::rom::md5`). The report
//! holds the status and the times, never the account's name, password or server.

use super::https::{self, Exchanged, FetchError, Until};
use super::saved::device_key;
use crate::boot::Report;
use crate::card::FileError;
use alloc::vec::Vec;
use core::fmt::{self, Write as _};
use embassy_net::Stack;
use purestwhite_http::url::{UrlError, split};
use purestwhite_kosync::auth::{Answer, RequestError, answer, auth_request};
use purestwhite_kosync::{Account, ReadError, read};

/// An account that can be checked, and its server's base URL.
pub struct Checkable {
    pub account: Account,
    url: Vec<u8>,
}

/// The account in `file` (its bytes, or why there are none) if it can be checked: `account: https`
/// in `report`, or why not (`no file`, `broken file`, `no credentials`, `http, not checked`...).
pub fn note_account(report: &mut Report, file: Result<&[u8], FileError>) -> Option<Checkable> {
    let checkable = file.map_err(FileError::note).and_then(checkable);
    let _ = match &checkable {
        Ok(_) => writeln!(report, "account: https"),
        Err(why) => writeln!(report, "account: {why}"),
    };
    report.save();
    checkable.ok()
}

/// The account in `bytes`, if it can be checked, or why not.
fn checkable(bytes: &[u8]) -> Result<Checkable, &'static str> {
    let account = read(bytes, &device_key()).map_err(|why| match why {
        ReadError::Json => "broken file",
        ReadError::TooLarge => "file too large",
        ReadError::Memory => "no memory",
    })?;
    if !account.has_credentials() {
        return Err("no credentials");
    }
    let url = account.base_url().map_err(|_| "no memory")?;
    match split(&url) {
        Ok(server) if server.tls => {}
        Ok(_) => return Err("http, not checked"),
        Err(UrlError::Scheme) => return Err("bad url scheme"),
        Err(UrlError::Host) => return Err("bad url host"),
    }
    Ok(Checkable { account, url })
}

/// What signing in came to.
pub enum Checked {
    Answered(Exchanged),
    Request(RequestError),
    Fetch(FetchError),
}

/// `kosync: 200 ok, ms 40/80/900/120` (the status, what it means, and the time DNS, TCP, TLS and
/// the answer took), or why there was no answer.
impl fmt::Display for Checked {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Checked::Answered(x) => {
                let meaning = match answer(x.status) {
                    Answer::Ok => "ok",
                    Answer::AuthFailed => "refused",
                    Answer::ServerError => "error",
                };
                write!(
                    out,
                    "kosync: {} {meaning}, ms {}/{}/{}/{}",
                    x.status, x.dns_ms, x.tcp_ms, x.tls_ms, x.answer_ms
                )
            }
            Checked::Request(why) => write!(out, "kosync: request ({why:?})"),
            Checked::Fetch(why) => write!(out, "kosync: {why}"),
        }
    }
}

/// Signs the account in at its server over `stack`.
pub async fn check(stack: Stack<'_>, checkable: &Checkable) -> Checked {
    // `checkable` split the URL already.
    let Ok(server) = split(&checkable.url) else {
        return Checked::Request(RequestError::Unsafe);
    };
    let md5 = |data: &[u8]| esp_hal::rom::md5::compute(data).into();
    let text = match auth_request(&server, &checkable.account, md5) {
        Ok(text) => text,
        Err(why) => return Checked::Request(why),
    };
    match https::exchange(
        stack,
        (server.host, server.port),
        &text,
        Until::Status,
        |_| {},
    )
    .await
    {
        Ok(x) => Checked::Answered(x),
        Err(why) => Checked::Fetch(why),
    }
}
