//! Toolings to retrieve pyth prices and validate them
//!
//! Validation partially follows [pyth best practices](https://docs.pyth.network/consumers/best-practices)
//!
//! 1. Some checks in [`validate_pyth_price`] are performed on the pyth price account upon registration in
//!    the oracle mapping. However some information present only in the associated pyth product account are
//!    expected to be checked by the admin to ensure the product has the expected quality prior the mapping
//!    update.
//! 2. Upon usage the current price state is checked in [`validate_valid_price`]
//! 3. The confidence interval is also checked in this same function with [`ORACLE_CONFIDENCE_FACTOR`]

use std::convert::{TryFrom, TryInto};

use anchor_lang::prelude::*;
use anchor_lang::solana_program::clock::DEFAULT_MS_PER_SLOT;
use pyth_client::PriceType;
use pyth_sdk_solana::state as pyth_client;

use crate::{DatedPrice, Price, ScopeError};

/// validate price confidence - confidence/price ratio should be less than 2%
pub const ORACLE_CONFIDENCE_FACTOR: u64 = 50; // 100% / 2%

/// Only update with prices not older than 10 minutes, users can still check actual price age
const STALENESS_SLOT_THRESHOLD: u64 = (10 * 60 * 1000) / DEFAULT_MS_PER_SLOT; // 10 minutes

pub fn get_price(price_info: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    let data = price_info.try_borrow_data()?;
    let price_account: &pyth_client::SolanaPriceAccount =
        pyth_client::load_price_account(data.as_ref()).map_err(|e| {
            msg!("Error loading pyth account {}: {:?}", price_info.key, e);
            ScopeError::PriceNotValid
        })?;

    let oldest_accepted_slot = clock.slot.saturating_sub(STALENESS_SLOT_THRESHOLD);

    let (pyth_price, slot, timestamp) = if price_account.agg.status
        == pyth_client::PriceStatus::Trading
        && price_account.agg.pub_slot >= oldest_accepted_slot
    {
        let pyth_price = pyth_client::Price {
            conf: price_account.agg.conf,
            expo: price_account.expo,
            price: price_account.agg.price,
            publish_time: price_account.timestamp,
        };
        (
            pyth_price,
            price_account.agg.pub_slot,
            price_account.timestamp,
        )
    } else if price_account.prev_slot >= oldest_accepted_slot {
        let pyth_price = pyth_client::Price {
            conf: price_account.prev_conf,
            expo: price_account.expo,
            price: price_account.prev_price,
            publish_time: price_account.prev_timestamp,
        };
        (
            pyth_price,
            price_account.prev_slot,
            price_account.prev_timestamp,
        )
    } else {
        msg!(
            "Price in pyth account {} is older than 10 minutes",
            price_info.key
        );
        return Err(ScopeError::PriceNotValid.into());
    };

    if pyth_price.expo > 0 {
        msg!(
            "Pyth price account {} provided has a negative price exponent: {}",
            price_info.key,
            pyth_price.expo
        );
        return Err(ScopeError::PriceNotValid.into());
    }

    let price = validate_valid_price(&pyth_price, ORACLE_CONFIDENCE_FACTOR).map_err(|e| {
        msg!(
            "Price validity check failed on pyth account {}",
            price_info.key
        );
        e
    })?;

    Ok(DatedPrice {
        price: Price {
            value: price,
            exp: pyth_price.expo.abs().try_into().unwrap(),
        },
        last_updated_slot: slot,
        unix_timestamp: u64::try_from(timestamp).unwrap(),
        ..Default::default()
    })
}

pub fn validate_valid_price(
    pyth_price: &pyth_client::Price,
    oracle_confidence_factor: u64,
) -> std::result::Result<u64, ScopeError> {
    if cfg!(feature = "skip_price_validation") {
        return Ok(u64::try_from(pyth_price.price).unwrap());
    }

    let price = u64::try_from(pyth_price.price).unwrap();
    if price == 0 {
        msg!("Pyth price is 0");
        return Err(ScopeError::PriceNotValid);
    }
    let conf: u64 = pyth_price.conf;
    let conf_50x: u64 = conf.checked_mul(oracle_confidence_factor).unwrap();
    if conf_50x > price {
        msg!("Pyth price has a confidence interval too large: {}", conf);
        return Err(ScopeError::PriceNotValid);
    };
    Ok(price)
}

fn validate_pyth_price(pyth_price: &pyth_client::SolanaPriceAccount) -> Result<()> {
    if pyth_price.magic != pyth_client::MAGIC {
        msg!("Pyth price account provided is not a valid Pyth account");
        return err!(ScopeError::PriceNotValid);
    }
    if !matches!(pyth_price.ptype, PriceType::Price) {
        msg!("Pyth price account provided has invalid price type");
        return err!(ScopeError::PriceNotValid);
    }
    if pyth_price.ver != pyth_client::VERSION_2 {
        msg!("Pyth price account provided has a different version than the Pyth client");
        return err!(ScopeError::PriceNotValid);
    }
    if !matches!(pyth_price.agg.status, pyth_client::PriceStatus::Trading) {
        msg!("Pyth price account provided is not active");
        return err!(ScopeError::PriceNotValid);
    }
    Ok(())
}

pub fn validate_pyth_price_info(pyth_price_info: &Option<AccountInfo>) -> Result<()> {
    if cfg!(feature = "skip_price_validation") {
        return Ok(());
    }
    let Some(pyth_price_info) = pyth_price_info else {
        msg!("No pyth price account provided");
        return err!(ScopeError::PriceNotValid);
    };
    let pyth_price_data = pyth_price_info.try_borrow_data()?;
    let pyth_price = pyth_client::load_price_account(&pyth_price_data).unwrap();

    validate_pyth_price(pyth_price)
}
