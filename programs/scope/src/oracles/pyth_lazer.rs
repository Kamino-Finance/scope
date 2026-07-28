use anchor_lang::prelude::*;
use pyth_lazer_protocol::{
    payload::{PayloadData, PayloadPropertyValue},
    time::TimestampUs,
    ChannelId, Price as PythLazerPrice,
};

use crate::{
    states::{OracleMappings, OraclePrices},
    utils::math::{check_confidence_interval, estimate_slot_update_from_ts},
    warn, DatedPrice, Price, ScopeError, ScopeResult,
};

const PYTH_LAZER_MIN_EXPONENT: u8 = 3;
const PYTH_LAZER_MAX_EXPONENT: u8 = 12;

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize, Clone, Copy)]
pub struct PythLazerData {
    pub feed_id: u16,
    pub exponent: u8,
    /// Tolerance factor for the bid/ask spread check (`ask - bid` against the
    /// price). `0` disables the spread check entirely, in which case the payload
    /// is not required to carry `BestBidPrice`/`BestAskPrice`.
    pub bid_ask_spread_factor: u32,
    pub ema_enabled: bool,
    pub ema_confidence_factor: u32,
    /// Tolerance factor for the native Lazer `Confidence` check; `0` disables it.
    pub price_confidence_factor: u32,
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

/// Reference-oracle config for `OracleType::PythLazerEMA`.
///
/// Stored in `oracle_mappings.generic[index]` for an EMA entry. The EMA value
/// itself lives in the source `PythLazer` entry's `dated_price.generic_data`,
/// populated by `update_price` whenever the spot refresh payload includes an
/// `EmaPrice` property. See `get_ema_price`.
#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize, Clone, Copy)]
pub struct PythLazerEmaRefData {
    /// Token index of the source `PythLazer` entry to read the EMA from.
    pub source_entry: u16,
}

impl PythLazerEmaRefData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize PythLazerEmaRefData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize PythLazerEmaRefData");
        buff
    }
}

/// Layout of `DatedPrice.generic_data` (24 bytes) for `PythLazer` entries.
///
/// `update_price` writes the spot feed timestamp on every refresh, and the EMA
/// fields whenever the payload carries an `EmaPrice`. `ema_feed_update_timestamp_us == 0`
/// is the "EMA never received" sentinel consumed by `get_ema_price`.
#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize, Clone, Copy)]
pub struct PythLazerStoredData {
    pub spot_feed_update_timestamp_us: u64,
    pub ema_price_value: u64,
    pub ema_feed_update_timestamp_us: u64,
}

impl PythLazerStoredData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize PythLazerStoredData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 24] {
        let mut buff = [0u8; 24];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize PythLazerStoredData");
        buff
    }
}

pub fn validate_payload_data_for_group(
    payload_data: &PayloadData,
    num_tokens_in_group: usize,
) -> ScopeResult<()> {
    // Check that the channel is what we expect
    if payload_data.channel_id != ChannelId::FIXED_RATE_200 {
        return Err(ScopeError::PythLazerInvalidChannel);
    }

    // Check that the payload has a single feed
    if payload_data.feeds.len() != num_tokens_in_group {
        return Err(ScopeError::PythLazerInvalidFeedsLength);
    }

    Ok(())
}

/// Convert a Pyth Lazer price mantissa to `u64`, logging `what` on a negative
/// mantissa. Prefer the `mantissa_to_u64!` macro, which fills `what` in for you.
fn mantissa_to_u64(price: PythLazerPrice, what: &str) -> ScopeResult<u64> {
    let mantissa: i64 = price.mantissa_i64();
    u64::try_from(mantissa).map_err(|_| {
        warn!(
            "Pyth Lazer: {} mantissa {} doesn't fit in u64",
            what, mantissa
        );
        ScopeError::OutOfRangeIntegralConversion
    })
}

/// Convert a Pyth Lazer price mantissa to `u64`, deriving the log label from the
/// passed identifier via `stringify!`. Two forms:
/// - `mantissa_to_u64!(opt, not_present_err)` unwraps an `Option<PythLazerPrice>`
///   (returning `not_present_err` on `None`) before converting.
/// - `mantissa_to_u64!(price)` converts an already-unwrapped `PythLazerPrice`.
macro_rules! mantissa_to_u64 {
    ($opt:ident, $not_present:expr) => {
        mantissa_to_u64($opt.ok_or($not_present)?, stringify!($opt))
    };
    ($price:ident) => {
        mantissa_to_u64($price, stringify!($price))
    };
}

pub fn validate_payload_data_for_token(
    payload_data: &PayloadData,
    feed_idx: usize,
    pyth_lazer_data: &PythLazerData,
) -> ScopeResult<(Price, TimestampUs)> {
    let PythLazerData {
        feed_id: expected_feed_id,
        exponent: expected_exponent,
        bid_ask_spread_factor,
        ema_enabled: _,
        ema_confidence_factor: _,
        price_confidence_factor,
    } = pyth_lazer_data;

    // Check that the feed id is what we expect
    if payload_data.feeds[feed_idx].feed_id.0 != u32::from(*expected_feed_id) {
        return Err(ScopeError::PythLazerInvalidFeedId);
    }

    // Collect the feed's properties; each check below reads only the inputs it needs.
    let mut price_opt: Option<PythLazerPrice> = None;
    let mut best_bid_price_opt: Option<PythLazerPrice> = None;
    let mut best_ask_price_opt: Option<PythLazerPrice> = None;
    let mut confidence_opt: Option<PythLazerPrice> = None;
    let mut exponent_opt: Option<i16> = None;
    let mut feed_update_timestamp_opt: Option<TimestampUs> = None;

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
            PayloadPropertyValue::Confidence(Some(c)) => {
                confidence_opt = Some(*c);
            }
            PayloadPropertyValue::Exponent(exponent) => {
                exponent_opt = Some(*exponent);
            }
            PayloadPropertyValue::FeedUpdateTimestamp(Some(ts)) => {
                feed_update_timestamp_opt = Some(*ts);
            }
            _ => {
                continue;
            }
        }
    }

    let pyth_lazer_price = mantissa_to_u64!(price_opt, ScopeError::PythLazerPriceNotPresent)?;

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

    // Bid/ask spread check. When the factor is 0 the check is disabled, and the
    // payload is neither required to carry bid/ask nor inspected for them (feeds
    // such as NAV/redemption rates publish no bid/ask).
    if *bid_ask_spread_factor > 0 {
        let best_bid_price = mantissa_to_u64!(
            best_bid_price_opt,
            ScopeError::PythLazerBestBidPriceNotPresent
        )?;

        let best_ask_price = mantissa_to_u64!(
            best_ask_price_opt,
            ScopeError::PythLazerBestAskPriceNotPresent
        )?;

        let spread_value = best_ask_price
            .checked_sub(best_bid_price)
            .ok_or(ScopeError::PythLazerInvalidAskBidPrices)?;

        check_confidence_interval(
            u128::from(new_price.value),
            u32::from(*expected_exponent),
            u128::from(spread_value),
            u32::from(*expected_exponent),
            *bid_ask_spread_factor,
        )
        .map_err(|e| {
            warn!(
                "PythLazer provided a price '{}' with bid '{best_bid_price}' and ask \
                '{best_ask_price}' not fitting the configured '{bid_ask_spread_factor}' \
                bid/ask spread factor",
                new_price.value,
            );
            e
        })?;
    }

    // Native Lazer confidence check. When the factor is 0 the check is disabled,
    // and the payload is not required to carry a `Confidence` property.
    if *price_confidence_factor > 0 {
        let confidence_value =
            mantissa_to_u64!(confidence_opt, ScopeError::PythLazerConfidenceNotPresent)?;

        check_confidence_interval(
            u128::from(new_price.value),
            u32::from(*expected_exponent),
            u128::from(confidence_value),
            u32::from(*expected_exponent),
            *price_confidence_factor,
        )
        .map_err(|e| {
            warn!(
                "PythLazer provided a price '{}' with confidence '{confidence_value}' not fitting \
                the configured '{price_confidence_factor}' price confidence factor",
                new_price.value,
            );
            e
        })?;
    }

    let feed_update_timestamp =
        feed_update_timestamp_opt.ok_or(ScopeError::PythLazerFeedUpdateTimestampNotPresent)?;

    Ok((new_price, feed_update_timestamp))
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
    let pyth_lazer_data = PythLazerData::from_generic_data(generic_data)?;
    let (new_price, feed_update_timestamp) =
        validate_payload_data_for_token(data, feed_idx, &pyth_lazer_data)?;

    let mut stored = PythLazerStoredData::from_generic_data(&dated_price.generic_data)?;
    let curr_feed_update_timestamp = feed_update_timestamp.as_micros();
    if curr_feed_update_timestamp <= stored.spot_feed_update_timestamp_us {
        warn!("Refreshing pyth lazer price: an outdated report was provided");
        return Err(ScopeError::BadTimestamp);
    }

    let curr_timestamp_us = data.timestamp_us.as_micros();
    if curr_feed_update_timestamp > curr_timestamp_us {
        warn!(
            "Pyth lazer feed_update_timestamp ({}) is newer than payload timestamp ({})",
            curr_feed_update_timestamp,
            data.timestamp_us.as_micros()
        );
    }

    let current_onchain_timestamp_s: u64 = clock
        .unix_timestamp
        .try_into()
        .expect("Invalid clock timestamp");
    // `curr_pyth_lazer_ts` is in microseconds, so we convert into seconds
    let price_timestamp_s = u64::min(
        u64::min(curr_feed_update_timestamp, curr_timestamp_us) / 1_000_000,
        current_onchain_timestamp_s,
    );

    // Preserve EMA fields when the current payload omits `EmaPrice` — Pyth Lazer
    // is allowed to publish a tick without it (`PayloadPropertyValue::EmaPrice(Option<Price>)`).
    // The EMA exponent is not stored: it equals `pyth_lazer_data.exponent` (already
    // validated against the wire `Exponent` for both spot and EMA in this same payload).
    stored.spot_feed_update_timestamp_us = curr_feed_update_timestamp;
    match extract_ema_from_payload(data, feed_idx)? {
        Some(ema) => {
            validate_ema_confidence(
                &ema,
                pyth_lazer_data.exponent,
                pyth_lazer_data.ema_confidence_factor,
            )?;
            stored.ema_price_value = ema.value;
            stored.ema_feed_update_timestamp_us = curr_feed_update_timestamp;
        }
        None if pyth_lazer_data.ema_enabled => {
            warn!("Pyth Lazer: EMA is enabled but payload does not contain EMA price");
            return Err(ScopeError::PythLazerEmaPriceNotPresent);
        }
        None => {}
    }

    *dated_price = DatedPrice {
        price: new_price,
        last_updated_slot: estimate_slot_update_from_ts(clock, price_timestamp_s),
        unix_timestamp: price_timestamp_s,
        generic_data: stored.to_generic_data(),
    };

    Ok(())
}

struct PythLazerEmaPayload {
    value: u64,
    confidence: Option<u64>,
}

/// Extract the EMA value and its confidence from a Pyth Lazer payload feed.
///
/// Returns `Some` when the payload carries a present `EmaPrice`, or `None` when
/// EMA is absent or `EmaPrice(None)` — the publisher is allowed to skip it on a
/// per-tick basis. `EmaConfidence` is captured alongside so the caller can gate
/// the EMA on its own confidence interval (an EMA has no bid/ask spread). The EMA
/// shares the spot's `FeedUpdateTimestamp` (Lazer publishes one per tick) and
/// `Exponent`, so the caller reuses both from `validate_payload_data_for_token`
/// rather than re-parsing them here.
fn extract_ema_from_payload(
    payload_data: &PayloadData,
    feed_idx: usize,
) -> ScopeResult<Option<PythLazerEmaPayload>> {
    let mut ema_price_opt: Option<PythLazerPrice> = None;
    let mut ema_confidence_opt: Option<PythLazerPrice> = None;
    for property in payload_data.feeds[feed_idx].properties.iter() {
        match property {
            PayloadPropertyValue::EmaPrice(Some(price)) => ema_price_opt = Some(*price),
            PayloadPropertyValue::EmaConfidence(Some(price)) => ema_confidence_opt = Some(*price),
            _ => continue,
        }
    }

    let Some(ema_price) = ema_price_opt else {
        return Ok(None);
    };
    let value = mantissa_to_u64!(ema_price)?;
    let confidence = match ema_confidence_opt {
        Some(ema_confidence) => Some(mantissa_to_u64!(ema_confidence)?),
        None => None,
    };
    Ok(Some(PythLazerEmaPayload { value, confidence }))
}

fn validate_ema_confidence(
    ema: &PythLazerEmaPayload,
    exponent: u8,
    ema_confidence_factor: u32,
) -> ScopeResult<()> {
    let confidence = ema
        .confidence
        .ok_or(ScopeError::PythLazerEmaConfidenceNotPresent)?;
    check_confidence_interval(
        u128::from(ema.value),
        u32::from(exponent),
        u128::from(confidence),
        u32::from(exponent),
        ema_confidence_factor,
    )
}

/// Project the EMA bytes packed by `update_price` on a `PythLazer` source entry
/// into a `DatedPrice` for a `PythLazerEMA` reference token.
///
/// The exponent is inherited from the source's `PythLazerData.exponent` (read from
/// `oracle_mappings.generic[source_entry]` inside this function). `ema_ts == 0` is the
/// "EMA never received" sentinel, returned as `PythLazerEmaPriceNotPresent`.
pub fn get_ema_price(
    oracle_prices: &OraclePrices,
    oracle_mappings: &OracleMappings,
    generic_data: &[u8],
    clock: &Clock,
) -> ScopeResult<DatedPrice> {
    let PythLazerEmaRefData { source_entry } =
        PythLazerEmaRefData::from_generic_data(generic_data)?;

    let source_idx = usize::from(source_entry);
    if source_idx >= crate::MAX_ENTRIES {
        return Err(ScopeError::BadTokenNb);
    }
    let source_type = oracle_mappings.get_entry_type(source_idx)?;
    if source_type != super::OracleType::PythLazer {
        warn!("PythLazerEMA: source entry {source_entry} is not PythLazer");
        return Err(ScopeError::BadTokenType);
    }
    let source_pyth_lazer_data =
        PythLazerData::from_generic_data(&oracle_mappings.generic[source_idx])?;
    if !source_pyth_lazer_data.ema_enabled {
        warn!(
            "PythLazerEMA: source entry {source_entry} does not have EMA enabled; \
            it will never receive an EMA price"
        );
        return Err(ScopeError::PythLazerEmaNotEnabledOnSource);
    }
    let source_exponent = source_pyth_lazer_data.exponent;

    let source = oracle_prices
        .prices
        .get(source_idx)
        .ok_or(ScopeError::BadTokenNb)?;

    let PythLazerStoredData {
        ema_price_value: ema_value,
        ema_feed_update_timestamp_us: ema_ts_us,
        spot_feed_update_timestamp_us: _,
    } = PythLazerStoredData::from_generic_data(&source.generic_data)?;

    if ema_ts_us == 0 {
        warn!("PythLazerEMA source entry {source_entry} has never received an EMA price");
        return Err(ScopeError::PythLazerEmaPriceNotPresent);
    }

    let now_s: u64 = clock
        .unix_timestamp
        .try_into()
        .map_err(|_| ScopeError::BadTimestamp)?;
    // Clamp EMA timestamp to on-chain clock (same as spot in update_price)
    // so the returned `unix_timestamp` can never be in the future. Consumer-side
    // max-age checks gate liveness against this value.
    let ema_ts_s = u64::min(ema_ts_us / 1_000_000, now_s);

    Ok(DatedPrice {
        price: Price {
            value: ema_value,
            exp: u64::from(source_exponent),
        },
        last_updated_slot: estimate_slot_update_from_ts(clock, ema_ts_s),
        unix_timestamp: ema_ts_s,
        generic_data: [0u8; 24],
    })
}

pub fn validate_mapping_cfg(mapping: Option<&AccountInfo>, generic_data: &[u8]) -> ScopeResult<()> {
    if mapping.is_some() {
        warn!("No mapping account is expected for PythLazer oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }

    let PythLazerData {
        feed_id,
        exponent,
        bid_ask_spread_factor,
        ema_enabled,
        ema_confidence_factor,
        price_confidence_factor,
    } = PythLazerData::from_generic_data(generic_data)?;

    msg!(
        "Pyth Lazer: validating mapping with feed_id = {feed_id}, exponent = {exponent}, bid_ask_spread_factor = {bid_ask_spread_factor}, ema_enabled = {ema_enabled}, ema_confidence_factor = {ema_confidence_factor}, price_confidence_factor = {price_confidence_factor}",
    );

    if feed_id == 0 {
        return Err(ScopeError::PythLazerInvalidFeedID);
    }

    if !(PYTH_LAZER_MIN_EXPONENT..=PYTH_LAZER_MAX_EXPONENT).contains(&exponent) {
        return Err(ScopeError::PythLazerInvalidExponent);
    }

    if price_confidence_factor < 1 {
        // bid/ask spread check is optional (0 disables it)
        return Err(ScopeError::PythLazerInvalidConfidenceFactor);
    }

    if ema_enabled && ema_confidence_factor < 1 {
        return Err(ScopeError::PythLazerInvalidConfidenceFactor);
    }

    Ok(())
}

pub fn validate_mapping_cfg_ema(
    mapping: Option<&AccountInfo>,
    generic_data: &[u8],
) -> ScopeResult<()> {
    // No price account expected: EMA is sourced from another scope entry.
    // The source's type is validated at runtime (in get_non_zero_price)
    // since validate_oracle_cfg does not have access to the mappings array.
    if mapping.is_some() {
        warn!("No mapping account is expected for PythLazerEMA oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }
    let parsed = PythLazerEmaRefData::from_generic_data(generic_data)?;
    if parsed.source_entry >= crate::MAX_ENTRIES_U16 {
        return Err(ScopeError::CompositeOracleInvalidSourceIndex);
    }
    Ok(())
}
