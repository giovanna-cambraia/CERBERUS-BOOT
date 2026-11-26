/* Fuzz harness for the OTA parser and agent (VTP-001 step 4).
 *   fuzzer [--mutate N] < input
 * Feeds the input to a fresh agent in random-sized fragments, then (with
 * --mutate) does the same for N deterministic mutations of it. It is meant to
 * be built with ASan and UBSan; it also aborts if the agent ever writes to the
 * active slot or leaves the slot state unreadable. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "mock_flash.h"
#include "ota_agent.h"

#define MAX_INPUT 65536u

static mock_flash_t g_flash;
static uint8_t g_resp[4096];
static uint32_t g_rng = 1u;

static uint32_t rng(void)
{
    uint32_t x = g_rng;

    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    g_rng = x;
    return x;
}

static void run_one(const uint8_t *data, uint32_t len)
{
    static const uint32_t slots[2] = { MOCK_SLOT_A_BASE, MOCK_SLOT_B_BASE };
    cerberus_flash_t hal;
    ota_agent_t agent;
    nv_state_t st;
    bool found;
    uint32_t pos = 0u;
    uint32_t i;

    mock_init(&g_flash);
    mock_make_hal(&g_flash, &hal);
    (void)memset(g_flash.slot[0], 0xA5, MOCK_SLOT_SIZE);
    if (!ota_agent_init(&agent, &hal, NV_BASE_ADDR, slots, MOCK_SLOT_SIZE)) {
        abort();
    }

    while (pos < len) {
        uint32_t n = 1u + (rng() % 64u);
        ota_feed_result_t r;

        if (n > (len - pos)) {
            n = len - pos;
        }
        r = ota_agent_feed(&agent, &data[pos], n, g_resp, sizeof g_resp);
        if (r.consumed != n) {
            abort(); /* the reply buffer is big enough for 64 input bytes */
        }
        pos += n;
    }

    for (i = 0u; i < MOCK_SLOT_SIZE; i++) {
        if (g_flash.slot[0][i] != 0xA5u) {
            abort(); /* the active slot must never be written */
        }
    }
    if (nv_load(&hal, NV_BASE_ADDR, &st, &found) != NV_OK) {
        abort();
    }
}

static uint32_t mutate(uint8_t *buf, uint32_t len)
{
    uint32_t rounds = 1u + (rng() % 8u);
    uint32_t r;

    for (r = 0u; r < rounds; r++) {
        uint32_t kind = rng() % 5u;
        uint32_t at = (len > 0u) ? (rng() % len) : 0u;

        if ((kind == 0u) && (len > 0u)) {
            buf[at] ^= (uint8_t)(1u << (rng() % 8u));
        } else if ((kind == 1u) && (len > 0u)) {
            buf[at] = (uint8_t)rng();
        } else if ((kind == 2u) && (len < (MAX_INPUT - 1u))) {
            (void)memmove(&buf[at + 1u], &buf[at], len - at);
            buf[at] = (uint8_t)rng();
            len++;
        } else if ((kind == 3u) && (len > 0u)) {
            (void)memmove(&buf[at], &buf[at + 1u], len - at - 1u);
            len--;
        } else if (len > 1u) {
            len = at + 1u; /* truncate */
        } else {
            /* nothing to do */
        }
    }
    return len;
}

int main(int argc, char **argv)
{
    static uint8_t input[MAX_INPUT];
    static uint8_t work[MAX_INPUT];
    uint32_t len;
    uint32_t iterations = 0u;
    uint32_t it;

    if ((argc == 3) && (strcmp(argv[1], "--mutate") == 0)) {
        iterations = (uint32_t)strtoul(argv[2], NULL, 10);
    }
    len = (uint32_t)fread(input, 1u, MAX_INPUT, stdin);

    run_one(input, len);
    for (it = 0u; it < iterations; it++) {
        uint32_t n;

        g_rng = it + 1u;
        (void)memcpy(work, input, len);
        n = mutate(work, len);
        run_one(work, n);
    }
    (void)printf("fuzz: %u bytes of input, %u mutations, no failure\n",
                 (unsigned)len, (unsigned)iterations);
    return 0;
}