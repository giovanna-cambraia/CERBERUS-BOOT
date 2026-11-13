//! CERBERUS-BOOT stage-2 bootloader core (no_std).
//! Image format: firmware/common/spec/image_header.md

#![no_std]
#![forbid(unsafe_code)]
#![deny(clippy::all)]

pub mod boot;
pub mod error;
pub mod flash;
pub mod header;
pub mod memmap;
pub mod nv;
pub mod verify;
