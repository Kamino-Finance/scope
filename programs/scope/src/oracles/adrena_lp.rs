pub use adrena_perp_itf as adrena;
use anchor_lang::prelude::*;

use crate::{
    utils::{math::estimate_slot_update_from_ts, zero_copy_deserialize},
    warn, DatedPrice, Price, Result, ScopeError,
};

pub const POOL_VALUE_SCALE_DECIMALS: u8 = 6;

pub fn validate_adrena_pool(account: &Option<AccountInfo>) -> Result<()> {
    let Some(account) = account else {
        warn!("No adrena pool account provided");
        return err!(ScopeError::PriceNotValid);
    };
    let _adrena_pool = zero_copy_deserialize::<adrena::state::Pool>(account)?;
    Ok(())
}

/// Get the price of 1 ALP token in USD
///
/// This function get price from Adrena Pool account
pub fn get_price<'a, 'b>(pool_acc: &AccountInfo<'a>, clock: &Clock) -> Result<DatedPrice>
where
    'a: 'b,
{
    // 1. Get accounts
    let pool_pk = pool_acc.key;
    let pool = zero_copy_deserialize::<adrena::state::Pool>(pool_acc)?;

    // 2. Check the price
    Ok(DatedPrice {
        price: Price {
            value: pool.lp_token_price_usd,
            exp: adrena::PRICE_DECIMALS as i8,
        },
        last_updated_slot: estimate_slot_update_from_ts(
            clock,
            pool.last_aum_and_lp_token_price_usd_update,
        ),
        unix_timestamp: u64::try_from(pool.last_aum_and_lp_token_price_usd_update).unwrap(),
        generic_data: [0; 24], // Placeholder for generic data
    })
}
