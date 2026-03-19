use anchor_lang::prelude::*;

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
