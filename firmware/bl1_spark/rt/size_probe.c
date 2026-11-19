/* Linked into a throw-away ELF only to measure the ROM cost of the Ed25519
   check. Not part of BL1. */
#include <stdint.h>

uint32_t cerberus_ed25519_check(const uint8_t sig[64], const uint8_t key[32],
                                const uint8_t *message, uint32_t length);

void probe_entry(void)
{
    static uint8_t buffer[64];
    (void)cerberus_ed25519_check(buffer, buffer, buffer, 0);
    for (;;) { }
}