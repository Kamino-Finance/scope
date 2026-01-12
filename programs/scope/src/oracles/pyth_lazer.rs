use anchor_lang::prelude::*;
use pyth_lazer_protocol::{
    payload::{PayloadData, PayloadPropertyValue},
    router::{channel_ids::FIXED_RATE_200, Price as PythLazerPrice},
};

use crate::{
    utils::math::{check_confidence_interval, estimate_slot_update_from_ts},
    warn, DatedPrice, Price, ScopeError, ScopeResult,
};

const PYTH_LAZER_MIN_EXPONENT: u8 = 3;
const PYTH_LAZER_MAX_EXPONENT: u8 = 12;

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize, Clone, Copy)]
pub struct PythLazerData {
    pub feed_id: u16,
    pub exponent: u8,
    pub confidence_factor: u32,
}

impl PythLazerData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize PythLazerData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize PythLazerData");
        buff
    }
}

pub fn validate_payload_data_for_group(
    payload_data: &PayloadData,
    num_tokens_in_group: usize,
) -> ScopeResult<()> {
    // Check that the channel is what we expect
    if payload_data.channel_id != FIXED_RATE_200 {
        return Err(ScopeError::PythLazerInvalidChannel);
    }

    // Check that the payload has a single feed
    if payload_data.feeds.len() != num_tokens_in_group {
        return Err(ScopeError::PythLazerInvalidFeedsLength);
    }

    Ok(())
}

pub fn validate_payload_data_for_token(
    payload_data: &PayloadData,
    feed_idx: usize,
    pyth_lazer_data: &PythLazerData,
) -> ScopeResult<Price> {
    let PythLazerData {
        feed_id: expected_feed_id,
        exponent: expected_exponent,
        confidence_factor,
    } = pyth_lazer_data;

    // Check that the feed id is what we expect
    if payload_data.feeds[feed_idx].feed_id.0 != u32::from(*expected_feed_id) {
        return Err(ScopeError::PythLazerInvalidFeedId);
    }

    // Check that the payload contains all the properties we expect: price, exponent,
    // best bid price and best ask price
    let mut price_opt: Option<PythLazerPrice> = None;
    let mut best_bid_price_opt: Option<PythLazerPrice> = None;
    let mut best_ask_price_opt: Option<PythLazerPrice> = None;
    let mut exponent_opt: Option<i16> = None;

    for property in payload_data.feeds[feed_idx].properties.iter() {
        match property {
            PayloadPropertyValue::Price(Some(price)) => {
                price_opt = Some(*price);
            }
            PayloadPropertyValue::BestBidPrice(Some(price)) => {
                best_bid_price_opt = Some(*price);
            }
            PayloadPropertyValue::BestAskPrice(Some(price)) => {
                best_ask_price_opt = Some(*price);
            }
            PayloadPropertyValue::Exponent(exponent) => {
                exponent_opt = Some(*exponent);
            }
            _ => {
                continue;
            }
        }
    }

    let pyth_lazer_price: i64 = price_opt
        .ok_or(ScopeError::PythLazerPriceNotPresent)?
        .into_inner()
        .into();
    let pyth_lazer_price = u64::try_from(pyth_lazer_price).map_err(|_| {
        warn!("Pyth Lazer: error converting price to u64");
        ScopeError::OutOfRangeIntegralConversion
    })?;

    let received_exponent = exponent_opt.ok_or(ScopeError::PythLazerExponentNotPresent)?;
    // Pyth Lazer sends the exponent as a negative integer, so we need to negate it
    let received_exponent_neg = received_exponent.checked_neg().ok_or_else(|| {
        warn!("Pyth Lazer: overflow when negating received exponent {received_exponent}");
        ScopeError::OutOfRangeIntegralConversion
    })?;
    if received_exponent_neg != i16::from(*expected_exponent) {
        warn!("Pyth Lazer: unexpected exponent received in feed payload {received_exponent}");
        return Err(ScopeError::PythLazerUnexpectedExponent);
    }
    let exponent_u64 = u64::from(*expected_exponent);

    let new_price = Price {
        value: pyth_lazer_price,
        exp: exponent_u64,
    };

    let best_bid_price: i64 = best_bid_price_opt
        .ok_or(ScopeError::PythLazerBestBidPriceNotPresent)?
        .into_inner()
        .into();
    let best_bid_price = u64::try_from(best_bid_price).map_err(|_| {
        warn!("Pyth Lazer: error converting best bid price to u64");
        ScopeError::OutOfRangeIntegralConversion
    })?;
    let best_bid_price = Price {
        value: best_bid_price,
        exp: exponent_u64,
    };

    let best_ask_price: i64 = best_ask_price_opt
        .ok_or(ScopeError::PythLazerBestAskPriceNotPresent)?
        .into_inner()
        .into();
    let best_ask_price = u64::try_from(best_ask_price).map_err(|_| {
        warn!("Pyth Lazer: error converting best ask price to u64");
        ScopeError::OutOfRangeIntegralConversion
    })?;
    let best_ask_price = Price {
        value: best_ask_price,
        exp: exponent_u64,
    };

    let spread_value_opt = best_ask_price.value.checked_sub(best_bid_price.value);
    if spread_value_opt.is_none() {
        return Err(ScopeError::PythLazerInvalidAskBidPrices);
    }

    check_confidence_interval(
        u128::from(new_price.value),
        u32::from(*expected_exponent),
        u128::from(spread_value_opt.unwrap()),
        u32::from(*expected_exponent),
        *confidence_factor,
    )
    .map_err(|e| {
        warn!(
            "PythLazer provided a price '{}' with bid '{}' and ask '{}' not fitting the \
            configured '{confidence_factor}' confidence factor",
            new_price.value, best_bid_price.value, best_ask_price.value,
        );
        e
    })?;

    Ok(new_price)
}

pub fn update_price(
    dated_price: &mut DatedPrice,
    data: &PayloadData,
    feed_idx: usize,
    generic_data: &[u8],
    clock: &Clock,
) -> ScopeResult<()> {
    // Check that the timestamp of the new price indicates a later update
    // Note: This logic should be correct the first time we refresh the price, when we have
    // `generic_data` from a previous price, because `generic_data` can be either:
    // - uninitialized with a 0 default value
    // - used by a previous price, with a smaller timestamp (because pyth lazer timestamps are in microseconds)
    let last_pyth_lazer_timestamp_us =
        u64::from_le_bytes(dated_price.generic_data[0..8].try_into().unwrap());
    let curr_pyth_lazer_timestamp_us = data.timestamp_us.0;
    if curr_pyth_lazer_timestamp_us <= last_pyth_lazer_timestamp_us {
        warn!("Refreshing pyth lazer price: an outdated report was provided");
        return Err(ScopeError::BadTimestamp);
    }

    let pyth_lazer_data = PythLazerData::from_generic_data(generic_data)?;
    let new_price = validate_payload_data_for_token(data, feed_idx, &pyth_lazer_data)?;

    let current_onchain_timestamp_s: u64 = clock
        .unix_timestamp
        .try_into()
        .expect("Invalid clock timestamp");
    // `curr_pyth_lazer_ts` is in microseconds, so we convert into seconds
    let price_timestamp_s = u64::min(
        curr_pyth_lazer_timestamp_us / 1_000_000,
        current_onchain_timestamp_s,
    );
    let mut generic_data = [0u8; 24];
    generic_data[..8].copy_from_slice(&curr_pyth_lazer_timestamp_us.to_le_bytes());

    *dated_price = DatedPrice {
        price: new_price,
        last_updated_slot: estimate_slot_update_from_ts(clock, price_timestamp_s),
        unix_timestamp: price_timestamp_s,
        generic_data,
    };

    Ok(())
}

pub fn validate_mapping_cfg(mapping: Option<&AccountInfo>, generic_data: &[u8]) -> ScopeResult<()> {
    if mapping.is_some() {
        warn!("No mapping account is expected for PythLazer oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }

    let PythLazerData {
        feed_id,
        exponent,
        confidence_factor,
    } = PythLazerData::from_generic_data(generic_data)?;

    msg!(
        "Pyth Lazer: validating mapping with feed_id = {feed_id}, exponent = {exponent}, confidence_factor = {confidence_factor}",
    );

    if feed_id == 0 {
        return Err(ScopeError::PythLazerInvalidFeedID);
    }

    if !(PYTH_LAZER_MIN_EXPONENT..=PYTH_LAZER_MAX_EXPONENT).contains(&exponent) {
        return Err(ScopeError::PythLazerInvalidExponent);
    }

    if confidence_factor < 1 {
        return Err(ScopeError::PythLazerInvalidConfidenceFactor);
    }

    Ok(())
}
