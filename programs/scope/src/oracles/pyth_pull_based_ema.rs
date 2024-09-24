use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::{
    error::GetPriceError,
    price_update::{PriceUpdateV2, VerificationLevel},
};

use crate::{utils::account_deserialize, DatedPrice, ScopeError};
pub const MAXIMUM_AGE: u64 = 10 * 60; // Ten minutes
use pyth_sdk_solana::Price as PythPrice;

use super::{pyth::validate_valid_price, pyth_pull_based::utils::get_last_updated_slot};
use crate::utils::consts::ORACLE_CONFIDENCE_FACTOR;

pub fn get_price(price_info: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    let price_account: PriceUpdateV2 = account_deserialize(price_info)?;
    let exponent = price_account.price_message.exponent;
    let conf = price_account.price_message.conf;
    let publish_time = price_account.price_message.publish_time;
    let price = get_ema_with_custom_verification_level(&price_account)?;

    if exponent > 0 {
        msg!(
            "Pyth price account provided has a negative price exponent: {}",
            exponent
        );
        return err!(ScopeError::PriceNotValid);
    }

    // Validate confidence, rebuild the struct to match the pyth_client::Price struct
    // and reuse the logic
    let old_pyth_price = PythPrice {
        conf,
        expo: exponent,
        price: price.price,
        publish_time,
    };
    let price = validate_valid_price(&old_pyth_price, ORACLE_CONFIDENCE_FACTOR).map_err(|e| {
        msg!(
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

fn get_ema_with_custom_verification_level(
    price_account: &PriceUpdateV2,
) -> std::result::Result<PythPrice, GetPriceError> {
    let price_message = price_account.price_message;

    // check verification level
    if !price_account
        .verification_level
        .gte(VerificationLevel::Full)
    {
        return Err(GetPriceError::InsufficientVerificationLevel);
    }

    let ema = PythPrice {
        price: price_message.ema_price,
        conf: price_message.ema_conf,
        expo: price_message.exponent,
        publish_time: price_message.publish_time,
    };

    Ok(ema)
}
