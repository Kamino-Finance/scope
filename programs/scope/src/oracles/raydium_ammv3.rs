use anchor_lang::prelude::*;

use crate::utils::account_deserialize;
use crate::utils::math::sqrt_price_to_price;
use crate::{DatedPrice, Result};
use raydium_amm_v3::states::PoolState;

/// Gives the price of the given token pair in the given pool
pub fn get_price(a_to_b: bool, pool: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    // Load main account
    let pool_data: PoolState = account_deserialize(pool)?;

    // Compute price
    let price = sqrt_price_to_price(
        a_to_b,
        pool_data.sqrt_price_x64,
        pool_data.mint_decimals_0,
        pool_data.mint_decimals_1,
    )?;

    // Return price
    Ok(DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: clock.unix_timestamp as u64,
        ..Default::default()
    })
}

pub fn validate_pool_account(pool: &AccountInfo) -> Result<()> {
    let _: PoolState = account_deserialize(pool)?;
    Ok(())
}
