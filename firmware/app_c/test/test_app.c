/* Host tests for the slot-state writer, the frame parser and the OTA agent.
 *   test_app [<frames file> <image file>]
 * With the two files (written by `cerberus-cli ota-frames`), the Rust frame
 * encoder is checked against the C one, and the agent is fed the Rust stream. */
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "cerberus_crc32.h"
#include "cerberus_le.h"
#include "cerberus_nv.h"
#include "golden_nv.h"
#include "mock_flash.h"
#include "ota_agent.h"
#include "ota_parser.h"

static int g_checks = 0;
static int g_failures = 0;

#define CHECK(cond, name)                                              \
    do {                                                               \
        g_checks++;                                                    \
        if (cond) {                                                    \
            (void)printf("  [ok]   %s\n", (name));                     \
        } else {                                                       \
            g_failures++;                                              \
            (void)printf("  [FAIL] %s (line %d)\n", (name), __LINE__); \
        }                                                              \
    } while (0)

static mock_flash_t g_flash; /* static: it is large */

static void hal_fresh(cerberus_flash_t *hal)
{
    mock_init(&g_flash);
    mock_make_hal(&g_flash, hal);
}

static bool same_state(const nv_state_t *a, const nv_state_t *b)
{
    return (a->seq == b->seq) && (a->active == b->active) && (a->status[0] == b->status[0])
           && (a->status[1] == b->status[1]) && (a->trials_left == b->trials_left);
}

/* ---- slot-state record ------------------------------------------------------ */

static void test_golden_record(void)
{
    nv_state_t ex = { 5u, NV_SLOT_B, { NV_STATUS_CONFIRMED, NV_STATUS_TESTING }, 2u };
    nv_state_t back;
    uint8_t rec[NV_RECORD_SIZE];

    nv_encode(&ex, rec);
    CHECK(memcmp(rec, GOLDEN_NV_RECORD, NV_RECORD_SIZE) == 0,
          "encoding matches the golden record generated from the spec");
    CHECK(nv_decode(GOLDEN_NV_RECORD, &back) && same_state(&back, &ex),
          "decoding the golden record gives the state");
}

static void test_corruption_is_rejected(void)
{
    uint8_t rec[NV_RECORD_SIZE];
    nv_state_t s;
    uint32_t bit;
    uint32_t accepted = 0u;

    (void)memcpy(rec, GOLDEN_NV_RECORD, NV_RECORD_SIZE);
    for (bit = 0u; bit < (NV_RECORD_SIZE * 8u); bit++) {
        rec[bit / 8u] ^= (uint8_t)(1u << (bit % 8u));
        if (nv_decode(rec, &s)) {
            accepted++;
        }
        rec[bit / 8u] ^= (uint8_t)(1u << (bit % 8u));
    }
    CHECK(accepted == 0u, "every single-bit corruption of a record is rejected");
}

static void test_commit_alternates_and_wraps(void)
{
    cerberus_flash_t hal;
    nv_state_t st = nv_factory_state();
    nv_state_t got;
    nv_state_t next;
    bool found = true;
    uint8_t rec[NV_RECORD_SIZE];

    hal_fresh(&hal);
    CHECK((nv_load(&hal, NV_BASE_ADDR, &got, &found) == NV_OK) && !found,
          "a blank flash has no record");
    CHECK(same_state(&got, &st), "... and loads as the factory state");

    CHECK((nv_commit(&hal, NV_BASE_ADDR, &st) == NV_OK) && (st.seq == 1u),
          "the first commit is sequence 1");
    CHECK(le_get32(g_flash.nv, 0u) == NV_MAGIC, "... and goes to sector 0");
    CHECK(g_flash.nv[MOCK_SECTOR] == 0xFFu, "... leaving sector 1 blank");
    CHECK((nv_commit(&hal, NV_BASE_ADDR, &st) == NV_OK) && (st.seq == 2u),
          "the second commit is sequence 2");
    CHECK(le_get32(g_flash.nv, MOCK_SECTOR) == NV_MAGIC, "... and goes to sector 1");

    /* Sequence numbers wrap: after 0xFFFFFFFF comes 0, and 0 is newer. */
    hal_fresh(&hal);
    next = nv_factory_state();
    next.seq = 0xFFFFFFFFu;
    nv_encode(&next, rec);
    (void)memcpy(g_flash.nv, rec, NV_RECORD_SIZE);
    next.trials_left = 1u;
    CHECK((nv_commit(&hal, NV_BASE_ADDR, &next) == NV_OK) && (next.seq == 0u),
          "the sequence number wraps to 0");
    CHECK((nv_load(&hal, NV_BASE_ADDR, &got, &found) == NV_OK) && found && (got.seq == 0u)
              && (got.trials_left == 1u),
          "... and the wrapped record is the newest");
}

static void test_power_loss_during_commit(void)
{
    cerberus_flash_t hal;
    nv_state_t last = nv_factory_state();
    uint8_t saved[2u * MOCK_SECTOR];
    uint32_t round;
    uint32_t budget;
    uint32_t failures = 0u;

    hal_fresh(&hal);
    (void)nv_commit(&hal, NV_BASE_ADDR, &last); /* sequence 1, sector 0 */

    /* Three rounds, so both sectors are the write target. A power cut at every
     * byte of a commit must leave either the old or the new record readable. */
    for (round = 0u; round < 3u; round++) {
        nv_state_t desired = last;
        nv_state_t expected_new;

        desired.trials_left = (uint8_t)(round + 1u);
        expected_new = desired;
        expected_new.seq = last.seq + 1u;
        (void)memcpy(saved, g_flash.nv, sizeof saved);

        for (budget = 0u; budget <= (MOCK_SECTOR + NV_RECORD_SIZE + 2u); budget++) {
            nv_state_t attempt = desired;
            nv_state_t after;
            bool found = false;

            (void)memcpy(g_flash.nv, saved, sizeof saved);
            g_flash.budget = (int64_t)budget;
            (void)nv_commit(&hal, NV_BASE_ADDR, &attempt);
            g_flash.budget = -1;

            if ((nv_load(&hal, NV_BASE_ADDR, &after, &found) != NV_OK) || !found
                || !(same_state(&after, &last) || same_state(&after, &expected_new))) {
                failures++;
            }
        }

        (void)memcpy(g_flash.nv, saved, sizeof saved);
        g_flash.budget = -1;
        last = desired;
        (void)nv_commit(&hal, NV_BASE_ADDR, &last);
    }
    CHECK(failures == 0u, "a power cut at any byte of a commit never loses the slot state");
}

static void test_update_guards(void)
{
    cerberus_flash_t hal;
    nv_state_t st;
    bool found;

    hal_fresh(&hal);
    CHECK(nv_begin_update(&hal, NV_BASE_ADDR, NV_SLOT_A) == NV_ERR_NOT_ALLOWED,
          "the active slot cannot be rewritten");
    CHECK(nv_mark_pending(&hal, NV_BASE_ADDR, NV_SLOT_A) == NV_ERR_NOT_ALLOWED,
          "the active slot cannot be marked pending");
    CHECK(nv_confirm(&hal, NV_BASE_ADDR, NV_SLOT_B) == NV_ERR_NOT_ALLOWED,
          "only a slot in Testing can be confirmed");

    CHECK(nv_mark_pending(&hal, NV_BASE_ADDR, NV_SLOT_B) == NV_OK,
          "a never-used passive slot can be marked pending");

    /* BL2 moves it to Testing when it boots it on trial. */
    (void)nv_load(&hal, NV_BASE_ADDR, &st, &found);
    st.status[1] = NV_STATUS_TESTING;
    st.trials_left = 2u;
    (void)nv_commit(&hal, NV_BASE_ADDR, &st);
    CHECK(nv_begin_update(&hal, NV_BASE_ADDR, NV_SLOT_B) == NV_ERR_NOT_ALLOWED,
          "no update while an image is on trial");

    CHECK(nv_confirm(&hal, NV_BASE_ADDR, NV_SLOT_B) == NV_OK, "the trial image is confirmed");
    (void)nv_load(&hal, NV_BASE_ADDR, &st, &found);
    CHECK((st.active == NV_SLOT_B) && (st.status[1] == NV_STATUS_CONFIRMED)
              && (st.trials_left == 0u),
          "... B becomes active and confirmed");

    CHECK(nv_mark_pending(&hal, NV_BASE_ADDR, NV_SLOT_A) == NV_ERR_NOT_ALLOWED,
          "a used passive slot must be emptied before it is marked pending");
    CHECK((nv_begin_update(&hal, NV_BASE_ADDR, NV_SLOT_A) == NV_OK)
              && (nv_mark_pending(&hal, NV_BASE_ADDR, NV_SLOT_A) == NV_OK),
          "... and then it can");
}

/* ---- frame parser ------------------------------------------------------------ */

static void count_events(const uint8_t *d, uint32_t n, uint32_t *frames, uint32_t *errors,
                         ota_parser_t *p)
{
    uint32_t i;

    *frames = 0u;
    *errors = 0u;
    for (i = 0u; i < n; i++) {
        ota_event_t ev = ota_parser_push(p, d[i]);

        if (ev == OTA_EVT_FRAME) {
            (*frames)++;
        } else if (ev == OTA_EVT_ERROR) {
            (*errors)++;
        } else {
            /* nothing */
        }
    }
}

static void test_parser(void)
{
    static const uint8_t payload[3] = { 1u, 2u, 3u };
    uint8_t good[OTA_MAX_FRAME];
    uint8_t stream[2u * OTA_MAX_FRAME];
    ota_parser_t p;
    uint32_t n;
    uint32_t frames;
    uint32_t errors;
    uint32_t i;

    n = ota_encode_frame(OTA_TYPE_DATA, 7u, payload, 3u, good, sizeof good);
    CHECK(n == (3u + OTA_FRAME_OVERHEAD), "a 3-byte frame is 15 bytes long");

    ota_parser_init(&p);
    count_events(good, n, &frames, &errors, &p);
    CHECK((frames == 1u) && (errors == 0u) && (p.frame.type == OTA_TYPE_DATA)
              && (p.frame.seq == 7u) && (p.frame.len == 3u) && (p.frame.payload[2] == 3u),
          "a valid frame is parsed");

    /* Garbage, a lone sync byte and a half sync pair before the frame. */
    stream[0] = 0x00u;
    stream[1] = OTA_SYNC0;
    stream[2] = OTA_SYNC0;
    stream[3] = 0x11u;
    (void)memcpy(&stream[4], good, n);
    ota_parser_init(&p);
    count_events(stream, 4u + n, &frames, &errors, &p);
    CHECK((frames == 1u) && (errors == 0u), "the parser finds a frame after garbage");

    /* A damaged frame is dropped, and the next one is still found. */
    (void)memcpy(stream, good, n);
    stream[9] ^= 0x01u;
    (void)memcpy(&stream[n], good, n);
    ota_parser_init(&p);
    count_events(stream, 2u * n, &frames, &errors, &p);
    CHECK((frames == 1u) && (errors == 1u), "a bad CRC drops one frame and the parser recovers");

    /* LEN above the maximum, and a non-zero FLAGS byte, are rejected at the header. */
    (void)memcpy(stream, good, n);
    le_put16(stream, 6u, 257u);
    ota_parser_init(&p);
    count_events(stream, n, &frames, &errors, &p);
    CHECK((frames == 0u) && (errors >= 1u), "LEN above 256 is rejected");

    (void)memcpy(stream, good, n);
    stream[3] = 1u;
    ota_parser_init(&p);
    count_events(stream, n, &frames, &errors, &p);
    CHECK((frames == 0u) && (errors >= 1u), "a non-zero FLAGS byte is rejected");

    CHECK(ota_encode_frame(OTA_TYPE_DATA, 0u, payload, 3u, good, 14u) == 0u,
          "encoding into a buffer that is too small fails");

    /* Every possible single-byte corruption of a frame is either dropped or
     * (never) accepted as the same frame. */
    n = ota_encode_frame(OTA_TYPE_DATA, 7u, payload, 3u, good, sizeof good);
    errors = 0u;
    for (i = 2u; i < n; i++) {
        uint8_t damaged[OTA_MAX_FRAME];
        uint32_t f;
        uint32_t e;

        (void)memcpy(damaged, good, n);
        damaged[i] ^= 0x01u;
        ota_parser_init(&p);
        count_events(damaged, n, &f, &e, &p);
        if (f != 0u) {
            errors++;
        }
    }
    CHECK(errors == 0u, "no single-bit corruption of a frame is accepted");
}

/* ---- OTA agent ------------------------------------------------------------------ */

typedef struct {
    uint8_t type;
    uint32_t value;
    bool ok;
} reply_t;

static reply_t send_frame(ota_agent_t *a, uint8_t type, uint16_t seq, const uint8_t *pl,
                          uint16_t len, bool *staged)
{
    uint8_t frame[OTA_MAX_FRAME];
    uint8_t resp[64];
    ota_parser_t p;
    ota_feed_result_t r;
    reply_t out;
    uint32_t n;
    uint32_t i;

    out.type = 0u;
    out.value = 0u;
    out.ok = false;

    n = ota_encode_frame(type, seq, pl, len, frame, sizeof frame);
    r = ota_agent_feed(a, frame, n, resp, sizeof resp);
    if ((staged != NULL) && r.staged) {
        *staged = true;
    }
    ota_parser_init(&p);
    for (i = 0u; i < r.resp_len; i++) {
        if (ota_parser_push(&p, resp[i]) == OTA_EVT_FRAME) {
            out.type = p.frame.type;
            out.value = (p.frame.len >= 4u) ? le_get32(p.frame.payload, 0u) : p.frame.payload[0];
            out.ok = true;
            break;
        }
    }
    return out;
}

static void make_image(uint8_t *img, uint32_t n)
{
    uint32_t i;

    for (i = 0u; i < n; i++) {
        img[i] = (uint8_t)((i * 31u + 7u) & 0xFFu);
    }
}

static reply_t send_begin(ota_agent_t *a, uint32_t total, uint16_t seq)
{
    uint8_t pl[4];

    le_put32(pl, 0u, total);
    return send_frame(a, OTA_TYPE_BEGIN, seq, pl, 4u, NULL);
}

static reply_t send_data(ota_agent_t *a, uint32_t off, const uint8_t *d, uint32_t n, uint16_t seq)
{
    uint8_t pl[OTA_MAX_PAYLOAD];

    le_put32(pl, 0u, off);
    (void)memcpy(&pl[4], d, n);
    return send_frame(a, OTA_TYPE_DATA, seq, pl, (uint16_t)(n + 4u), NULL);
}

static reply_t send_end(ota_agent_t *a, uint32_t crc, uint16_t seq, bool *staged)
{
    uint8_t pl[4];

    le_put32(pl, 0u, crc);
    return send_frame(a, OTA_TYPE_END, seq, pl, 4u, staged);
}

static reply_t send_image(ota_agent_t *a, const uint8_t *img, uint32_t n, bool *staged)
{
    uint32_t off = 0u;
    uint16_t seq = 1u;
    reply_t r = send_begin(a, n, 0u);

    while ((r.type == OTA_TYPE_ACK) && (off < n)) {
        uint32_t c = n - off;

        if (c > OTA_DATA_CHUNK) {
            c = OTA_DATA_CHUNK;
        }
        r = send_data(a, off, &img[off], c, seq);
        seq++;
        off += c;
    }
    if (r.type != OTA_TYPE_ACK) {
        return r;
    }
    return send_end(a, crc32_final(crc32_update(CRC32_INIT, img, n)), seq, staged);
}

static bool slot_is(uint32_t slot, uint8_t value)
{
    uint32_t i;

    for (i = 0u; i < MOCK_SLOT_SIZE; i++) {
        if (g_flash.slot[slot][i] != value) {
            return false;
        }
    }
    return true;
}

static const uint32_t SLOTS[2] = { MOCK_SLOT_A_BASE, MOCK_SLOT_B_BASE };

static void agent_fresh(ota_agent_t *a, cerberus_flash_t *hal)
{
    hal_fresh(hal);
    (void)memset(g_flash.slot[0], 0xA5, MOCK_SLOT_SIZE); /* the active slot: must stay as it is */
    (void)ota_agent_init(a, hal, NV_BASE_ADDR, SLOTS, MOCK_SLOT_SIZE);
}

static void test_agent_stages_an_image(void)
{
    uint8_t img[700];
    cerberus_flash_t hal;
    ota_agent_t a;
    nv_state_t st;
    bool found;
    bool staged = false;
    reply_t r;

    make_image(img, sizeof img);
    hal_fresh(&hal);
    CHECK(ota_agent_init(&a, &hal, NV_BASE_ADDR, SLOTS, MOCK_SLOT_SIZE), "the agent initialises");
    (void)memset(g_flash.slot[0], 0xA5, MOCK_SLOT_SIZE);

    r = send_image(&a, img, (uint32_t)sizeof img, &staged);
    CHECK((r.type == OTA_TYPE_ACK) && (r.value == sizeof img),
          "END is acknowledged with the total size");
    CHECK(staged, "the agent reports a staged update");
    CHECK(memcmp(g_flash.slot[1], img, sizeof img) == 0, "the image is in the passive slot");
    CHECK((nv_load(&hal, NV_BASE_ADDR, &st, &found) == NV_OK) && found
              && (st.status[1] == NV_STATUS_PENDING) && (st.active == NV_SLOT_A),
          "slot B is pending and A is still active");
    CHECK(slot_is(0u, 0xA5u), "the active slot was never written");
}

static void test_agent_refusals(void)
{
    uint8_t img[700];
    uint8_t chunk[16];
    cerberus_flash_t hal;
    ota_agent_t a;
    nv_state_t st;
    bool found;
    bool staged = false;
    reply_t r;
    uint32_t crc;

    make_image(img, sizeof img);
    make_image(chunk, sizeof chunk);
    crc = crc32_final(crc32_update(CRC32_INIT, img, (uint32_t)sizeof img));

    agent_fresh(&a, &hal);
    r = send_data(&a, 0u, chunk, 16u, 1u);
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_SEQUENCE), "DATA before BEGIN is refused");

    r = send_begin(&a, 256u, 0u);
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_BAD_SIZE),
          "an image no larger than its header is refused");
    r = send_begin(&a, MOCK_SLOT_SIZE + 1u, 0u);
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_BAD_SIZE),
          "an image larger than the slot is refused");

    r = send_frame(&a, (uint8_t)0x55u, 0u, NULL, 0u, NULL);
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_UNKNOWN_TYPE), "an unknown type is refused");

    r = send_begin(&a, (uint32_t)sizeof img, 0u);
    CHECK(r.type == OTA_TYPE_ACK, "BEGIN is accepted");
    r = send_data(&a, 100u, chunk, 16u, 1u);
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_OUT_OF_ORDER),
          "DATA with a gap is refused");
    r = send_data(&a, 0u, chunk, 16u, 2u);
    CHECK((r.type == OTA_TYPE_ACK) && (r.value == 16u), "DATA at the expected offset is accepted");

    (void)memset(g_flash.slot[1], 0x00, 16u);
    r = send_data(&a, 0u, chunk, 16u, 3u);
    CHECK((r.type == OTA_TYPE_ACK) && (r.value == 16u), "a retransmission is acknowledged again");
    CHECK(slot_is(0u, 0xA5u), "... without writing anything outside the passive slot");

    r = send_end(&a, crc, 4u, &staged);
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_INCOMPLETE) && !staged,
          "END before all bytes arrived is refused");

    /* A complete image with a wrong CRC is refused and never marked pending. */
    agent_fresh(&a, &hal);
    r = send_image(&a, img, (uint32_t)sizeof img, &staged);
    (void)r;
    agent_fresh(&a, &hal);
    staged = false;
    (void)send_begin(&a, (uint32_t)sizeof img, 0u);
    {
        uint32_t off = 0u;
        uint16_t seq = 1u;

        while (off < sizeof img) {
            uint32_t c = (uint32_t)sizeof img - off;

            if (c > OTA_DATA_CHUNK) {
                c = OTA_DATA_CHUNK;
            }
            (void)send_data(&a, off, &img[off], c, seq);
            seq++;
            off += c;
        }
        r = send_end(&a, crc ^ 1u, seq, &staged);
    }
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_BAD_IMAGE_CRC) && !staged,
          "a wrong image CRC is refused");
    CHECK((nv_load(&hal, NV_BASE_ADDR, &st, &found) == NV_OK) && (st.status[1] == NV_STATUS_EMPTY),
          "... and the slot is not marked pending");

    /* An image is on trial: no new update may start. */
    agent_fresh(&a, &hal);
    (void)nv_mark_pending(&hal, NV_BASE_ADDR, NV_SLOT_B);
    (void)nv_load(&hal, NV_BASE_ADDR, &st, &found);
    st.status[1] = NV_STATUS_TESTING;
    (void)nv_commit(&hal, NV_BASE_ADDR, &st);
    r = send_begin(&a, (uint32_t)sizeof img, 0u);
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_NOT_ALLOWED),
          "BEGIN while an image is on trial is refused");

    /* The flash fails in the middle of an update. */
    agent_fresh(&a, &hal);
    (void)send_begin(&a, (uint32_t)sizeof img, 0u);
    g_flash.budget = 3;
    r = send_data(&a, 0u, chunk, 16u, 1u);
    g_flash.budget = -1;
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_FLASH), "a flash error is reported");
    r = send_end(&a, crc, 2u, NULL);
    CHECK((r.type == OTA_TYPE_NAK) && (r.value == OTA_NAK_SEQUENCE),
          "... and aborts the update");
}

/* ---- cross-check with the Rust frame encoder ---------------------------------------- */

static bool read_file(const char *path, uint8_t *buf, uint32_t cap, uint32_t *len)
{
    FILE *f = fopen(path, "rb");
    size_t n;

    if (f == NULL) {
        return false;
    }
    n = fread(buf, 1u, cap, f);
    (void)fclose(f);
    *len = (uint32_t)n;
    return n > 0u;
}

/* The stream a sender produces for an image (the C side of the protocol). */
static uint32_t build_stream(const uint8_t *img, uint32_t n, uint8_t *out, uint32_t cap)
{
    uint8_t pl[OTA_MAX_PAYLOAD];
    uint32_t used = 0u;
    uint32_t off = 0u;
    uint16_t seq = 0u;

    le_put32(pl, 0u, n);
    used += ota_encode_frame(OTA_TYPE_BEGIN, seq, pl, 4u, &out[used], cap - used);
    seq++;
    while (off < n) {
        uint32_t c = n - off;

        if (c > OTA_DATA_CHUNK) {
            c = OTA_DATA_CHUNK;
        }
        le_put32(pl, 0u, off);
        (void)memcpy(&pl[4], &img[off], c);
        used += ota_encode_frame(OTA_TYPE_DATA, seq, pl, (uint16_t)(c + 4u), &out[used], cap - used);
        seq++;
        off += c;
    }
    le_put32(pl, 0u, crc32_final(crc32_update(CRC32_INIT, img, n)));
    used += ota_encode_frame(OTA_TYPE_END, seq, pl, 4u, &out[used], cap - used);
    return used;
}

static void test_rust_frames(const char *frames_path, const char *image_path)
{
    static uint8_t frames[1u << 16];
    static uint8_t image[1u << 15];
    static uint8_t mine[1u << 16];
    static uint8_t resp[1u << 12];
    uint32_t frames_len = 0u;
    uint32_t image_len = 0u;
    uint32_t mine_len;
    cerberus_flash_t hal;
    ota_agent_t a;
    ota_feed_result_t r;
    nv_state_t st;
    bool found;

    if (!read_file(frames_path, frames, sizeof frames, &frames_len)
        || !read_file(image_path, image, sizeof image, &image_len)) {
        CHECK(false, "the Rust-generated frames and the image can be read");
        return;
    }

    mine_len = build_stream(image, image_len, mine, sizeof mine);
    CHECK((mine_len == frames_len) && (memcmp(mine, frames, frames_len) == 0),
          "the C and Rust frame encoders agree byte for byte");

    agent_fresh(&a, &hal);
    r = ota_agent_feed(&a, frames, frames_len, resp, sizeof resp);
    CHECK((r.consumed == frames_len) && r.staged, "the agent stages the image from the Rust stream");
    CHECK(memcmp(g_flash.slot[1], image, image_len) == 0, "... and the passive slot holds the image");
    CHECK((nv_load(&hal, NV_BASE_ADDR, &st, &found) == NV_OK) && (st.status[1] == NV_STATUS_PENDING),
          "... and it is marked pending");
    CHECK(slot_is(0u, 0xA5u), "... without touching the active slot");

    /* The same stream with one payload byte damaged: nothing may be staged. */
    agent_fresh(&a, &hal);
    frames[30] ^= 0x01u;
    r = ota_agent_feed(&a, frames, frames_len, resp, sizeof resp);
    CHECK(!r.staged, "a damaged stream is not staged");
    CHECK((nv_load(&hal, NV_BASE_ADDR, &st, &found) == NV_OK) && (st.status[1] != NV_STATUS_PENDING),
          "... and the slot is not marked pending");
    CHECK(slot_is(0u, 0xA5u), "... and the active slot is untouched");
}

int main(int argc, char **argv)
{
    (void)printf("slot-state record:\n");
    test_golden_record();
    test_corruption_is_rejected();
    test_commit_alternates_and_wraps();
    test_power_loss_during_commit();
    test_update_guards();

    (void)printf("frame parser:\n");
    test_parser();

    (void)printf("OTA agent:\n");
    test_agent_stages_an_image();
    test_agent_refusals();

    if (argc == 3) {
        (void)printf("cross-check with the Rust encoder:\n");
        test_rust_frames(argv[1], argv[2]);
    } else {
        (void)printf("(no frames/image arguments: the Rust cross-check is skipped)\n");
    }

    (void)printf("%d checks, %d failures\n", g_checks, g_failures);
    return (g_failures == 0) ? 0 : 1;
}