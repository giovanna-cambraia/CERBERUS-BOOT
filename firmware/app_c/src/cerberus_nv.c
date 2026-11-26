#include "cerberus_nv.h"
#include "cerberus_crc32.h"
#include "cerberus_le.h"

#define NV_CRC_OFFSET (NV_RECORD_SIZE - 4u)

uint32_t nv_slot_index(nv_slot_t s)
{
    return (s == NV_SLOT_B) ? 1u : 0u;
}

nv_slot_t nv_other_slot(nv_slot_t s)
{
    return (s == NV_SLOT_A) ? NV_SLOT_B : NV_SLOT_A;
}

nv_state_t nv_factory_state(void)
{
    nv_state_t s;

    s.seq = 0u;
    s.active = NV_SLOT_A;
    s.status[0] = NV_STATUS_CONFIRMED;
    s.status[1] = NV_STATUS_EMPTY;
    s.trials_left = 0u;
    return s;
}

void nv_encode(const nv_state_t *s, uint8_t out[NV_RECORD_SIZE])
{
    uint32_t i;
    uint32_t crc;

    for (i = 0u; i < NV_RECORD_SIZE; i++) {
        out[i] = 0u;
    }
    le_put32(out, 0u, NV_MAGIC);
    le_put16(out, 4u, (uint16_t)NV_VERSION);
    le_put32(out, 8u, s->seq);
    out[12] = (uint8_t)s->active;
    out[13] = (uint8_t)s->status[0];
    out[14] = (uint8_t)s->status[1];
    out[15] = s->trials_left;
    crc = crc32_final(crc32_update(CRC32_INIT, out, NV_CRC_OFFSET));
    le_put32(out, NV_CRC_OFFSET, crc);
}

bool nv_decode(const uint8_t in[NV_RECORD_SIZE], nv_state_t *s)
{
    uint32_t i;
    uint32_t crc;

    if (le_get32(in, 0u) != NV_MAGIC) {
        return false;
    }
    if (le_get16(in, 4u) != (uint16_t)NV_VERSION) {
        return false;
    }
    crc = crc32_final(crc32_update(CRC32_INIT, in, NV_CRC_OFFSET));
    if (le_get32(in, NV_CRC_OFFSET) != crc) {
        return false;
    }
    if ((in[6] != 0u) || (in[7] != 0u)) {
        return false;
    }
    for (i = 16u; i < NV_CRC_OFFSET; i++) {
        if (in[i] != 0u) {
            return false;
        }
    }
    if ((in[12] > 1u) || (in[13] > 5u) || (in[14] > 5u)) {
        return false;
    }

    s->seq = le_get32(in, 8u);
    s->active = (nv_slot_t)in[12];
    s->status[0] = (nv_status_t)in[13];
    s->status[1] = (nv_status_t)in[14];
    s->trials_left = in[15];
    return true;
}

/* True if b is newer than a, comparing sequence numbers modulo 2^32. */
static bool seq_newer(uint32_t a, uint32_t b)
{
    uint32_t d = b - a;

    return (d != 0u) && (d < 0x80000000u);
}

static int32_t find_newest(const cerberus_flash_t *f, uint32_t base,
                           uint32_t *idx, nv_state_t *st, bool *have)
{
    nv_state_t rec[2];
    bool valid[2];
    uint32_t i;

    for (i = 0u; i < NV_SECTOR_COUNT; i++) {
        uint8_t buf[NV_RECORD_SIZE];

        rec[i] = nv_factory_state();
        if (f->read(f->ctx, base + (i * f->sector_size), buf, NV_RECORD_SIZE) != FLASH_OK) {
            return NV_ERR_FLASH;
        }
        valid[i] = nv_decode(buf, &rec[i]);
    }

    *have = false;
    *idx = 0u;
    if (valid[0] && valid[1]) {
        *have = true;
        *idx = seq_newer(rec[0].seq, rec[1].seq) ? 1u : 0u;
        *st = rec[*idx];
    } else if (valid[0]) {
        *have = true;
        *idx = 0u;
        *st = rec[0];
    } else if (valid[1]) {
        *have = true;
        *idx = 1u;
        *st = rec[1];
    } else {
        /* nothing valid: *have stays false */
    }
    return NV_OK;
}

int32_t nv_load(const cerberus_flash_t *f, uint32_t base, nv_state_t *out, bool *found)
{
    uint32_t idx = 0u;
    nv_state_t st = nv_factory_state();
    bool have = false;
    int32_t rc = find_newest(f, base, &idx, &st, &have);

    if (rc != NV_OK) {
        return rc;
    }
    *out = have ? st : nv_factory_state();
    *found = have;
    return NV_OK;
}

int32_t nv_commit(const cerberus_flash_t *f, uint32_t base, nv_state_t *st)
{
    uint32_t idx = 0u;
    nv_state_t cur = nv_factory_state();
    bool have = false;
    uint32_t target;
    uint32_t addr;
    uint8_t rec[NV_RECORD_SIZE];
    int32_t rc = find_newest(f, base, &idx, &cur, &have);

    if (rc != NV_OK) {
        return rc;
    }
    if (have) {
        target = 1u - idx;
        st->seq = cur.seq + 1u;
    } else {
        target = 0u;
        st->seq = 1u;
    }

    nv_encode(st, rec);
    addr = base + (target * f->sector_size);
    if (f->erase_sector(f->ctx, addr) != FLASH_OK) {
        return NV_ERR_FLASH;
    }
    if (f->write(f->ctx, addr, rec, NV_RECORD_SIZE) != FLASH_OK) {
        return NV_ERR_FLASH;
    }
    return NV_OK;
}

static bool any_status(const nv_state_t *s, nv_status_t v)
{
    return (s->status[0] == v) || (s->status[1] == v);
}

int32_t nv_begin_update(const cerberus_flash_t *f, uint32_t base, nv_slot_t slot)
{
    nv_state_t st;
    bool found;
    int32_t rc = nv_load(f, base, &st, &found);

    if (rc != NV_OK) {
        return rc;
    }
    if ((slot == st.active) || any_status(&st, NV_STATUS_TESTING)) {
        return NV_ERR_NOT_ALLOWED;
    }
    st.status[nv_slot_index(slot)] = NV_STATUS_EMPTY;
    return nv_commit(f, base, &st);
}

int32_t nv_mark_pending(const cerberus_flash_t *f, uint32_t base, nv_slot_t slot)
{
    nv_state_t st;
    bool found;
    int32_t rc = nv_load(f, base, &st, &found);

    if (rc != NV_OK) {
        return rc;
    }
    if ((slot == st.active) || any_status(&st, NV_STATUS_TESTING)
        || (st.status[nv_slot_index(slot)] != NV_STATUS_EMPTY)) {
        return NV_ERR_NOT_ALLOWED;
    }
    st.status[nv_slot_index(slot)] = NV_STATUS_PENDING;
    return nv_commit(f, base, &st);
}

int32_t nv_confirm(const cerberus_flash_t *f, uint32_t base, nv_slot_t slot)
{
    nv_state_t st;
    bool found;
    int32_t rc = nv_load(f, base, &st, &found);

    if (rc != NV_OK) {
        return rc;
    }
    if (st.status[nv_slot_index(slot)] != NV_STATUS_TESTING) {
        return NV_ERR_NOT_ALLOWED;
    }
    st.status[nv_slot_index(slot)] = NV_STATUS_CONFIRMED;
    st.active = slot;
    st.trials_left = 0u;
    return nv_commit(f, base, &st);
}