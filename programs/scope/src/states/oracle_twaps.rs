use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use strum::{EnumCount, EnumIter, IntoEnumIterator};

use crate::{errors::ScopeError, MAX_ENTRIES};

#[derive(
    Debug, PartialEq, Eq, Clone, Copy, TryFromPrimitive, IntoPrimitive, EnumIter, EnumCount,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(usize)]
pub enum EmaType {
    #[cfg_attr(feature = "serde", serde(rename = "1h"))]
    Ema1h,
    #[cfg_attr(feature = "serde", serde(rename = "8h"))]
    Ema8h,
    #[cfg_attr(feature = "serde", serde(rename = "24h"))]
    Ema24h,
    #[cfg_attr(feature = "serde", serde(rename = "7d"))]
    Ema7d,
}

#[zero_copy]
#[derive(Debug, Eq, PartialEq)]
pub struct EmaTwap {
    pub last_update_slot: u64, // the slot when the last observation was added
    pub last_update_unix_timestamp: u64,

    pub current_ema_1h: u128,
    /// The sample tracker is a 64 bit number where each bit represents a point in time.
    pub updates_tracker_1h: u64,
    pub updates_tracker_7d: u64,

    pub current_ema_8h: u128,
    pub current_ema_24h: u128,
    pub updates_tracker_8h: u64,
    pub updates_tracker_24h: u64,

    pub current_ema_7d: u128,

    pub padding_1: [u128; 35],
}

impl Default for EmaTwap {
    fn default() -> Self {
        Self {
            last_update_slot: 0,
            last_update_unix_timestamp: 0,
            current_ema_1h: 0,
            current_ema_8h: 0,
            current_ema_24h: 0,
            updates_tracker_1h: 0,
            updates_tracker_8h: 0,
            updates_tracker_24h: 0,
            current_ema_7d: 0,
            updates_tracker_7d: 0,
            padding_1: [0_u128; 35],
        }
    }
}

impl EmaTwap {
    pub fn reset(&mut self) {
        self.current_ema_1h = 0;
        self.current_ema_8h = 0;
        self.current_ema_24h = 0;
        self.current_ema_7d = 0;
        self.updates_tracker_1h = 0;
        self.updates_tracker_8h = 0;
        self.updates_tracker_24h = 0;
        self.updates_tracker_7d = 0;
        self.last_update_slot = 0;
        self.last_update_unix_timestamp = 0;
    }
}

// Account to store dated TWAP prices
#[account(zero_copy)]
pub struct OracleTwaps {
    pub oracle_prices: Pubkey,
    pub oracle_mappings: Pubkey,
    pub twaps: [EmaTwap; MAX_ENTRIES],
}

impl OracleTwaps {
    pub fn reset_entry(&mut self, entry_id: usize) {
        self.twaps[entry_id] = EmaTwap::default();
    }
}

#[derive(Debug, Clone, Copy, AnchorSerialize, AnchorDeserialize, Zeroable, Pod, PartialEq)]
#[repr(C)]
pub struct TwapEnabledBitmask {
    bitmask: u8,
}

impl TwapEnabledBitmask {
    pub const fn new() -> Self {
        Self { bitmask: 0 }
    }

    pub fn enable(&self, ema_type: EmaType) -> Self {
        let ema_type: usize = ema_type.into();
        Self {
            bitmask: self.bitmask | (1 << ema_type),
        }
    }

    // Used in tests
    pub const fn new_enable_all() -> Self {
        // Equivalent to:
        //   Self::new()
        //      .enable(EmaType::Ema1h)
        //      .enable(EmaType::Ema8h)
        //      .enable(EmaType::Ema24h)
        //      .enable(EmaType::Ema7d)
        // but need to be able to declare it as const
        // Bits 0, 1, 2, 3 enabled = 0b1111
        Self { bitmask: 0b1111 }
    }

    pub fn is_twap_enabled(&self) -> bool {
        self.bitmask > 0
    }

    pub fn is_twap_enabled_for_ema_type(&self, ema_type: EmaType) -> bool {
        let ema_type: usize = ema_type.into();
        self.bitmask & (1 << ema_type) > 0
    }
}

impl TryFrom<u8> for TwapEnabledBitmask {
    type Error = ScopeError;

    fn try_from(bitmask: u8) -> std::result::Result<Self, Self::Error> {
        if bitmask < (1 << EmaType::COUNT) {
            Ok(Self { bitmask })
        } else {
            Err(ScopeError::TwapEnabledBitmaskConversionFailure)
        }
    }
}

impl From<TwapEnabledBitmask> for u8 {
    fn from(val: TwapEnabledBitmask) -> Self {
        val.bitmask
    }
}

impl From<Vec<EmaType>> for TwapEnabledBitmask {
    fn from(ema_types: Vec<EmaType>) -> Self {
        let bitmask = ema_types.iter().fold(0, |acc, ema_type| {
            let ema_type_usize: usize = (*ema_type).into();
            acc | (1 << ema_type_usize)
        });
        Self { bitmask }
    }
}

impl From<TwapEnabledBitmask> for Vec<EmaType> {
    fn from(val: TwapEnabledBitmask) -> Self {
        let mut res = Vec::with_capacity(val.bitmask.count_ones() as usize);
        EmaType::iter().for_each(|ema_type| {
            let ema_type_usize: usize = ema_type.into();
            if val.bitmask & (1 << ema_type_usize) > 0 {
                res.push(ema_type);
            }
        });
        res
    }
}
