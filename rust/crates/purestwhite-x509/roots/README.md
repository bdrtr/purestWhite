# The roots

`x509_crt_bundle` is ESP-IDF 5.5.5's certificate bundle, as the C++ firmware links it
(`CONFIG_MBEDTLS_CERTIFICATE_BUNDLE_DEFAULT_FULL`, at most
`CONFIG_MBEDTLS_CERTIFICATE_BUNDLE_MAX_CERTS` = 200 roots): the 68,983 bytes between
`_binary_x509_crt_bundle_start` and `_binary_x509_crt_bundle_end` in `pio run -e x4pro`'s
`firmware.elf`. ESP-IDF's `gen_crt_bundle.py` builds it from `cacrt_all.pem` and `cacrt_local.pem`
(`components/mbedtls/esp_crt_bundle/`; the local file is empty in 5.5.5). A copy of
`cacrt_all.pem`, 150 certificates, is in `../testdata`.

The crate's tests rebuild the bundle from that copy and compare it byte for byte, so a new bundle
comes with the PEM it was built from. The C++ firmware's bundle comes with the pioarduino platform
`platformio.ini` pins (its `platform` line): when that line changes, copy both files again from
the ESP-IDF release it brings, and check the C++ firmware's image holds the same bundle.

The roots come from Mozilla's root store (NSS `certdata.txt`, extracted by curl on 2025-02-25: see
the PEM's header) and are under the Mozilla Public License 2.0 (https://mozilla.org/MPL/2.0/).
`x509_crt_bundle` holds only their names and keys, made by ESP-IDF's `gen_crt_bundle.py`
(Apache-2.0); no ESP-IDF code is copied.
