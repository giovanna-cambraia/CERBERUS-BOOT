# OTA transport protocol (version 1)

How an update image travels from the operator's tool to the OTA agent on the
device. The agent is **untrusted**: it only stages bytes. BL2 verifies the staged
image (signature, digest, version) on the next boot, so a hostile or faulty
sender can cause a rejected update, never a booted one.

## Frame

All integers are little-endian.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | SYNC0 | `0xC3` |
| 1 | 1 | SYNC1 | `0x5A` |
| 2 | 1 | TYPE | see below |
| 3 | 1 | FLAGS | must be 0 |
| 4 | 2 | SEQ | chosen by the sender; replies echo it |
| 6 | 2 | LEN | payload length, 0 to 256 |
| 8 | LEN | PAYLOAD | |
| 8 + LEN | 4 | CRC32 | CRC-32/ISO-HDLC over bytes 2 to 8 + LEN - 1 |

A frame is at most 268 bytes. A receiver that sees a bad CRC, a non-zero FLAGS
byte or a LEN above 256 drops the frame and hunts for the next sync pair.

## Messages

| Type | Name | Direction | Payload |
|---|---|---|---|
| `0x01` | BEGIN | sender to agent | u32 total image size (header + payload) |
| `0x02` | DATA | sender to agent | u32 offset, then 1 to 252 image bytes |
| `0x03` | END | sender to agent | u32 CRC-32/ISO-HDLC of the whole image |
| `0x81` | ACK | agent to sender | u32 next expected offset |
| `0x82` | NAK | agent to sender | u8 reason |

## Agent behaviour

1. **BEGIN.** The size must be above 256 and at most the slot size. The agent
   finds the passive slot (the one that is not `active` in the slot-state
   record), refuses if any slot is on trial, marks the passive slot `Empty`
   (`begin_update`), and erases the sectors the image will occupy. It replies
   ACK(0). A second BEGIN restarts the update.
2. **DATA.** Only while receiving. If the offset equals the next expected offset,
   the bytes are written to the passive slot and the reply is ACK(next offset).
   A retransmission of bytes already written is acknowledged without writing. Any
   other offset is refused. A flash error aborts the update.
3. **END.** Only when every byte has arrived and the CRC matches. The agent then
   marks the passive slot `Pending` (`mark_pending`) and replies ACK(total). The
   caller is expected to reset the device, so BL2 verifies the new image.
   A CRC mismatch aborts the update.

The agent never writes outside the passive slot, whatever the frames say.

## NAK reasons

| Code | Name | Meaning |
|---|---|---|
| 1 | BAD_FRAME | the parser dropped a damaged frame |
| 2 | UNKNOWN_TYPE | not a message the agent accepts |
| 3 | BAD_SIZE | image size or chunk does not fit |
| 4 | NOT_ALLOWED | the slot-state rules refuse the operation (for example, an image is on trial) |
| 5 | SEQUENCE | the message does not fit the current state |
| 6 | OUT_OF_ORDER | DATA offset is not the next expected one |
| 7 | FLASH | the flash reported an error |
| 8 | BAD_IMAGE_CRC | END's CRC does not match the received bytes |
| 9 | INCOMPLETE | END arrived before all bytes did |
| 10 | BAD_LENGTH | the payload length is wrong for the message type |

## Security notes

The protocol has no authentication or encryption: integrity of the *image* comes
from BL2's signature check, and the agent's only protection against a hostile
sender is that it cannot cause an unverified boot. A sender can still waste flash
wear or keep the passive slot unusable; that is a denial of service, accepted in
`docs/threat-model/stride.md`.