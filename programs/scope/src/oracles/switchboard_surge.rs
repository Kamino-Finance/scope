use std::convert::TryInto;

use anchor_lang::prelude::*;
use switchboard_surge_itf::SwitchboardQuote;
use switchboard_surge_itf::switchboard_quote::QUOTE_DISCRIMINATOR;
use crate::{
    utils::math::slots_to_secs,
    warn, DatedPrice, Price, ScopeError,
};

/// Switchboard Surge uses 18 decimals of precision (10^18)
const SB_PRECISION: u32 = 18;

/// Maximum exponent we support in our Price type
/// This matches the Switchboard On-Demand implementation
const MAX_EXPONENT: u32 = 15;

pub fn get_price(
    price_oracle: &AccountInfo,
    clock: &Clock,
) -> std::result::Result<DatedPrice, ScopeError> {
    let data = price_oracle.try_borrow_data().map_err(|_| {
        warn!("Failed to borrow Switchboard Surge quote data");
        ScopeError::UnableToDeserializeAccount
    })?;

    let discriminator = &data[..8];
    if discriminator != QUOTE_DISCRIMINATOR {
        return Err(ScopeError::UnableToDeserializeAccount);
    }
    // Deserialize as SwitchboardQuote
    let quote_data = SwitchboardQuote::deserialize(&mut &data[8..]).map_err(|e| {
        warn!("Failed to deserialize Switchboard Surge quote: {:?}", e);
        ScopeError::UnableToDeserializeAccount
    })?;

    // Get feeds from the quote
    let feeds = quote_data.feeds_slice();

    if feeds.is_empty() {
        warn!("SwitchboardQuote has no feeds");
        return Err(ScopeError::PriceNotValid);
    }

    // Use the first feed's price
    let first_feed_price = feeds[0].feed_value();

    // Check for negative price
    if first_feed_price < 0 {
        warn!("Switchboard Surge oracle price feed is negative");
        return Err(ScopeError::PriceNotValid);
    }

    // Convert the price to our Price type
    let price = convert_surge_price(first_feed_price)?;

    // Get the slot from the quote
    let last_updated_slot = quote_data.slot;

    // Estimate timestamp from slot
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

fn convert_surge_price(value: i128) -> std::result::Result<Price, ScopeError> {
    if value < 0 {
        warn!("Switchboard Surge price value is negative");
        return Err(ScopeError::PriceNotValid);
    }

    let (exp, value) = if SB_PRECISION > MAX_EXPONENT {
        // exp is capped. Remove the extra digits from the value.
        let exp_diff = SB_PRECISION
            .checked_sub(MAX_EXPONENT)
            .ok_or(ScopeError::MathOverflow)?;
        let factor = 10_i128
            .checked_pow(exp_diff)
            .ok_or(ScopeError::MathOverflow)?;
        // Loss of precision here is expected.
        let adjusted_value = value / factor;
        (MAX_EXPONENT, adjusted_value)
    } else {
        (SB_PRECISION, value)
    };

    let exp: u64 = exp.into();
    let value: u64 = value.try_into().map_err(|_| ScopeError::IntegerOverflow)?;
    Ok(Price { value, exp })
}
