pub use flashtrade_perp_itf as flashtrade;
use anchor_lang::prelude::*;

use crate::{
    utils::{math::estimate_slot_update_from_ts, account_deserialize},
    warn, DatedPrice, Price, Result, ScopeError,
};

pub fn validate_flashtrade_pool(account: &Option<AccountInfo>, clock: &Clock) -> Result<()> {
    let Some(account) = account else {
        warn!("No flashtrade pool account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };
    let _flp_pool: flashtrade::Pool = account_deserialize(account)?;
    Ok(())
}

/// Get the price of 1 FLP.1 token in USD
///
/// This function gets price from flashtrade Pool account
pub fn get_price(pool_acc: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    // 1. Get accounts
    let flashtrade_pool: flashtrade::Pool = account_deserialize(pool_acc)?;
    
    if flashtrade_pool.inception_time == 0 {
        warn!("flashtrade pool account isn't initialized");
        return err!(ScopeError::PriceNotValid);
    }

    let timestamp: u64 = u64::try_from(flashtrade_pool.last_updated_timestamp)
        .map_err(|_| ScopeError::BadTimestamp)?;
    // 2. Check the price
    Ok(DatedPrice {
        price: Price {
            value: flashtrade_pool.compounding_lp_price,
            exp: flashtrade::USD_DECIMALS.into(),
        },
        last_updated_slot: estimate_slot_update_from_ts(clock, timestamp),
        unix_timestamp: timestamp,
        generic_data: [0; 24], // Placeholder for generic data
    })
}
