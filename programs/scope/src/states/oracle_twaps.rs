use anchor_lang::prelude::*;
use decimal_wad::decimal::Decimal;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use super::DatedPrice;
use crate::{utils::consts::*, MAX_ENTRIES};

#[derive(Debug, PartialEq, Eq, Clone, Copy, TryFromPrimitive, IntoPrimitive)]
#[repr(usize)]
pub enum EmaType {
    Ema1h,
}

#[zero_copy]
#[derive(Debug, Eq, PartialEq)]
pub struct EmaTwap {
    pub last_update_slot: u64, // the slot when the last observation was added
    pub last_update_unix_timestamp: u64,

    pub current_ema_1h: u128,
    /// The sample tracker is a 64 bit number where each bit represents a point in time.
    pub updates_tracker_1h: u64,
    pub padding_0: u64,

    pub padding_1: [u128; 39],
}

impl Default for EmaTwap {
    fn default() -> Self {
        Self {
            current_ema_1h: 0,
            last_update_slot: 0,
            last_update_unix_timestamp: 0,
            updates_tracker_1h: 0,
            padding_0: 0,
            padding_1: [0_u128; 39],
        }
    }
}

impl EmaTwap {
    pub fn as_dated_price(&self) -> DatedPrice {
        DatedPrice {
            price: Decimal::from_scaled_val(self.current_ema_1h).into(),
            last_updated_slot: self.last_update_slot,
            unix_timestamp: self.last_update_unix_timestamp,
            generic_data: Default::default(),
        }
    }

    pub fn reset(&mut self) {
        self.current_ema_1h = 0;
        self.last_update_slot = 0;
        self.last_update_unix_timestamp = 0;
        self.updates_tracker_1h = 0;
    }
}

static_assertions::const_assert_eq!(ORACLE_TWAPS_SIZE, std::mem::size_of::<OracleTwaps>());
static_assertions::const_assert_eq!(0, std::mem::size_of::<OracleTwaps>() % 8);
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
