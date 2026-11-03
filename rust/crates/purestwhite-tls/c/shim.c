/* Checks the C types the Rust side mirrors (purestwhite-tls's ffi.rs and hooks.rs). */
#include <wolfssl/wolfcrypt/settings.h>
#include <wolfssl/ssl.h>

_Static_assert(sizeof(time_t) == 8, "XTIME takes and returns a 64-bit time_t");
_Static_assert(sizeof(int) == 4, "the callbacks' int is 32 bits");
_Static_assert(WOLFSSL_ERROR_WANT_READ == 2 && WOLFSSL_ERROR_WANT_WRITE == 3, "ffi.rs");
_Static_assert(WOLFSSL_CBIO_ERR_WANT_READ == -2 && WOLFSSL_CBIO_ERR_CONN_CLOSE == -5, "ffi.rs");
_Static_assert(WOLFSSL_SUCCESS == 1 && WOLFSSL_FILETYPE_PEM == 1, "ffi.rs");
_Static_assert(WOLFSSL_VERIFY_PEER == 1, "ffi.rs");
_Static_assert(WOLFSSL_LOAD_FLAG_DATE_ERR_OKAY == 0x2, "ffi.rs");
_Static_assert(WOLFSSL_ERROR_ZERO_RETURN == 6 && WOLFSSL_SNI_HOST_NAME == 0, "ffi.rs");
_Static_assert(sizeof(word32) == 4 && sizeof(long) == sizeof(void *), "ffi.rs's c_long and u32");

/* The signatures ffi.rs declares by hand. */
#define SAME(fn, type) _Static_assert(__builtin_types_compatible_p(__typeof__(&fn), type), #fn)
SAME(wolfSSL_CTX_load_verify_buffer_ex,
     int (*)(WOLFSSL_CTX *, const unsigned char *, long, int, int, word32));
SAME(wolfSSL_CTX_set_verify, void (*)(WOLFSSL_CTX *, int, VerifyCallback));
SAME(wolfSSL_UseSNI, int (*)(WOLFSSL *, unsigned char, const void *, unsigned short));
SAME(wolfSSL_check_domain_name, int (*)(WOLFSSL *, const char *));
SAME(wolfSSL_SSLSetIORecv, void (*)(WOLFSSL *, CallbackIORecv));
SAME(wolfSSL_SSLSetIOSend, void (*)(WOLFSSL *, CallbackIOSend));
SAME(wolfSSL_SetIOReadCtx, void (*)(WOLFSSL *, void *));
SAME(wolfSSL_connect, int (*)(WOLFSSL *));
SAME(wolfSSL_read, int (*)(WOLFSSL *, void *, int));
SAME(wolfSSL_write, int (*)(WOLFSSL *, const void *, int));
SAME(wolfSSL_get_error, int (*)(WOLFSSL *, int));
