use anchor_lang::prelude::*;
use raydium_amm_v3::states::PoolState;

use crate::{
    utils::{account_deserialize, math::sqrt_price_to_price},
    warn, DatedPrice, Result, ScopeError,
};

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
    )
    .map_err(|e| {
        warn!("Error while computing the price of the tokens in the pool: {e:?}",);
        e
    })?;

    // Return price
    Ok(DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: clock.unix_timestamp as u64,
        ..Default::default()
    })
}

pub fn validate_pool_account(pool: &Option<AccountInfo>) -> Result<()> {
    let Some(pool) = pool else {
        warn!("No pool account provided");
        return err!(ScopeError::PriceNotValid);
    };
    let _: PoolState = account_deserialize(pool)?;
    Ok(())
}
