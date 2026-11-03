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

/* The bundle check (src/bundle.rs): the C side reads wolfSSL's store and wolfCrypt checks the
   signature; the Rust side reads the certificate, finds its root and checks its dates. */
#include <wolfssl/wolfcrypt/asn_public.h>
#include <wolfssl/wolfcrypt/ecc.h>
#include <wolfssl/wolfcrypt/error-crypt.h>
#include <wolfssl/wolfcrypt/rsa.h>
#include <wolfssl/wolfcrypt/signature.h>

/* Whether `der` (`len` bytes), a certificate wolfSSL found no signer for, is signed by a root of
   the bundle `check` holds and within its dates: 1 or 0. */
extern int pw_tls_bundle_accepts(const void *check, const unsigned char *der, unsigned int len);

/* wolfSSL's verify callback for a context trusting a bundle, as the C++ firmware's
   SecureClient::verifyPeer: a certificate wolfSSL found no signer for, other than the server's
   own (a root signs intermediates only), is looked up in the bundle; every other failure
   stands. */
int pw_tls_verify(int preverified, WOLFSSL_X509_STORE_CTX *store)
{
    if (preverified) return 1;
    if (store == NULL || store->userCtx == NULL || store->certs == NULL) return 0;
    if (store->error != ASN_NO_SIGNER_E || store->error_depth < 1 ||
        store->error_depth >= store->totalCerts) {
        return 0;
    }
    const WOLFSSL_BUFFER_INFO *der = &store->certs[store->error_depth];
    if (der->buffer == NULL) return 0;
    return pw_tls_bundle_accepts(store->userCtx, der->buffer, der->length);
}

/* The hashes and key kinds src/bundle.rs passes (its SHA* and RSA/EC constants). */
enum { PW_SHA1 = 0, PW_SHA256 = 1, PW_SHA384 = 2, PW_SHA512 = 3 };
enum { PW_RSA = 0, PW_EC = 1 };

static enum wc_HashType pw_hash(int hash)
{
    switch (hash) {
    case PW_SHA1: return WC_HASH_TYPE_SHA;
    case PW_SHA256: return WC_HASH_TYPE_SHA256;
    case PW_SHA384: return WC_HASH_TYPE_SHA384;
    case PW_SHA512: return WC_HASH_TYPE_SHA512;
    default: return WC_HASH_TYPE_NONE;
    }
}

static int pw_rsa_verify(const unsigned char *spki, word32 spki_len, enum wc_HashType hash,
                         const unsigned char *data, word32 data_len,
                         const unsigned char *sig, word32 sig_len)
{
    /* On the heap: an RsaKey is kilobytes with 4096-bit numbers, too much for a callback's
       stack. */
    RsaKey *key = (RsaKey *)XMALLOC(sizeof(RsaKey), NULL, DYNAMIC_TYPE_RSA);
    if (key == NULL) return MEMORY_E;
    word32 at = 0;
    int ret = wc_InitRsaKey(key, NULL);
    if (ret == 0) {
        ret = wc_RsaPublicKeyDecode(spki, &at, key, spki_len);
        if (ret == 0) {
            ret = wc_SignatureVerify(hash, WC_SIGNATURE_TYPE_RSA_W_ENC, data, data_len, sig,
                                     sig_len, key, sizeof(RsaKey));
        }
        wc_FreeRsaKey(key);
    }
    XFREE(key, NULL, DYNAMIC_TYPE_RSA);
    return ret;
}

static int pw_ecc_verify(const unsigned char *spki, word32 spki_len, enum wc_HashType hash,
                         const unsigned char *data, word32 data_len,
                         const unsigned char *sig, word32 sig_len)
{
    ecc_key *key = (ecc_key *)XMALLOC(sizeof(ecc_key), NULL, DYNAMIC_TYPE_ECC);
    if (key == NULL) return MEMORY_E;
    word32 at = 0;
    int ret = wc_ecc_init(key);
    if (ret == 0) {
        ret = wc_EccPublicKeyDecode(spki, &at, key, spki_len);
        if (ret == 0) {
            ret = wc_SignatureVerify(hash, WC_SIGNATURE_TYPE_ECC, data, data_len, sig, sig_len,
                                     key, sizeof(ecc_key));
        }
        wc_ecc_free(key);
    }
    XFREE(key, NULL, DYNAMIC_TYPE_ECC);
    return ret;
}

/* Whether `sig` is the signature over `data` of the key `spki` (a SubjectPublicKeyInfo), of
   `kind`, over `hash`: 0 when it is, else wolfCrypt's error. */
int pw_tls_signature_check(const unsigned char *spki, unsigned int spki_len, int kind, int hash,
                           const unsigned char *data, unsigned int data_len,
                           const unsigned char *sig, unsigned int sig_len)
{
    const enum wc_HashType type = pw_hash(hash);
    if (type == WC_HASH_TYPE_NONE) return BAD_FUNC_ARG;
    switch (kind) {
    case PW_RSA: return pw_rsa_verify(spki, spki_len, type, data, data_len, sig, sig_len);
    case PW_EC: return pw_ecc_verify(spki, spki_len, type, data, data_len, sig, sig_len);
    default: return BAD_FUNC_ARG;
    }
}

_Static_assert(ASN_NO_SIGNER_E == -188, "bundle.rs's NO_SIGNER");
_Static_assert(MEMORY_E == -125 && MP_MEM == -2, "bundle.rs's OUT_OF_MEMORY");
SAME(wolfSSL_CTX_SetCertCbCtx, void (*)(WOLFSSL_CTX *, void *));
SAME(pw_tls_verify, VerifyCallback);
