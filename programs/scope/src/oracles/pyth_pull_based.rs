use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::{self, PriceUpdateV2, VerificationLevel};

use crate::{utils::account_deserialize, DatedPrice, ScopeError};
pub const MAXIMUM_AGE: u64 = 10 * 60; // Ten minutes
use pyth_sdk_solana::state as pyth_client;

use self::utils::get_last_updated_slot;
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

    // todo: Discuss how we should handle the time jump that can happen when there is an outage?
    let last_updated_slot = get_last_updated_slot(clock, publish_time);
    Ok(DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp: publish_time.try_into().unwrap(),
        ..Default::default()
    })
}

pub fn validate_price_update_v2_info(price_info: &Option<AccountInfo>) -> Result<()> {
    if cfg!(feature = "skip_price_validation") {
        return Ok(());
    }
    let Some(price_info) = price_info else {
        warn!("No pyth pull price account provided");
        return err!(ScopeError::PriceNotValid);
    };
    let _: PriceUpdateV2 = account_deserialize(price_info)?;
    Ok(())
}

pub mod utils {
    use super::*;
    use crate::utils::math::saturating_secs_to_slots;

    pub fn get_last_updated_slot(clock: &Clock, publish_time: i64) -> u64 {
        let elapsed_time_s = u64::try_from(clock.unix_timestamp)
            .unwrap()
            .saturating_sub(u64::try_from(publish_time).unwrap());
        let elapsed_slot_estimate = saturating_secs_to_slots(elapsed_time_s);
        clock.slot.saturating_sub(elapsed_slot_estimate)
    }
}
