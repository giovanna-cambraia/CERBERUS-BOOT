/* Test application: confirms itself when it boots on trial, then runs the OTA
 * agent on the update stream in the mailbox.
 *   APP_VERSION       version number reported (default 1)
 *   APP_SELFTEST_FAIL 1 builds a version whose self-test always fails */
#include <stdbool.h>
#include <stdint.h>

#include "app_platform.h"
#include "cerberus_crc32.h"
#include "cerberus_nv.h"
#include "ota_agent.h"

#ifndef APP_VERSION
#define APP_VERSION 1u
#endif
#ifndef APP_SELFTEST_FAIL
#define APP_SELFTEST_FAIL 0
#endif

#define CONFIRM_NONE             0u
#define CONFIRM_DONE             1u
#define CONFIRM_SELFTEST_FAILED  2u
#define CONFIRM_NV_ERROR         3u

#define CHUNK_BYTES 64u

void app_main(void);

static ota_agent_t g_agent;

static bool self_test(void)
{
    static const uint8_t vector[9] = { 0x31u, 0x32u, 0x33u, 0x34u, 0x35u, 0x36u, 0x37u, 0x38u, 0x39u };
    uint8_t pattern[32];
    uint32_t i;
    bool ok;

    ok = (crc32_final(crc32_update(CRC32_INIT, vector, 9u)) == 0xCBF43926u);
    for (i = 0u; i < 32u; i++) {
        pattern[i] = (uint8_t)(i * 5u);
    }
    for (i = 0u; i < 32u; i++) {
        if (pattern[i] != (uint8_t)(i * 5u)) {
            ok = false;
        }
    }
#if APP_SELFTEST_FAIL
    ok = false;
#endif
    return ok;
}

static void run_ota(uint32_t *staged, uint32_t *consumed)
{
    static const uint32_t slot_base[2] = { APP_SLOT_A_BASE, APP_SLOT_B_BASE };
    uint8_t chunk[CHUNK_BYTES];
    uint8_t resp[256];
    uint32_t total = platform_rx_length();
    uint32_t pos = 0u;

    *staged = 0u;
    *consumed = 0u;
    if (total == 0u) {
        return;
    }
    if (!ota_agent_init(&g_agent, platform_flash(), NV_BASE_ADDR, slot_base, APP_SLOT_SIZE)) {
        return;
    }

    while ((pos < total) && (*staged == 0u)) {
        uint32_t n = total - pos;
        uint32_t off = 0u;
        uint32_t i;

        if (n > CHUNK_BYTES) {
            n = CHUNK_BYTES;
        }
        for (i = 0u; i < n; i++) {
            chunk[i] = platform_rx_byte(pos + i);
        }
        while (off < n) {
            ota_feed_result_t r = ota_agent_feed(&g_agent, &chunk[off], n - off,
                                                 resp, (uint32_t)sizeof resp);

            platform_tx_append(resp, r.resp_len);
            off += r.consumed;
            if (r.staged) {
                *staged = 1u;
            }
            if (r.consumed == 0u) {
                break;
            }
        }
        pos += n;
        *consumed = pos;
    }
}

void app_main(void)
{
    const cerberus_flash_t *flash = platform_flash();
    nv_slot_t me = platform_running_slot();
    nv_state_t st;
    bool found = false;
    uint32_t trial = 0u;
    uint32_t confirm = CONFIRM_NONE;
    uint32_t staged = 0u;
    uint32_t consumed = 0u;

    /* On a trial boot: run the self-test and, only if it passes, confirm. */
    if ((nv_load(flash, NV_BASE_ADDR, &st, &found) == NV_OK)
        && (st.status[nv_slot_index(me)] == NV_STATUS_TESTING)) {
        trial = 1u;
        if (self_test()) {
            confirm = (nv_confirm(flash, NV_BASE_ADDR, me) == NV_OK) ? CONFIRM_DONE
                                                                     : CONFIRM_NV_ERROR;
        } else {
            confirm = CONFIRM_SELFTEST_FAILED;
        }
    }
    platform_report(APP_VERSION, nv_slot_index(me), trial, confirm, 0u, 0u);

    run_ota(&staged, &consumed);
    platform_report(APP_VERSION, nv_slot_index(me), trial, confirm, staged, consumed);

    platform_idle();
}