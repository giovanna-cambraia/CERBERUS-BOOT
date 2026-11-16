//! CERBERUS-BOOT stage-2 bootloader core (no_std).
//! Image format: firmware/common/spec/image_header.md

#![no_std]
#![forbid(unsafe_code)]
#![deny(clippy::all)]

pub mod boot;
#[cfg(feature = "crypto")]
pub mod crypto;
pub mod error;
pub mod flash;
pub mod flow;
pub mod header;
pub mod memmap;
pub mod nv;
pub mod platform;
pub mod verify;
