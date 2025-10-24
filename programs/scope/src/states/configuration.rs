use anchor_lang::prelude::*;

use crate::utils::consts::CONFIGURATION_SIZE;

static_assertions::const_assert_eq!(CONFIGURATION_SIZE, std::mem::size_of::<Configuration>());
static_assertions::const_assert_eq!(0, std::mem::size_of::<Configuration>() % 8);
// Configuration account of the program
#[account(zero_copy)]
pub struct Configuration {
    pub admin: Pubkey,
    pub oracle_mappings: Pubkey,
    pub oracle_prices: Pubkey,
    pub tokens_metadata: Pubkey,
    pub oracle_twaps: Pubkey,
    pub admin_cached: Pubkey,
    _padding: [u64; 1255],
}
