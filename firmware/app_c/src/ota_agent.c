#include "ota_agent.h"

#include <stddef.h>

#include "cerberus_crc32.h"
#include "cerberus_le.h"

static uint32_t send_ack(uint16_t seq, uint32_t next_offset, uint8_t *out, uint32_t cap)
{
    uint8_t payload[4];

    le_put32(payload, 0u, next_offset);
    return ota_encode_frame(OTA_TYPE_ACK, seq, payload, 4u, out, cap);
}

static uint32_t send_nak(uint16_t seq, uint8_t reason, uint8_t *out, uint32_t cap)
{
    return ota_encode_frame(OTA_TYPE_NAK, seq, &reason, 1u, out, cap);
}

static uint32_t handle_begin(ota_agent_t *a, const ota_frame_t *fr, uint8_t *out, uint32_t cap)
{
    uint32_t total;
    uint32_t off;
    uint32_t base;
    nv_state_t st;
    bool found;
    nv_slot_t passive;
    int32_t rc;

    if (fr->len != 4u) {
        return send_nak(fr->seq, OTA_NAK_BAD_LENGTH, out, cap);
    }
    total = le_get32(fr->payload, 0u);
    if ((total <= OTA_IMAGE_HEADER_SIZE) || (total > a->slot_size)) {
        return send_nak(fr->seq, OTA_NAK_BAD_SIZE, out, cap);
    }

    rc = nv_load(a->flash, a->nv_base, &st, &found);
    if (rc != NV_OK) {
        return send_nak(fr->seq, OTA_NAK_FLASH, out, cap);
    }
    passive = nv_other_slot(st.active);
    rc = nv_begin_update(a->flash, a->nv_base, passive);
    if (rc == NV_ERR_NOT_ALLOWED) {
        return send_nak(fr->seq, OTA_NAK_NOT_ALLOWED, out, cap);
    }
    if (rc != NV_OK) {
        return send_nak(fr->seq, OTA_NAK_FLASH, out, cap);
    }

    /* Erase the sectors the image will occupy, in the passive slot only. */
    base = a->slot_base[nv_slot_index(passive)];
    a->state = OTA_A_IDLE;
    for (off = 0u; off < total; off += a->flash->sector_size) {
        if (a->flash->erase_sector(a->flash->ctx, base + off) != FLASH_OK) {
            return send_nak(fr->seq, OTA_NAK_FLASH, out, cap);
        }
    }

    a->state = OTA_A_RECEIVING;
    a->target = passive;
    a->total_size = total;
    a->next_offset = 0u;
    a->crc_run = CRC32_INIT;
    return send_ack(fr->seq, 0u, out, cap);
}

static uint32_t handle_data(ota_agent_t *a, const ota_frame_t *fr, uint8_t *out, uint32_t cap)
{
    uint32_t off;
    uint32_t n;

    if (a->state != OTA_A_RECEIVING) {
        return send_nak(fr->seq, OTA_NAK_SEQUENCE, out, cap);
    }
    if ((uint32_t)fr->len < 5u) {
        return send_nak(fr->seq, OTA_NAK_BAD_LENGTH, out, cap);
    }

    off = le_get32(fr->payload, 0u);
    n = (uint32_t)fr->len - 4u;

    if (off > a->next_offset) {
        return send_nak(fr->seq, OTA_NAK_OUT_OF_ORDER, out, cap);
    }
    if (off < a->next_offset) {
        /* A retransmission of bytes that were already written. */
        if ((off + n) <= a->next_offset) {
            return send_ack(fr->seq, a->next_offset, out, cap);
        }
        return send_nak(fr->seq, OTA_NAK_OUT_OF_ORDER, out, cap);
    }

    /* off == next_offset, which never exceeds total_size. */
    if (n > (a->total_size - off)) {
        return send_nak(fr->seq, OTA_NAK_BAD_SIZE, out, cap);
    }
    if (a->flash->write(a->flash->ctx,
                        a->slot_base[nv_slot_index(a->target)] + off,
                        &fr->payload[4], n) != FLASH_OK) {
        a->state = OTA_A_IDLE;
        return send_nak(fr->seq, OTA_NAK_FLASH, out, cap);
    }
    a->crc_run = crc32_update(a->crc_run, &fr->payload[4], n);
    a->next_offset += n;
    return send_ack(fr->seq, a->next_offset, out, cap);
}

static uint32_t handle_end(ota_agent_t *a, const ota_frame_t *fr, uint8_t *out, uint32_t cap,
                           bool *staged)
{
    int32_t rc;

    if (a->state != OTA_A_RECEIVING) {
        return send_nak(fr->seq, OTA_NAK_SEQUENCE, out, cap);
    }
    if (fr->len != 4u) {
        return send_nak(fr->seq, OTA_NAK_BAD_LENGTH, out, cap);
    }
    if (a->next_offset != a->total_size) {
        return send_nak(fr->seq, OTA_NAK_INCOMPLETE, out, cap);
    }
    if (le_get32(fr->payload, 0u) != crc32_final(a->crc_run)) {
        a->state = OTA_A_IDLE;
        return send_nak(fr->seq, OTA_NAK_BAD_IMAGE_CRC, out, cap);
    }

    rc = nv_mark_pending(a->flash, a->nv_base, a->target);
    if (rc == NV_ERR_NOT_ALLOWED) {
        a->state = OTA_A_IDLE;
        return send_nak(fr->seq, OTA_NAK_NOT_ALLOWED, out, cap);
    }
    if (rc != NV_OK) {
        return send_nak(fr->seq, OTA_NAK_FLASH, out, cap);
    }

    a->state = OTA_A_DONE;
    *staged = true;
    return send_ack(fr->seq, a->total_size, out, cap);
}

static uint32_t handle_frame(ota_agent_t *a, const ota_frame_t *fr, uint8_t *out, uint32_t cap,
                             bool *staged)
{
    uint32_t n;

    switch (fr->type) {
    case OTA_TYPE_BEGIN:
        n = handle_begin(a, fr, out, cap);
        break;
    case OTA_TYPE_DATA:
        n = handle_data(a, fr, out, cap);
        break;
    case OTA_TYPE_END:
        n = handle_end(a, fr, out, cap, staged);
        break;
    default:
        n = send_nak(fr->seq, OTA_NAK_UNKNOWN_TYPE, out, cap);
        break;
    }
    return n;
}

bool ota_agent_init(ota_agent_t *a, const cerberus_flash_t *flash, uint32_t nv_base,
                    const uint32_t slot_base[2], uint32_t slot_size)
{
    bool ok = (a != NULL) && (flash != NULL) && (slot_base != NULL);

    if (ok) {
        ok = (flash->read != NULL) && (flash->erase_sector != NULL) && (flash->write != NULL)
             && (flash->sector_size != 0u) && (slot_size != 0u)
             && ((slot_size % flash->sector_size) == 0u)
             && ((slot_base[0] % flash->sector_size) == 0u)
             && ((slot_base[1] % flash->sector_size) == 0u);
    }
    if (ok) {
        a->flash = flash;
        a->nv_base = nv_base;
        a->slot_base[0] = slot_base[0];
        a->slot_base[1] = slot_base[1];
        a->slot_size = slot_size;
        a->state = OTA_A_IDLE;
        a->target = NV_SLOT_B;
        a->total_size = 0u;
        a->next_offset = 0u;
        a->crc_run = CRC32_INIT;
        ota_parser_init(&a->parser);
    }
    return ok;
}

ota_feed_result_t ota_agent_feed(ota_agent_t *a, const uint8_t *data, uint32_t len,
                                 uint8_t *resp, uint32_t resp_cap)
{
    ota_feed_result_t r;
    uint32_t i;

    r.consumed = 0u;
    r.resp_len = 0u;
    r.staged = false;

    for (i = 0u; i < len; i++) {
        ota_event_t ev;

        if ((r.resp_len + OTA_MAX_REPLY) > resp_cap) {
            break;
        }
        ev = ota_parser_push(&a->parser, data[i]);
        r.consumed = i + 1u;

        if (ev == OTA_EVT_FRAME) {
            r.resp_len += handle_frame(a, &a->parser.frame, &resp[r.resp_len],
                                       resp_cap - r.resp_len, &r.staged);
        } else if (ev == OTA_EVT_ERROR) {
            r.resp_len += send_nak(0u, OTA_NAK_BAD_FRAME, &resp[r.resp_len],
                                   resp_cap - r.resp_len);
        } else {
            /* Mid-frame: nothing to do yet. */
        }
    }
    return r;
}