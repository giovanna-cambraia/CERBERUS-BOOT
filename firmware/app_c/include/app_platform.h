#ifndef APP_PLATFORM_H
#define APP_PLATFORM_H

#include <stdbool.h>
#include <stdint.h>

#include "cerberus_flash.h"
#include "cerberus_nv.h"

/* What the application sees of the platform: ASM-003 plus the emulated link. */
#define APP_SLOT_A_BASE   0x40020000u
#define APP_SLOT_B_BASE   0x40100000u
#define APP_SLOT_SIZE     0x000E0000u
#define APP_RX_BASE       0x00040000u  /* update stream in: u32 length, then bytes */
#define APP_TX_BASE       0x00050000u  /* replies out: u32 length, then bytes */
#define APP_MAILBOX_SIZE  0x00010000u
#define APP_REPORT_ADDR   0x20038000u  /* eight words, magic last */
#define APP_REPORT_MAGIC  0xA9910001u

/* Flash: the NV region and both slots, memory mapped. */
const cerberus_flash_t *platform_flash(void);

/* The slot this image runs from, decided by where it is linked. */
nv_slot_t platform_running_slot(void);

/* The mailbox that stands in for a UART. */
uint32_t platform_rx_length(void);
uint8_t platform_rx_byte(uint32_t index);
void platform_tx_append(const uint8_t *data, uint32_t len);

/* Report for the simulator: version, slot, trial boot (0/1), confirm result,
 * update staged (0/1), bytes consumed from the mailbox. */
void platform_report(uint32_t version, uint32_t slot, uint32_t trial,
                     uint32_t confirm, uint32_t staged, uint32_t consumed);

_Noreturn void platform_idle(void);

#endif 