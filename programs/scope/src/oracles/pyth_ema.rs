//! Toolings to retrieve pyth ema prices and validate them
//!
//! Validation partially follows [pyth best practices](https://docs.pyth.network/consumers/best-practices)
//!
//! 1. Some checks in [`validate_pyth_price`] are performed on the pyth ema price account upon registration in
//!    the oracle mapping. However some information present only in the associated pyth product account are
//!    expected to be checked by the admin to ensure the product has the expected quality prior the mapping
//!    update.
//! 2. Upon usage the current ema price state is checked in [`validate_valid_price`]
//! 3. The confidence interval is also checked in this same function with [`ORACLE_CONFIDENCE_FACTOR`]

use std::convert::{TryFrom, TryInto};

use anchor_lang::prelude::*;
use pyth_sdk_solana::state as pyth_client;

use crate::{DatedPrice, Price, Result, ScopeError};

/// validate price confidence - confidence/price ratio should be less than 2%
const ORACLE_CONFIDENCE_FACTOR: u64 = 50; // 100% / 2%

/// Only update with prices not older than 10 minutes, users can still check actual price age
const STALENESS_THRESHOLD: u64 = 10 * 60; // 10 minutes

pub fn get_price(price_info: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    let data = price_info.try_borrow_data()?;
    let price_account: &pyth_client::SolanaPriceAccount =
        pyth_client::load_price_account(data.as_ref()).map_err(|_| {
            msg!("Loading pyth price account failed {}", price_info.key);
            ScopeError::PriceNotValid
        })?;

    let pyth_raw = price_account.to_price_feed(price_info.key);

    let pyth_ema_price = if cfg!(feature = "skip_price_validation") {
        // Don't validate price in tests
        pyth_raw.get_ema_price_unchecked()
    } else if let Some(pyth_ema_price) =
        pyth_raw.get_ema_price_no_older_than(clock.unix_timestamp, STALENESS_THRESHOLD)
    {
        pyth_ema_price
    } else {
        msg!(
            "No recent (10 minutes) EMA price in pyth account {}",
            price_info.key
        );
        return Err(ScopeError::PriceNotValid.into());
    };

    if pyth_ema_price.expo > 0 {
        msg!(
            "Pyth price account {} provided has a negative EMA price exponent: {}",
            price_info.key,
            pyth_ema_price.expo,
        );
        return Err(ScopeError::PriceNotValid.into());
    }

    let ema_price =
        crate::oracles::pyth::validate_valid_price(&pyth_ema_price, ORACLE_CONFIDENCE_FACTOR)
            .map_err(|e| {
                msg!("Invalid EMA price on pyth account {}", price_info.key);
                e
            })?;

    Ok(DatedPrice {
        price: Price {
            value: ema_price,
            exp: pyth_ema_price.expo.abs().try_into().unwrap(),
        },
        last_updated_slot: price_account.valid_slot,
        unix_timestamp: u64::try_from(price_account.timestamp).unwrap(),
        ..Default::default()
    })
}
