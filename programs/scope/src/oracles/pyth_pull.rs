use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::{self, PriceUpdateV2, VerificationLevel};

use crate::{
    utils::{
        account_deserialize,
        math::{clamp_timestamp_to_now, estimate_slot_update_from_ts},
    },
    DatedPrice, ScopeError,
};
pub const MAXIMUM_AGE: u64 = 10 * 60; // Ten minutes
pub use pyth_sdk_solana::state as pyth_client;

use super::pyth::validate_valid_price;
use crate::{utils::consts::ORACLE_CONFIDENCE_FACTOR, warn};

pub fn get_price(price_info: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    let price_account: PriceUpdateV2 = account_deserialize(price_info)?;

    let price = price_account.get_price_no_older_than_with_custom_verification_level(
        clock,
        i64::MAX.try_into().unwrap(), // MAXIMUM_AGE, // this should be filtered by the caller
        &price_account.price_message.feed_id,
        VerificationLevel::Full, // All our prices and the sponsored feeds are full verified
    )?;

    let price_update::Price {
        price,
        conf,
        exponent,
        publish_time,
    } = price;

    if exponent > 0 {
        warn!(
            "Pyth price account provided has a negative price exponent: {}",
            exponent
        );
        return err!(ScopeError::PriceNotValid);
    }

    // Validate confidence, rebuild the struct to match the pyth_client::Price struct
    // and reuse the logic
    let old_pyth_price = pyth_client::Price {
        conf,
        expo: exponent,
        price,
        publish_time,
    };
    let price = validate_valid_price(&old_pyth_price, ORACLE_CONFIDENCE_FACTOR).map_err(|e| {
        warn!(
            "Confidence interval check failed on pyth account {}",
            price_info.key
        );
        e
    })?;

    // Clamp publish_time to current time to prevent future timestamps
    let unix_timestamp = clamp_timestamp_to_now(publish_time, clock)?;

    // todo: Discuss how we should handle the time jump that can happen when there is an outage?
    let last_updated_slot = estimate_slot_update_from_ts(clock, unix_timestamp);
    Ok(DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        ..Default::default()
    })
}

pub fn validate_price_update_v2_info(price_info: Option<&AccountInfo>) -> Result<()> {
    if cfg!(feature = "skip_price_validation") {
        return Ok(());
    }
    let Some(price_info) = price_info else {
        warn!("No pyth pull price account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };
    let _: PriceUpdateV2 = account_deserialize(price_info)?;
    Ok(())
}
