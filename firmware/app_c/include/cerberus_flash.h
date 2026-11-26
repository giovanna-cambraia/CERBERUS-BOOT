#ifndef CERBERUS_FLASH_H
#define CERBERUS_FLASH_H

#include <stdint.h>

/* Flash as the agent sees it. NOR semantics: an erased byte reads 0xFF and a
 * write can only clear bits. All functions return FLASH_OK or a negative code. */
#define FLASH_OK          0
#define FLASH_ERR_RANGE   (-1)
#define FLASH_ERR_DEVICE  (-2)

typedef struct cerberus_flash {
    void *ctx;
    uint32_t sector_size;
    int32_t (*read)(void *ctx, uint32_t addr, uint8_t *buf, uint32_t len);
    int32_t (*erase_sector)(void *ctx, uint32_t addr);
    int32_t (*write)(void *ctx, uint32_t addr, const uint8_t *data, uint32_t len);
} cerberus_flash_t;

#endif 