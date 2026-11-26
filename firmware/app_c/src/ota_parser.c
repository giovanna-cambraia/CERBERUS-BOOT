#include "ota_parser.h"
#include "cerberus_crc32.h"
#include "cerberus_le.h"

void ota_parser_init(ota_parser_t *p)
{
    uint32_t i;

    p->state = OTA_P_SYNC0;
    p->pos = 0u;
    p->crc = CRC32_INIT;
    p->errors = 0u;
    for (i = 0u; i < OTA_HDR_BYTES; i++) {
        p->hdr[i] = 0u;
    }
    for (i = 0u; i < 4u; i++) {
        p->crc_bytes[i] = 0u;
    }
    p->frame.type = 0u;
    p->frame.seq = 0u;
    p->frame.len = 0u;
    for (i = 0u; i < OTA_MAX_PAYLOAD; i++) {
        p->frame.payload[i] = 0u;
    }
}

static void parser_reset(ota_parser_t *p)
{
    p->state = OTA_P_SYNC0;
    p->pos = 0u;
}

static ota_event_t parser_fail(ota_parser_t *p)
{
    p->errors++;
    parser_reset(p);
    return OTA_EVT_ERROR;
}

/* All six header bytes are in: check them and move on. */
static ota_event_t header_complete(ota_parser_t *p)
{
    uint32_t len = (uint32_t)le_get16(p->hdr, 4u);

    if ((p->hdr[1] != 0u) || (len > OTA_MAX_PAYLOAD)) {
        return parser_fail(p);
    }
    p->frame.type = p->hdr[0];
    p->frame.seq = le_get16(p->hdr, 2u);
    p->frame.len = (uint16_t)len;
    p->pos = 0u;
    p->state = (len == 0u) ? OTA_P_CRC : OTA_P_PAYLOAD;
    return OTA_EVT_NONE;
}

ota_event_t ota_parser_push(ota_parser_t *p, uint8_t byte)
{
    ota_event_t ev = OTA_EVT_NONE;

    switch (p->state) {
    case OTA_P_SYNC0:
        if (byte == OTA_SYNC0) {
            p->state = OTA_P_SYNC1;
        }
        break;

    case OTA_P_SYNC1:
        if (byte == OTA_SYNC1) {
            p->state = OTA_P_HEADER;
            p->pos = 0u;
            p->crc = CRC32_INIT;
        } else if (byte != OTA_SYNC0) {
            p->state = OTA_P_SYNC0;
        } else {
            /* Another first sync byte: keep waiting for the second one. */
        }
        break;

    case OTA_P_HEADER:
        if (p->pos < OTA_HDR_BYTES) {
            p->hdr[p->pos] = byte;
            p->pos++;
            p->crc = crc32_update(p->crc, &byte, 1u);
        }
        if (p->pos >= OTA_HDR_BYTES) {
            ev = header_complete(p);
        }
        break;

    case OTA_P_PAYLOAD:
        if (p->pos < OTA_MAX_PAYLOAD) {
            p->frame.payload[p->pos] = byte;
            p->pos++;
            p->crc = crc32_update(p->crc, &byte, 1u);
        }
        if (p->pos >= (uint32_t)p->frame.len) {
            p->state = OTA_P_CRC;
            p->pos = 0u;
        }
        break;

    case OTA_P_CRC:
        if (p->pos < 4u) {
            p->crc_bytes[p->pos] = byte;
            p->pos++;
        }
        if (p->pos >= 4u) {
            if (le_get32(p->crc_bytes, 0u) == crc32_final(p->crc)) {
                ev = OTA_EVT_FRAME;
                parser_reset(p);
            } else {
                ev = parser_fail(p);
            }
        }
        break;

    default:
        parser_reset(p);
        break;
    }
    return ev;
}

uint32_t ota_encode_frame(uint8_t type, uint16_t seq, const uint8_t *payload,
                          uint16_t len, uint8_t *out, uint32_t cap)
{
    uint32_t n = (uint32_t)len;
    uint32_t total = n + OTA_FRAME_OVERHEAD;
    uint32_t i;
    uint32_t crc;

    if ((n > OTA_MAX_PAYLOAD) || (cap < total)) {
        return 0u;
    }
    out[0] = OTA_SYNC0;
    out[1] = OTA_SYNC1;
    out[2] = type;
    out[3] = 0u;
    le_put16(out, 4u, seq);
    le_put16(out, 6u, len);
    for (i = 0u; i < n; i++) {
        out[8u + i] = payload[i];
    }
    crc = crc32_final(crc32_update(CRC32_INIT, &out[2], OTA_HDR_BYTES + n));
    le_put32(out, 8u + n, crc);
    return total;
}