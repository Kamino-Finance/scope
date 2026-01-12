use anchor_lang::prelude::*;
pub use flashtrade_perp_itf as flashtrade;

use crate::{
    utils::{
        account_deserialize,
        math::{clamp_timestamp_to_now, estimate_slot_update_from_ts},
    },
    warn, DatedPrice, Price, Result, ScopeError,
};

pub fn validate_flashtrade_pool(account: Option<&AccountInfo>, clock: &Clock) -> Result<()> {
    let Some(account) = account else {
        warn!("No flashtrade pool account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };

    require_keys_eq!(*account.owner, flashtrade::ID, ErrorCode::ConstraintOwner);

    let flp_pool: flashtrade::Pool = account_deserialize(account)?;
    if flp_pool.inception_time == 0 {
        warn!("flashtrade pool account is not initialized");
        return err!(ScopeError::PriceNotValid);
    }

    msg!(
        "Validated flashtrade pool mapping with name {}, current price {} updated {} seconds ago",
        flp_pool.name,
        flp_pool.compounding_lp_price,
        clock
            .unix_timestamp
            .saturating_sub(flp_pool.last_updated_timestamp)
    );

    Ok(())
}

/// Get the price of 1 FLP.1 token in USD
///
/// This function gets price from flashtrade Pool account
pub fn get_price(pool_acc: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    // 1. Get accounts
    let flashtrade_pool: flashtrade::Pool = account_deserialize(pool_acc)?;

    if flashtrade_pool.inception_time == 0 {
        warn!("flashtrade pool account is not initialized");
        return err!(ScopeError::PriceNotValid);
    }

    // Ideally we would have `last_updated_timestamp` be based on the timestamps of the source oracles
    // the price is computed from, but instead it is the time at which `compounding_lp_price` was computed
    // When defining a max_age for this price type, we need to take into account the max_age for these
    // source oracles (which is 10s for all assets except USDC for which it is 100s)

    // Clamp timestamp to current time to prevent future timestamps
    let pool_timestamp = flashtrade_pool.last_updated_timestamp;
    let unix_timestamp = clamp_timestamp_to_now(pool_timestamp, clock)?;

    // 2. Check the price
    Ok(DatedPrice {
        price: Price {
            value: flashtrade_pool.compounding_lp_price,
            exp: flashtrade::USD_DECIMALS.into(),
        },
        last_updated_slot: estimate_slot_update_from_ts(clock, unix_timestamp),
        unix_timestamp,
        generic_data: [0; 24], // Placeholder for generic data
    })
}
