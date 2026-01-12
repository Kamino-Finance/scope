pub use adrena_perp_itf as adrena;
use anchor_lang::prelude::*;

use crate::{
    utils::{
        math::{clamp_timestamp_to_now, estimate_slot_update_from_ts},
        zero_copy_deserialize,
    },
    warn, DatedPrice, Price, Result, ScopeError,
};

pub fn validate_adrena_pool(account: Option<&AccountInfo>, clock: &Clock) -> Result<()> {
    let Some(account) = account else {
        warn!("No adrena pool account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };

    require_keys_eq!(*account.owner, adrena::ID, ErrorCode::ConstraintOwner);

    let adrena_pool = zero_copy_deserialize::<adrena::state::Pool>(account)?;

    if adrena_pool.initialized != 1 {
        warn!("Adrena pool account isn't initialized");
        return err!(ScopeError::PriceNotValid);
    }

    msg!(
        "Validated Adrena pool mapping with name {}, current price {} updated {} seconds ago",
        adrena_pool.name,
        adrena_pool.lp_token_price_usd,
        clock
            .unix_timestamp
            .saturating_sub(adrena_pool.last_aum_and_lp_token_price_usd_update)
    );

    Ok(())
}

/// Get the price of 1 ALP token in USD
///
/// This function gets price from Adrena Pool account
pub fn get_price(pool_acc: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    // 1. Get accounts
    let adrena_pool = zero_copy_deserialize::<adrena::state::Pool>(pool_acc)?;

    if adrena_pool.initialized != 1 {
        warn!("Adrena pool account isn't initialized");
        return err!(ScopeError::PriceNotValid);
    }

    // Clamp timestamp to current time to prevent future timestamps
    let pool_timestamp = adrena_pool.last_aum_and_lp_token_price_usd_update;
    let timestamp = clamp_timestamp_to_now(pool_timestamp, clock)?;

    // 2. Check the price
    Ok(DatedPrice {
        price: Price {
            value: adrena_pool.lp_token_price_usd,
            exp: adrena::PRICE_DECIMALS.into(),
        },
        last_updated_slot: estimate_slot_update_from_ts(clock, timestamp),
        unix_timestamp: timestamp,
        generic_data: [0; 24], // Placeholder for generic data
    })
}
