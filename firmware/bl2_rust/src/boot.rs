//! Boot slot selection (the transitions of spec section 4.3).
//!
//! `select_boot` is the only place that changes slot state at boot. The caller
//! (entry code, later) performs the jump and the rollback-counter write.

use crate::error::Error;
use crate::flash::{FlashError, FlashMemory};
use crate::nv::{NvStore, NvState, Slot, SlotStatus};

/// verifies the image in a slot (the pipeline from `verify`, including the
/// rollback gate) and returns its security version
pub trait SlotVerifier {
    fn verify_slot(&mut self, slot: Slot) -> Result<u32, Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// boots a new image gets to be confirmed (minimum 1)
    pub trial_boots: u8,
}

impl Default for Config {
    fn default() -> Self {
        Config { trial_boots: 3 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    /// confirmed image
    Normal,
    /// unconfirmed image on trial; the application must call `confirm`
    Trial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootPlan {
    pub slot: Slot,
    pub mode: BootMode,
    /// burn the hardware monotonic counter forward to at least this value
    /// (a no-op if it is already there); only set for `Normal` boots
    pub counter_target: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltReason {
    /// nothing verifiable to boot; fail-stop or recovery is the caller's
    /// decision (open question Q3)
    NoBootableImage,
    /// the slot state could not be read or saved safely
    Storage(FlashError),
}

impl From<FlashError> for HaltReason {
    fn from(e: FlashError) -> Self {
        HaltReason::Storage(e)
    }
}

pub fn select_boot<F: FlashMemory, V: SlotVerifier>(
    store: &mut NvStore<F>,
    verifier: &mut V,
    cfg: &Config,
) -> Result<BootPlan, HaltReason> {
    let mut st = store.load()?.unwrap_or_else(NvState::factory);

    // 1. a slot already on trial
    if let Some(slot) = st.find(SlotStatus::Testing) {
        if st.trials_left > 0 && verifier.verify_slot(slot).is_ok() {
            st.trials_left -= 1;
            store.commit(st)?;
            return Ok(BootPlan { slot, mode: BootMode::Trial, counter_target: None });
        }
        // trials used up (FALLBACK) or the image no longer verifies
        let status = if st.trials_left == 0 { SlotStatus::Broken } else { SlotStatus::Invalid };
        st.set_status(slot, status);
        st.trials_left = 0;
        st = store.commit(st)?;
    }

    // 2. a freshly staged image (EVALUATE)
    if let Some(slot) = st.find(SlotStatus::Pending) {
        match verifier.verify_slot(slot) {
            Ok(_) => {
                st.set_status(slot, SlotStatus::Testing);
                // this boot uses one of the trials
                st.trials_left = cfg.trial_boots.max(1) - 1;
                store.commit(st)?;
                return Ok(BootPlan { slot, mode: BootMode::Trial, counter_target: None });
            }
            Err(_) => {
                st.set_status(slot, SlotStatus::Invalid);
                st = store.commit(st)?;
            }
        }
    }

    // 3. a confirmed image, the active slot first
    for slot in [st.active, st.active.other()] {
        if st.status(slot) != SlotStatus::Confirmed {
            continue;
        }
        match verifier.verify_slot(slot) {
            Ok(secver) => {
                if slot != st.active {
                    // keep "active" equal to the slot that is running, so the
                    // OTA agent never writes into it
                    st.active = slot;
                    store.commit(st)?;
                }
                return Ok(BootPlan {
                    slot,
                    mode: BootMode::Normal,
                    counter_target: Some(secver),
                });
            }
            Err(_) => {
                st.set_status(slot, SlotStatus::Invalid);
                st = store.commit(st)?;
            }
        }
    }

    Err(HaltReason::NoBootableImage)
}
