use std::convert::TryInto;

use anchor_lang::prelude::*;
use sbod_itf::accounts::PullFeedAccountData;

use super::switchboard_v2::validate_confidence;
use crate::{
    utils::{math::slots_to_secs, zero_copy_deserialize},
    warn, DatedPrice, Price, ScopeError,
};

const MAX_EXPONENT: u32 = 15;

pub fn get_price(
    switchboard_feed_info: &AccountInfo,
    clock: &Clock,
) -> std::result::Result<DatedPrice, ScopeError> {
    let feed = zero_copy_deserialize::<PullFeedAccountData>(switchboard_feed_info)?;

    let price_switchboard_desc = feed
        .result
        .value()
        .ok_or(ScopeError::SwitchboardOnDemandError)?;
    let price: Price = price_switchboard_desc.try_into()?;

    if !cfg!(feature = "skip_price_validation") {
        let std_dev = feed
            .result
            .std_dev()
            .ok_or(ScopeError::SwitchboardOnDemandError)?;
        if validate_confidence(
            price_switchboard_desc.mantissa(),
            price_switchboard_desc.scale(),
            std_dev.mantissa(),
            std_dev.scale(),
        )
        .is_err()
        {
            warn!(
                    "Validation of confidence interval for SB On-Demand feed {} failed. Price: {:?}, stdev_mantissa: {:?}, stdev_scale: {:?}",
                    switchboard_feed_info.key(),
                    price,
                    std_dev.mantissa(),
                    std_dev.scale()
                );
            return Err(ScopeError::SwitchboardOnDemandError);
        }
    };

    // NOTE: This is the slot and timestamp of the selected sample,
    // not necessarily the most recent one.
    let last_updated_slot = feed.result.slot;

    // In absence of better option, we estimate the timestamp from the slot.
    let elapsed_slots = clock.slot.saturating_sub(last_updated_slot);
    let unix_timestamp = u64::try_from(clock.unix_timestamp)
        .unwrap_or(0)
        .saturating_sub(slots_to_secs(elapsed_slots));

    Ok(DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        ..Default::default()
    })
}

pub fn validate_price_account(switchboard_feed_info: Option<&AccountInfo>) -> Result<()> {
    if cfg!(feature = "skip_price_validation") {
        return Ok(());
    }
    let Some(switchboard_feed_info) = switchboard_feed_info else {
        warn!("No switchboard price account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };
    zero_copy_deserialize::<PullFeedAccountData>(switchboard_feed_info)?;
    Ok(())
}

impl TryFrom<rust_decimal::Decimal> for Price {
    type Error = ScopeError;

    fn try_from(sb_decimal: rust_decimal::Decimal) -> std::result::Result<Self, Self::Error> {
        if sb_decimal.mantissa() < 0 {
            warn!("Switchboard v2 oracle price feed is negative");
            return Err(ScopeError::PriceNotValid);
        }
        let (exp, value) = if sb_decimal.scale() > MAX_EXPONENT {
            // exp is capped. Remove the extra digits from the mantissa.
            let exp_diff = sb_decimal
                .scale()
                .checked_sub(MAX_EXPONENT)
                .ok_or(ScopeError::MathOverflow)?;
            let factor = 10_i128
                .checked_pow(exp_diff)
                .ok_or(ScopeError::MathOverflow)?;
            // Loss of precision here is expected.
            let value = sb_decimal.mantissa() / factor;
            (MAX_EXPONENT, value)
        } else {
            (sb_decimal.scale(), sb_decimal.mantissa())
        };
        let exp: u64 = exp.into();
        let value: u64 = value.try_into().map_err(|_| ScopeError::IntegerOverflow)?;
        Ok(Price { value, exp })
    }
}
