use anchor_lang::prelude::*;
use anchor_lang::solana_program::clock;
use pyth_solana_receiver_sdk::price_update::{self, PriceUpdateV2, VerificationLevel};

use crate::{
    utils::{account_deserialize, price_impl::check_ref_price_difference},
    DatedPrice, OracleMappingsCore as OracleMappings, OraclePrices, Price, ScopeError,
};
pub const MAXIMUM_AGE: u64 = 10 * 60; // Ten minutes
use pyth_sdk_solana::state as pyth_client;

use super::pyth::{validate_valid_price, ORACLE_CONFIDENCE_FACTOR};

pub fn get_price(
    entry_id: usize,
    price_info: &AccountInfo,
    clock: &Clock,
    oracle_prices: &OraclePrices,
    oracle_mappings: &OracleMappings,
) -> Result<DatedPrice> {
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
        msg!(
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
    let price_value =
        validate_valid_price(&old_pyth_price, ORACLE_CONFIDENCE_FACTOR).map_err(|e| {
            msg!(
                "Confidence interval check failed on pyth account {}",
                price_info.key
            );
            e
        })?;

    let final_price = Price {
        value: price_value,
        exp: exponent.abs().try_into().unwrap(),
    };

    if oracle_mappings.ref_price[entry_id] != u16::MAX {
        let ref_price =
            oracle_prices.prices[usize::from(oracle_mappings.ref_price[entry_id])].price;
        check_ref_price_difference(final_price, ref_price)?;
    }

    // todo: Discuss how we should handle the time jump that can happen when there is an outage?
    let elapsed_time_s = u64::try_from(clock.unix_timestamp)
        .unwrap()
        .saturating_sub(u64::try_from(publish_time).unwrap());
    let elapsed_slot_estimate = elapsed_time_s * 1000 / clock::DEFAULT_MS_PER_SLOT;
    let estimated_published_slot = clock.slot.saturating_sub(elapsed_slot_estimate);
    let last_updated_slot = u64::min(estimated_published_slot, price_account.posted_slot);
    Ok(DatedPrice {
        price: final_price,
        last_updated_slot,
        unix_timestamp: publish_time.try_into().unwrap(),
        ..Default::default()
    })
}

pub fn validate_price_update_v2_info(price_info: &AccountInfo) -> Result<()> {
    if cfg!(feature = "skip_price_validation") {
        return Ok(());
    }
    let _: PriceUpdateV2 = account_deserialize(price_info)?;
    Ok(())
}
