#include "mock_flash.h"

#include <stdbool.h>
#include <stddef.h>
#include <string.h>

static bool in_window(uint32_t addr, uint32_t len, uint32_t base, uint32_t size)
{
    return (addr >= base) && (len <= size) && ((addr - base) <= (size - len));
}

static uint8_t *resolve(mock_flash_t *m, uint32_t addr, uint32_t len)
{
    if (in_window(addr, len, NV_BASE_ADDR, 2u * MOCK_SECTOR)) {
        return &m->nv[addr - NV_BASE_ADDR];
    }
    if (in_window(addr, len, MOCK_SLOT_A_BASE, MOCK_SLOT_SIZE)) {
        return &m->slot[0][addr - MOCK_SLOT_A_BASE];
    }
    if (in_window(addr, len, MOCK_SLOT_B_BASE, MOCK_SLOT_SIZE)) {
        return &m->slot[1][addr - MOCK_SLOT_B_BASE];
    }
    return NULL;
}

static bool spend(mock_flash_t *m)
{
    if (m->budget < 0) {
        return true;
    }
    if (m->budget == 0) {
        return false;
    }
    m->budget--;
    return true;
}

static int32_t mock_read(void *ctx, uint32_t addr, uint8_t *buf, uint32_t len)
{
    mock_flash_t *m = (mock_flash_t *)ctx;
    const uint8_t *p = resolve(m, addr, len);
    uint32_t i;

    if (p == NULL) {
        return FLASH_ERR_RANGE;
    }
    for (i = 0u; i < len; i++) {
        buf[i] = p[i];
    }
    return FLASH_OK;
}

static int32_t mock_erase(void *ctx, uint32_t addr)
{
    mock_flash_t *m = (mock_flash_t *)ctx;
    uint8_t *p = resolve(m, addr, MOCK_SECTOR);
    uint32_t i;

    if ((p == NULL) || ((addr % MOCK_SECTOR) != 0u)) {
        return FLASH_ERR_RANGE;
    }
    for (i = 0u; i < MOCK_SECTOR; i++) {
        if (!spend(m)) {
            return FLASH_ERR_DEVICE;
        }
        p[i] = 0xFFu;
    }
    return FLASH_OK;
}

static int32_t mock_write(void *ctx, uint32_t addr, const uint8_t *data, uint32_t len)
{
    mock_flash_t *m = (mock_flash_t *)ctx;
    uint8_t *p = resolve(m, addr, len);
    uint32_t i;

    if (p == NULL) {
        return FLASH_ERR_RANGE;
    }
    for (i = 0u; i < len; i++) {
        if (!spend(m)) {
            return FLASH_ERR_DEVICE;
        }
        p[i] = (uint8_t)(p[i] & data[i]);  /* NOR: programming can only clear bits */
    }
    return FLASH_OK;
}

void mock_init(mock_flash_t *m)
{
    (void)memset(m->nv, 0xFF, sizeof m->nv);
    (void)memset(m->slot, 0xFF, sizeof m->slot);
    m->budget = -1;
}

void mock_make_hal(mock_flash_t *m, cerberus_flash_t *hal)
{
    hal->ctx = m;
    hal->sector_size = MOCK_SECTOR;
    hal->read = mock_read;
    hal->erase_sector = mock_erase;
    hal->write = mock_write;
}