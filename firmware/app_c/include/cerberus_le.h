#ifndef CERBERUS_LE_H
#define CERBERUS_LE_H

#include <stdint.h>

/* Little-endian access to byte buffers, independent of the host's byte order. */

static inline uint16_t le_get16(const uint8_t *b, uint32_t off)
{
    return (uint16_t)((uint32_t)b[off] | ((uint32_t)b[off + 1u] << 8));
}

static inline uint32_t le_get32(const uint8_t *b, uint32_t off)
{
    return (uint32_t)b[off]
         | ((uint32_t)b[off + 1u] << 8)
         | ((uint32_t)b[off + 2u] << 16)
         | ((uint32_t)b[off + 3u] << 24);
}

static inline void le_put16(uint8_t *b, uint32_t off, uint16_t v)
{
    b[off] = (uint8_t)((uint32_t)v & 0xFFu);
    b[off + 1u] = (uint8_t)((uint32_t)v >> 8);
}

static inline void le_put32(uint8_t *b, uint32_t off, uint32_t v)
{
    b[off] = (uint8_t)(v & 0xFFu);
    b[off + 1u] = (uint8_t)((v >> 8) & 0xFFu);
    b[off + 2u] = (uint8_t)((v >> 16) & 0xFFu);
    b[off + 3u] = (uint8_t)((v >> 24) & 0xFFu);
}

#endif 