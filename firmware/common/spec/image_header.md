# Image Header Specification (HDR_VERSION 0x0001)

Fixed 256-byte header prepended to every BL2 and application image.
The machine-readable source of truth is `image_header.toml`.

## Rules

- All multi-byte integers are **little-endian**.
- MAGIC value is `0x53454342`; the bytes in the image are `42 43 45 53`.
- IMAGE_DIGEST is SHA-256 over the payload (the `PAYLOAD_SIZE` bytes that
  follow the header).
- SIGNATURE is Ed25519 over header bytes `[0x000..0x047]` (72 bytes). This
  range includes NONCE_SALT and IMAGE_DIGEST, so the payload is bound to
  the signature through the digest.
- RESERVED (`0x088..0x0FB`) is not signed. It MUST be all zero, and a
  verifier MUST reject the image otherwise.
- HEADER_CRC32 is CRC-32/ISO-HDLC (zlib) over `[0x000..0x0FB]`. It detects
  corruption only and provides no security.
- Payload starts at header offset 0x100.

## Verifier check order

1. MAGIC matches
2. HDR_VERSION is supported
3. HEADER_CRC32 matches
4. RESERVED is all zero
5. IMAGE_TYPE is the expected type for this slot/stage
6. PAYLOAD_SIZE is between 1 and 0xDFF00 (max_payload)
7. LOAD_ADDRESS and ENTRY_POINT lie inside the permitted region
8. SECURITY_VERSION >= hardware monotonic counter
9. SHA-256 of payload equals IMAGE_DIGEST
10. Ed25519 signature over `[0x000..0x047]` verifies against the RoT key

Any failure means the image is rejected.
