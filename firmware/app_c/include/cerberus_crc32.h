#ifndef CERBERUS_CRC32_H
#define CERBERUS_CRC32_H

#include <stdint.h>

/* CRC-32/ISO-HDLC (zlib's crc32), in running form:
 *   crc = CRC32_INIT;
 *   crc = crc32_update(crc, data, len);   (any number of times)
 *   result = crc32_final(crc);
 * Check value for "123456789": 0xCBF43926. */
#define CRC32_INIT 0xFFFFFFFFu

static inline uint32_t crc32_update(uint32_t crc, const uint8_t *data, uint32_t len)
{
    uint32_t c = crc;
    uint32_t i;
    uint32_t bit;

    for (i = 0u; i < len; i++) {
        c ^= (uint32_t)data[i];
        for (bit = 0u; bit < 8u; bit++) {
            if ((c & 1u) != 0u) {
                c = (c >> 1) ^ 0xEDB88320u;
            } else {
                c = c >> 1;
            }
        }
    }
    return c;
}

static inline uint32_t crc32_final(uint32_t crc)
{
    return ~crc;
}

#endif