//! BL2 bare-metal entry (RV32IMAC). Hardware access lives here and only here;
//! all decisions are made by the `cerberus-bl2` library.

#![no_std]
#![no_main]

mod vendor_key;

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use cerberus_bl2::boot::Config;
use cerberus_bl2::flash::{FlashError, FlashMemory};
use cerberus_bl2::flow::{boot_flow, Outcome};
use cerberus_bl2::memmap::{NV_STATE, SLOT_A, SLOT_B};
use cerberus_bl2::nv::{NvStore, Slot};
use cerberus_bl2::platform::{hash_from_words, RootOfTrust, SlotStorage};
use cerberus_bl2::verify::CryptoEngine;

// ---- startup -----------------------------------------------------------------

global_asm!(
    r#"
    .section .text.init, "ax"
    .balign 4
    .global _start
_start:
    la   sp, _stack_top

    # zero .bss
    la   t0, _sbss
    la   t1, _ebss
1:
    bgeu t0, t1, 2f
    sw   zero, 0(t0)
    addi t0, t0, 4
    j    1b
2:
    # copy .data from flash to RAM
    la   t0, _sdata
    la   t1, _edata
    la   t2, _sidata
3:
    bgeu t0, t1, 4f
    lw   t3, 0(t2)
    sw   t3, 0(t0)
    addi t0, t0, 4
    addi t2, t2, 4
    j    3b
4:
    call bl2_main
5:
    wfi
    j    5b
"#
);

// ---- RoT registers (ICD-001) ---------------------------------------------------

const ROT_BASE: usize = 0x0002_0000;
const ROT_STATUS: usize = 0x000;
const ROT_HASH0: usize = 0x004;
const ROT_COUNTER: usize = 0x024;

fn rot_read(off: usize) -> u32 {
    // SAFETY: the RoT window is ROT_BASE..ROT_BASE+0x1000 (ICD-001); word access
    unsafe { read_volatile((ROT_BASE + off) as *const u32) }
}

fn rot_write(off: usize, value: u32) {
    // SAFETY: as above
    unsafe { write_volatile((ROT_BASE + off) as *mut u32, value) }
}

struct MmioRot;

impl RootOfTrust for MmioRot {
    fn locked(&self) -> bool {
        rot_read(ROT_STATUS) & 1 != 0
    }

    fn key_hash(&self) -> [u8; 32] {
        let mut words = [0u32; 8];
        for (i, w) in words.iter_mut().enumerate() {
            *w = rot_read(ROT_HASH0 + 4 * i);
        }
        hash_from_words(words)
    }

    fn counter(&self) -> u32 {
        rot_read(ROT_COUNTER)
    }

    fn burn_counter(&mut self, value: u32) {
        rot_write(ROT_COUNTER, value);
    }
}

// ---- application slots (memory-mapped flash, execute in place) -----------------

struct MmioSlots;

impl SlotStorage for MmioSlots {
    fn slot_bytes(&self, slot: Slot) -> &[u8] {
        let r = match slot {
            Slot::A => SLOT_A,
            Slot::B => SLOT_B,
        };
        // SAFETY: the slot is a fixed memory-mapped flash region (ASM-003),
        // always readable and never written while BL2 runs.
        unsafe { core::slice::from_raw_parts(r.start as *const u8, (r.end - r.start) as usize) }
    }
}

// ---- NV state flash ------------------------------------------------------------
// SIMULATION SHORTCUT: the emulated NV flash is plain memory-mapped memory with
// no controller; real flash needs a controller driver here instead

struct MmioFlash;

fn in_nv(addr: u32, len: usize) -> bool {
    addr >= NV_STATE.start && u64::from(addr) + len as u64 <= u64::from(NV_STATE.end)
}

impl FlashMemory for MmioFlash {
    const SECTOR_SIZE: u32 = 4096;

    fn read(&self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
        if !in_nv(addr, buf.len()) {
            return Err(FlashError::OutOfRange);
        }
        for (i, b) in buf.iter_mut().enumerate() {
            // SAFETY: range checked above, inside the NV flash window
            *b = unsafe { read_volatile((addr as usize + i) as *const u8) };
        }
        Ok(())
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), FlashError> {
        if addr % Self::SECTOR_SIZE != 0 || !in_nv(addr, Self::SECTOR_SIZE as usize) {
            return Err(FlashError::OutOfRange);
        }
        for i in 0..Self::SECTOR_SIZE as usize {
            // SAFETY: range checked above, inside the NV flash window
            unsafe { write_volatile((addr as usize + i) as *mut u8, 0xFF) };
        }
        Ok(())
    }

    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError> {
        if !in_nv(addr, data.len()) {
            return Err(FlashError::OutOfRange);
        }
        for (i, &d) in data.iter().enumerate() {
            let p = (addr as usize + i) as *mut u8;
            // SAFETY: range checked above; NOR semantics: bits can only be cleared
            unsafe {
                let old = read_volatile(p);
                write_volatile(p, old & d);
            }
        }
        Ok(())
    }
}

// ---- crypto (STUB) -------------------------------------------------------------

/// FAIL-CLOSED STUB: never accepts a signature, so BL2 cannot boot anything
/// the next step replaces it with a real SHA-256 / Ed25519 engine; never ship
struct FailClosedCrypto;

impl CryptoEngine for FailClosedCrypto {
    fn sha256(&self, _data: &[u8]) -> [u8; 32] {
        [0u8; 32]
    }

    fn ed25519_verify(&self, _pk: &[u8; 32], _msg: &[u8], _sig: &[u8; 64]) -> bool {
        false
    }
}

// ---- boot report ---------------------------------------------------------------

/// written to a fixed symbol so a debugger or Renode can read the result:
/// magic 0xB001_0001, `code` as in `Outcome::code`, slot (0 = A, 1 = B, 0xFF = none), entry
#[repr(C)]
pub struct BootReport {
    magic: u32,
    code: u32,
    slot: u32,
    entry: u32,
}

#[no_mangle]
pub static mut BOOT_REPORT: BootReport = BootReport { magic: 0, code: 0, slot: 0, entry: 0 };

fn write_report(code: u32, slot: u32, entry: u32) {
    // SAFETY: single-threaded bare metal; BOOT_REPORT is only written here
    unsafe {
        write_volatile(
            addr_of_mut!(BOOT_REPORT),
            BootReport { magic: 0xB001_0001, code, slot, entry },
        );
    }
}

// ---- control flow --------------------------------------------------------------

fn halt() -> ! {
    loop {
        // SAFETY: wfi has no memory effects
        unsafe { asm!("wfi") };
    }
}

fn jump(entry: u32) -> ! {
    // SAFETY: `entry` comes from a header that passed full verification and lies
    // inside the verified payload; fence.i makes the instruction stream coherent
    unsafe {
        asm!("fence.i");
        let f: extern "C" fn() -> ! = core::mem::transmute::<usize, extern "C" fn() -> !>(entry as usize);
        f()
    }
}

#[no_mangle]
pub extern "C" fn bl2_main() -> ! {
    let mut rot = MmioRot;
    let slots = MmioSlots;
    let mut nv = NvStore::new(MmioFlash, NV_STATE.start);

    let outcome = boot_flow(
        &mut rot,
        &slots,
        &mut nv,
        &FailClosedCrypto,
        &vendor_key::VENDOR_PUBKEY,
        &Config::default(),
    );

    match outcome {
        Outcome::Jump(t) => {
            write_report(outcome.code(), t.slot as u32, t.entry_point);
            jump(t.entry_point)
        }
        Outcome::Halt(_) => {
            write_report(outcome.code(), 0xFF, 0);
            halt()
        }
    }
}

/// there is no reset peripheral in the platform spec yet, so a panic is a fail-stop
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    halt()
}
