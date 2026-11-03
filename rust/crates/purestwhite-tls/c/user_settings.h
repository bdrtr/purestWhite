/* wolfSSL for the bare-metal firmware (purestwhite-tls): a TLS client with no operating system.
 * The choices follow the C++ firmware's (platformio.ini, Arduino-wolfSSL's user_settings.h) where
 * they apply: TLS 1.3 and 1.2, ECC with single-precision math for P-256 and P-384, SHA-384/512,
 * X25519, AES-GCM and ChaCha20-Poly1305, RSA for RSA certificates. What an operating system gave
 * comes from Rust (purestwhite-tls's hooks.rs): memory, the time, random bytes, and the bytes on
 * the wire, through memory buffers. */
#ifndef PURESTWHITE_TLS_USER_SETTINGS_H
#define PURESTWHITE_TLS_USER_SETTINGS_H

#include <stddef.h>
#include <time.h>

#define WOLFSSL_USER_SETTINGS_ID "purestWhite bare-metal"
#define WOLFSSL_IGNORE_FILE_WARN

/* No operating system: one thread, no files, no sockets. */
#define SINGLE_THREADED
#define NO_FILESYSTEM
#define NO_WRITEV
#define WOLFSSL_USER_IO
#define WOLFSSL_NO_SOCK
#define NO_STDIO_FILESYSTEM
#define NO_SESSION_CACHE

/* A client only; the host tests build the server too, to shake hands with. */
#ifndef PURESTWHITE_TLS_TEST_SERVER
    #define NO_WOLFSSL_SERVER
#endif

/* TLS 1.3 and 1.2. */
#define WOLFSSL_TLS13
#define NO_OLD_TLS
#define HAVE_TLS_EXTENSIONS
#define HAVE_SUPPORTED_CURVES
#define HAVE_SNI
#define HAVE_HKDF
#define HAVE_AEAD
#define WC_RSA_PSS
/* No WOLFSSL_ALT_CERT_CHAINS (the C++ firmware's build has none): it clears an intermediate's
   no-signer error before the verify callback and never adds it as a CA, so the bundle check
   (src/bundle.rs) would not run and the rest of the chain could not be checked. Without it, PEM
   roots too must sign the topmost certificate a server sends. */
/* TLS 1.2 with AEAD suites only (no CBC, MAC-then-encrypt), its extended master secret, and RSA
 * keys of 2048 bits at least. */
#define WOLFSSL_AEAD_ONLY
#define HAVE_EXTENDED_MASTER
#define WOLFSSL_MIN_RSA_BITS 2048
/* PEM roots (`Context::new`) and the tests' certificates come as PEM, which wolfSSL reads
   without files only so. */
#define WOLFSSL_PEM_TO_DER

/* Math: sp_int for RSA, single precision for P-256 and P-384, as the C++ firmware has it. */
#define WOLFSSL_SP_MATH_ALL
#define WOLFSSL_HAVE_SP_ECC
#define WOLFSSL_HAVE_SP_RSA
#define WOLFSSL_SP_SMALL
#define WOLFSSL_SP_384
#define WOLFSSL_SP_4096
#define SP_INT_BITS 4096
#define WOLFSSL_SMALL_STACK
#define TFM_TIMING_RESISTANT
#define ECC_TIMING_RESISTANT
#define WC_RSA_BLINDING

/* Algorithms. */
#define HAVE_ECC
#define ECC_SHAMIR
#define HAVE_CURVE25519
#define CURVE25519_SMALL
#define WOLFSSL_SHA384
#define WOLFSSL_SHA512
#define USE_SLOW_SHA512
#define HAVE_AESGCM
#define GCM_SMALL
#define HAVE_CHACHA
#define HAVE_POLY1305
#define HAVE_ONE_TIME_AUTH
/* No finite-field Diffie-Hellman: ECDHE and X25519 serve TLS 1.3 and 1.2 (and dh.c wants libm). */
#define NO_DH
#define NO_DSA
#define NO_RC4
#define NO_MD4
#define NO_DES3
#define NO_PSK
#define NO_PWDBASED
#define WOLFSSL_NO_SHAKE128
#define WOLFSSL_NO_SHAKE256

/* Memory, time and random bytes from Rust (hooks.rs). With XMALLOC_USER wolfSSL calls
 * XMALLOC(size, heap, type), XFREE(ptr, heap, type) and XREALLOC(ptr, size, heap, type). */
#define XMALLOC_USER

/* Seconds since 1970 (the device's clock), through XTIME(time_t *): certificates' dates are
 * checked against it. */
#define USER_TIME
#define HAVE_TIME_T_TYPE
#define HAVE_TM_TYPE

int pw_tls_random(unsigned char *output, unsigned int size);
#define CUSTOM_RAND_TYPE unsigned int
#define CUSTOM_RAND_GENERATE_BLOCK pw_tls_random

/* Flash: no error strings (the codes are kept). */
#define NO_ERROR_STRINGS

#endif
