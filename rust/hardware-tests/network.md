# Hardware tests: network

These tests cover BLE remotes, the HTTPS server certificate checks, time sync, TLS session
resumption, the KOSync connection, replies without a body, and the update over Wi-Fi.

## BLE remotes (598e41e, 76620d4)

- [ ] Pair four remotes; all four must be able to connect after a restart (before, the first
  remote's key was lost).
- [ ] With four remotes paired, scan for a fifth: in the list it must be inactive and show
  "Forget a paired remote first" (in English if the UI language has no translation for it).
  Forget one: the fifth must pair, and the other three must keep working.

## Server certificate checks ("net: SecureNet moves into the fork", "net: HTTPS checks server certificates")

HTTPS now checks the server: the chain must reach a root in ESP-IDF's root bundle (150 roots),
the certificate must carry the server's name, and its dates must be valid. Setting: Settings >
System > Verify Server Certificates (Turkish UI: "Sunucu sertifikalarını doğrula"), on by default.
The CrossPoint (crosspointreader.com), FreeInk (freeink.org) and GitHub addresses are always
checked, whatever the setting. The SDK's OPDS OAuth sign-in also follows the setting. SHA-384/512
and a fast P-384 were added to wolfSSL; chains signed with P-384 (Let's Encrypt, Google, Sectigo)
could not be checked without them.

Extra time per new connection (computed for 240 MHz from the instruction count in the emulator; it
may be a little longer on the device): Let's Encrypt ECDSA chain (sync.crosspointreader.com)
≈1.6 s, SSL.com chain (e.g. example.com) ≈1.2 s, github.com ≈1.0 s, Google Trust Services chain
≈0.7 s. KOSync opens one connection per sync (see [KOSync: one connection per sync][kosync]);
with the default server, a sync takes ≈1.6 s longer, and once more for the upload if you wait
longer than ≈1 minute on the result screen. On servers that send a session ticket and resume
sessions, the chain is not checked again from the second connection after boot on (see
[TLS session resumption][resumption]). sync.crosspointreader.com and github.com do not resume
sessions (also tried with the OpenSSL client; it is on the server side); on these servers the
extra time stays on every connection.

[kosync]: #kosync-one-connection-per-sync-net-one-kosync-connection-per-sync
[resumption]: #tls-session-resumption-net-secureclient-resumes-tls-sessions

In the log line `[SecureClient] certificate N refused: K`, N is the position in the chain (0 is
the server's own certificate) and K is the wolfSSL code:
- `-188` no signer found: an intermediate certificate is missing, or the root is not in the
  bundle. -188 also comes when the top certificate's dates are not valid or its signature does not
  match. On a signature error, `esp-x509-crt-bundle: Certificate matched but signature
  verification failed` shows above it; if the root is not in the bundle, only `Failed to verify
  certificate`. Check the clock first.
- `-150` / `-151` not yet valid / expired (if the clock is wrong, these also come on a valid
  server).
- `-322` the certificate does not carry the server's name; `-155` bad signature; `-232` the hash
  type is not compiled in.
- `certificate N not checked: low memory` means that memory ran out; it is not a certificate
  refusal. Memory errors are written as numbers: `wolfSSL_connect failed (...): -125` or
  `read failed: -125` (MEMORY_E); `-303` MEMORY_ERROR, `-2` MP_MEM.

- [x] CrossPoint Sync (default server sync.crosspointreader.com): send and get a position; it must
  work. Compare the times in the serial log's `[SecureClient] handshake ok (...) in N ms` lines,
  and the time of the whole sync, with the earlier version (the extra time above is expected).
- [x] OPDS: browse a public catalog (e.g. `https://m.gutenberg.org/ebooks.opds/`) and download a
  book.
- [ ] The Plugin Hub catalog and install, font download from Manage Fonts (GitHub), and
  downloading and opening an LCP-protected book must work.
- [ ] If you have an OPDS catalog with OAuth sign-in: with the setting on, sign-in must work. If
  the sign-in server's certificate is refused, it must show "Server certificate not trusted", not
  "Authentication failed".
- [ ] A plugin that connects to other servers over HTTPS (e.g. a book lending plugin): with the
  setting on, it must work on servers with a valid certificate.
- [ ] A self-signed HTTPS home server, or one reached by IP address (Calibre-Web, COPS, KOSync
  `https://<ip>:7200`): with the setting on, OPDS and CrossPoint Sync must show "Server
  certificate not trusted" (the same message with Sync Clippings on); with the setting off, they
  must work as before. Servers reached with `http://` are not affected. Then turn the setting on
  again.
- [ ] If badssl.com can be reached, add `https://expired.badssl.com/`,
  `https://wrong.host.badssl.com/`, `https://self-signed.badssl.com/` and
  `https://untrusted-root.badssl.com/` as OPDS servers: all must give "Server certificate not
  trusted". `https://ecc384.badssl.com/` must not give a certificate error (it gives a feed error,
  because the page is not OPDS).
- [ ] Clock: Settings > System > Clock must be right; certificate dates are checked against this
  clock. If the clock was never set, the first checked connection while Wi-Fi is connected waits
  at most 8 s for SNTP; if SNTP does not answer, the log shows `Clock not set`, and there is no
  more waiting on this boot.
- [ ] A KOSync server that requires TLS 1.3 (e.g. `https://kosync.ak-team.com:3042`) must
  connect.
- [ ] Memory: during a sync and an OPDS download, the log must not show the memory errors above or
  `not checked: low memory`, and the device must not restart.
- [ ] Cipher suite: the suite in the `handshake ok` line may now be `TLS_AES_256_GCM_SHA384`
  (SHA-384 was turned on); downloading a large book must not take clearly longer than in the
  earlier version.

## Time sync ("fix: trustedtime::syncNow reports the sync it waited for")

`syncNow()` counted a successful SNTP sync as failed (ESP-IDF resets the sync status when it is
read, and the function read it twice). Only one flow uses it: the one that runs when a time-limited
(borrowed) LCP book is opened and the clock cannot be verified.

- [ ] On a device whose clock is not verified (e.g. the clock was never set, or NVS was erased),
  open a time-limited LCP book and choose Wi-Fi: after "Syncing time...", the "Sync failed"
  (Turkish UI: "Eşitleme başarısız") window must not appear; the book must open again.

## TLS session resumption ("net: SecureClient resumes TLS sessions")

If the server sends a session ticket, SecureClient keeps the session: at most 4 sessions (one per
server, port and verify setting; the least recently used one is dropped), each ≈400 B of internal
RAM and a ticket of up to 512 bytes; it keeps them until a restart, deep sleep, or the ticket's
expiry. The next connection to the same server does not check the certificate chain again; the log
shows `handshake ok (auto): TLSv1.3 / <suite>, resumed in N ms`. Sessions with and without
verification are kept apart; connections with a pinned root certificate and the TLS 1.2 retry are
not resumed. In the emulator (real elapsed time; it may be different on the device), the second
connection: sync.koreader.rocks 1102 → 194 ms, kosync.ak-team.com 2452 → 788 ms, www.google.com
973 → 177 ms, a local server that speaks only TLS 1.2 3091 → 56 ms. sync.crosspointreader.com and
github.com did not resume. With all four slots full, the sessions hold ≈2.5 KB of internal RAM; the
largest free block did not change.

- [ ] After a restart, set the KOSync server to sync.koreader.rocks or
  `https://kosync.ak-team.com:3042` and sync twice: the first `handshake ok` line has no
  "resumed"; the later ones must have `, resumed` and a much shorter time; the sync must work.
- [ ] Default server sync.crosspointreader.com: "resumed" is not expected; the sync must work as
  before.
- [ ] Restart, turn off Verify Server Certificates, and sync with sync.koreader.rocks: the first
  connection must not have "resumed"; later ones may have it. Turn the setting on and sync again:
  if there was no connection to that server with the setting on during this boot, the first
  connection must not have "resumed". At the end, leave the setting on.
- [ ] After deep sleep or a restart, the first connection to the same server must not show
  "resumed" (full handshake).
- [ ] Memory: connect to more than 4 different HTTPS servers (sync, OPDS, Plugin Hub/GitHub, a
  plugin); the log must not show `-125`/`-303` or `not checked: low memory`, and the device must
  not restart.
- [ ] If you have a home server that speaks only TLS 1.2: the second connection may show
  `TLSv1.2 ..., resumed`; it must connect in every case.

## KOSync: one connection per sync ("net: one KOSync connection per sync")

The requests of one sync (the clipping sync if it is on, getting the position, sending the
position) go to the server over one connection. With Smart Sync (the default), the position is
sent right after it is received; with Ask Every Time, it is chosen on the result screen. In both
cases, on the default server, which does not resume sessions (sync.crosspointreader.com), the
upload does not make a separate full handshake (≈1.6–2.2 s). The server keeps an idle connection
open for ≈57 s (sync.koreader.rocks ≈17 s); if you wait longer, the connection is opened again (in
the emulator, with a server that closes the connection at 15 s, without errors). While the result
screen or the "No remote progress found" screen is open, the connection holds ≈7.5 KB of internal
RAM; it closes when the position is sent, when the server's position is applied, when the
positions are already equal, when the sync fails, or when you leave the sync screen (the device
restarts).

- [ ] With Smart Sync on and the device's position ahead of the server's, sync with the default
  server: the log must have a single `handshake ok` line, and the position must be sent.
- [ ] Set Settings > CrossPoint Sync > Sync Behavior to Ask Every Time; sync, and choose "Upload
  local progress" on the result screen within a few seconds: there must be no second
  `handshake ok` line between getting and sending the position, and the upload must finish fast.
- [ ] Do the same after waiting more than a minute on the result screen: a second `handshake ok`
  shows; the upload must still succeed.
- [ ] With a book that has no position on the server (Ask Every Time), choose "Upload" on the
  "No remote progress found" screen: there must be no second `handshake ok`; the upload must
  succeed, with no memory error.
- [ ] With Sync Clippings on, sync with the default server: from the clippings to getting the
  position, the log must have a single `handshake ok` line; the clippings, the result screen and
  the upload must work.
- [ ] On the result screen, the server's position must be shown correctly as chapter/page (the
  book is loaded again while the connection is open); the log must have no memory error
  (`-125`/`-303`), and the device must not restart. At the end, set Sync Behavior back to Smart
  Sync.

## Replies without a body ("fix: 204, 304 and HEAD replies end at their headers")

204 and 304 replies, and replies to HEAD requests, carry no body. When there was no length header,
the client waited for their body until the connection closed; on a server that keeps the
connection open, this meant a 15 s timeout. Spring-based KOSync servers (BookLore, grimmory)
return 204 for a book with no position. A host test checks that, on a connection kept open, all
three end without waiting and the connection is reused.

- [ ] If you have a Spring-based KOSync server (BookLore, grimmory), sync a book that has no
  position on the server: "No remote progress found" must show without waiting (at once, not after
  ≈15 s).
- [ ] A sync with the default server, OPDS and Plugin Hub must work as before.

## Update over Wi-Fi: our own release channel ("feat: update over Wi-Fi from bdrtr/purestWhite releases")

Settings > System > Check for Updates is now visible. It downloads the
`purestWhite-<version>-<board>.bin` file from the latest published release of
github.com/bdrtr/purestWhite. Releases are prepared as drafts with `scripts/release.sh`, and are
published by hand after the hardware tests. Like the SD update, the update writes to the other
slot: it replaces the firmware there (e.g. CrossPoint). In the emulator, with a fake release
server: the right board's file was chosen, the CDN redirect was followed, the 5.4 MB image was
written to the other slot byte for byte, and boot was switched to that slot. A tag like "v0.0.2"
is also understood; a tag that is not understood is never offered.

- [ ] While no release is published yet, Check for Updates: without an error, it must say "up to
  date" or "no update".
- [ ] After the first release is published, while an older version runs, Check for Updates: it
  must show the new version, download and install it, and the device must start with the new
  version (the version in About). The earlier firmware must stay in the other slot (Start Other
  Firmware must name it).
- [ ] While the same version runs, Check for Updates must not offer an update.
- [ ] Cut Wi-Fi during the update's download: the install must stop halfway, and the device must
  keep starting with the old version.
