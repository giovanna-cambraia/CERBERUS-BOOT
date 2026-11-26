#ifndef OTA_PARSER_H
#define OTA_PARSER_H

#include <stdint.h>

/* OTA frame parser: firmware/common/spec/ota_protocol.md. */
#define OTA_SYNC0           ((uint8_t)0xC3u)
#define OTA_SYNC1           ((uint8_t)0x5Au)
#define OTA_MAX_PAYLOAD     256u
#define OTA_DATA_CHUNK      252u  /* image bytes per DATA frame (4 bytes carry the offset) */
#define OTA_HDR_BYTES       6u    /* type, flags, seq (2), len (2), after the sync bytes */
#define OTA_FRAME_OVERHEAD  12u   /* 2 sync + 6 header + 4 CRC */
#define OTA_MAX_FRAME       (OTA_MAX_PAYLOAD + OTA_FRAME_OVERHEAD)

#define OTA_TYPE_BEGIN      ((uint8_t)0x01u)
#define OTA_TYPE_DATA       ((uint8_t)0x02u)
#define OTA_TYPE_END        ((uint8_t)0x03u)
#define OTA_TYPE_ACK        ((uint8_t)0x81u)
#define OTA_TYPE_NAK        ((uint8_t)0x82u)

#define OTA_NAK_BAD_FRAME     ((uint8_t)1u)
#define OTA_NAK_UNKNOWN_TYPE  ((uint8_t)2u)
#define OTA_NAK_BAD_SIZE      ((uint8_t)3u)
#define OTA_NAK_NOT_ALLOWED   ((uint8_t)4u)
#define OTA_NAK_SEQUENCE      ((uint8_t)5u)
#define OTA_NAK_OUT_OF_ORDER  ((uint8_t)6u)
#define OTA_NAK_FLASH         ((uint8_t)7u)
#define OTA_NAK_BAD_IMAGE_CRC ((uint8_t)8u)
#define OTA_NAK_INCOMPLETE    ((uint8_t)9u)
#define OTA_NAK_BAD_LENGTH    ((uint8_t)10u)

typedef enum {
    OTA_P_SYNC0 = 0,
    OTA_P_SYNC1,
    OTA_P_HEADER,
    OTA_P_PAYLOAD,
    OTA_P_CRC
} ota_pstate_t;

typedef enum {
    OTA_EVT_NONE = 0,
    OTA_EVT_FRAME,  /* a complete, valid frame is in parser->frame */
    OTA_EVT_ERROR   /* a damaged frame was dropped */
} ota_event_t;

typedef struct {
    uint8_t type;
    uint16_t seq;
    uint16_t len;
    uint8_t payload[OTA_MAX_PAYLOAD];
} ota_frame_t;

typedef struct {
    ota_pstate_t state;
    uint32_t pos;
    uint32_t crc;
    uint32_t errors;
    uint8_t hdr[OTA_HDR_BYTES];
    uint8_t crc_bytes[4];
    ota_frame_t frame;
} ota_parser_t;

void ota_parser_init(ota_parser_t *p);

/* Feed one byte. On OTA_EVT_FRAME the frame stays valid until the next call. */
ota_event_t ota_parser_push(ota_parser_t *p, uint8_t byte);

/* Build a frame. Returns its length, or 0 if the payload is too long or the
 * buffer too small. `payload` may be NULL when len is 0. */
uint32_t ota_encode_frame(uint8_t type, uint16_t seq, const uint8_t *payload,
                          uint16_t len, uint8_t *out, uint32_t cap);

#endif 