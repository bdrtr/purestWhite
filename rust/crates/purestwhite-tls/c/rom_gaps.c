/* The C library function the ESP32-S3's ROM lacks and wolfSSL calls (build.rs builds this for the
 * ESP32-S3 only; the host's C library has it). newlib's own is assembly whose literals the linker
 * cannot reach from esp-hal's layout. */
#include <stddef.h>

/* Not turned into a call to strlen itself by the loop-idiom pass. */
__attribute__((optimize("no-tree-loop-distribute-patterns")))
size_t strlen(const char *s)
{
    const char *end = s;
    while (*end != '\0') {
        end++;
    }
    return (size_t)(end - s);
}
