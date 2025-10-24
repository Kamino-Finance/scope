use anchor_lang::prelude::*;

use super::DatedPrice;
use crate::{utils::consts::*, MAX_ENTRIES};

static_assertions::const_assert_eq!(ORACLE_PRICES_SIZE, std::mem::size_of::<OraclePrices>());
static_assertions::const_assert_eq!(0, std::mem::size_of::<OraclePrices>() % 8);
// Account to store dated prices
#[account(zero_copy)]
pub struct OraclePrices {
    pub oracle_mappings: Pubkey,
    pub prices: [DatedPrice; MAX_ENTRIES],
}

impl OraclePrices {
    pub fn get_price(&self, entry_id: usize) -> Option<DatedPrice> {
        self.prices.get(entry_id).cloned()
    }

    pub fn reset_entry(&mut self, entry_id: usize) {
        self.prices[entry_id] = DatedPrice::default();
    }
}
