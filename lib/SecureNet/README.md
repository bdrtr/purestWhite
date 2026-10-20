# SecureNet

The firmware's HTTPS client: `SecureClient` (wolfSSL TLS 1.3 over a
`WiFiClient`), the `SecureHttpClient` shim and `fetchResumable`.

This is a copy of `freeink-sdk/libs/network/SecureNet` at SDK revision
084fdfb, formatted with this repository's clang-format and owned by this fork
from then on. The commit that imported it is "net: SecureNet moves into the
fork"; `git log -p -- lib/SecureNet` from there shows what the fork changed.
To take a later SDK change, apply it here by hand.
