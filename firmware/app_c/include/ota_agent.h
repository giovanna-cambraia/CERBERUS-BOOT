#ifndef OTA_AGENT_H
#define OTA_AGENT_H

#include <stdbool.h>
#include <stdint.h>

#include "cerberus_flash.h"
#include "cerberus_nv.h"
#include "ota_parser.h"

#define OTA_IMAGE_HEADER_SIZE 256u
#define OTA_MAX_REPLY         24u  /* longest reply frame: header 8 + payload 4 + CRC 4, rounded up */

typedef enum {
    OTA_A_IDLE = 0,
    OTA_A_RECEIVING,
    OTA_A_DONE
} ota_astate_t;

typedef struct {
    const cerberus_flash_t *flash;
    uint32_t nv_base;
    uint32_t slot_base[2];
    uint32_t slot_size;
    ota_parser_t parser;
    ota_astate_t state;
    nv_slot_t target;
    uint32_t total_size;
    uint32_t next_offset;
    uint32_t crc_run;
} ota_agent_t;

typedef struct {
    uint32_t consumed;  /* input bytes used */
    uint32_t resp_len;  /* reply bytes written */
    bool staged;        /* an update was completed and marked pending: reset to apply it */
} ota_feed_result_t;

/* Returns false if the configuration is unusable (null pointers, zero or
 * unaligned sizes). */
bool ota_agent_init(ota_agent_t *a, const cerberus_flash_t *flash, uint32_t nv_base,
                    const uint32_t slot_base[2], uint32_t slot_size);

/* Feed received bytes. Replies are appended to resp. Stops early, with
 * consumed < len, when resp has no room for another reply. */
ota_feed_result_t ota_agent_feed(ota_agent_t *a, const uint8_t *data, uint32_t len,
                                 uint8_t *resp, uint32_t resp_cap);

#endif