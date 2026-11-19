/* C side of the BL1 Ed25519 binding (see src/bl1-ed25519.ads). */
#include <stddef.h>
#include <stdint.h>

#include "monocypher-ed25519.h"

/* Returns 1 if `sig` is a valid Ed25519 signature of the first `length` bytes
   of `message` under the public key `key`, and 0 otherwise. */
uint32_t cerberus_ed25519_check(const uint8_t sig[64], const uint8_t key[32],
                                const uint8_t *message, uint32_t length)
{
    return crypto_ed25519_check(sig, key, message, (size_t)length) == 0 ? 1u : 0u;
}