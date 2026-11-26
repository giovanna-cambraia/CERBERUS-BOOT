#ifndef MOCK_FLASH_H
#define MOCK_FLASH_H

#include <stdint.h>

#include "cerberus_flash.h"
#include "cerberus_nv.h"

#define MOCK_SECTOR      4096u
#define MOCK_SLOT_SIZE   0x10000u        /* small slots keep the tests fast */
#define MOCK_SLOT_A_BASE 0x40020000u
#define MOCK_SLOT_B_BASE 0x40100000u

/* NOR flash model covering the NV region and the two slots. `budget` is the
 * number of byte operations left before the "power is cut" (a negative budget
 * never cuts), so a test can stop an erase or a write at any byte. */
typedef struct {
    uint8_t nv[2u * MOCK_SECTOR];
    uint8_t slot[2][MOCK_SLOT_SIZE];
    int64_t budget;
} mock_flash_t;

void mock_init(mock_flash_t *m);
void mock_make_hal(mock_flash_t *m, cerberus_flash_t *hal);

#endif 