#ifndef CERBERUS_NV_H
#define CERBERUS_NV_H

#include <stdbool.h>
#include <stdint.h>

#include "cerberus_flash.h"

/* Slot-state record: firmware/common/spec/nv_state.md. The numbers below are
 * compared with nv_state.toml by `make spec-check`. */
#define NV_BASE_ADDR            0x401E0000u
#define NV_SECTOR_SIZE          4096u
#define NV_SECTOR_COUNT         2u
#define NV_RECORD_SIZE          32u
#define NV_MAGIC                0x4E565354u
#define NV_VERSION              1u
#define NV_DEFAULT_TRIAL_BOOTS  3u

#define NV_OK                   0
#define NV_ERR_FLASH            (-1)
#define NV_ERR_NOT_ALLOWED      (-2)

typedef enum {
    NV_SLOT_A = 0,
    NV_SLOT_B = 1
} nv_slot_t;

typedef enum {
    NV_STATUS_EMPTY = 0,
    NV_STATUS_PENDING = 1,
    NV_STATUS_TESTING = 2,
    NV_STATUS_CONFIRMED = 3,
    NV_STATUS_INVALID = 4,
    NV_STATUS_BROKEN = 5
} nv_status_t;

typedef struct {
    uint32_t seq;
    nv_slot_t active;
    nv_status_t status[2];  /* indexed by nv_slot_index() */
    uint8_t trials_left;
} nv_state_t;

uint32_t nv_slot_index(nv_slot_t s);
nv_slot_t nv_other_slot(nv_slot_t s);
nv_state_t nv_factory_state(void);

void nv_encode(const nv_state_t *s, uint8_t out[NV_RECORD_SIZE]);
bool nv_decode(const uint8_t in[NV_RECORD_SIZE], nv_state_t *s);

/* Newest valid record, or the factory state if there is none (*found = false). */
int32_t nv_load(const cerberus_flash_t *f, uint32_t base, nv_state_t *out, bool *found);

/* Persist *st with the next sequence number (updated in *st). Erases and writes
 * the sector that does NOT hold the newest record. */
int32_t nv_commit(const cerberus_flash_t *f, uint32_t base, nv_state_t *st);

/* Operations of nv_state.md. NV_ERR_NOT_ALLOWED when the rules refuse. */
int32_t nv_begin_update(const cerberus_flash_t *f, uint32_t base, nv_slot_t slot);
int32_t nv_mark_pending(const cerberus_flash_t *f, uint32_t base, nv_slot_t slot);
int32_t nv_confirm(const cerberus_flash_t *f, uint32_t base, nv_slot_t slot);

#endif 