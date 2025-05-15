use anchor_lang::prelude::*;

use crate::{
    utils::{account_deserialize, math},
    warn, DatedPrice, Price, ScopeError, ScopeResult,
};

const MILLIS_PER_SECOND: u64 = 1_000;
const MILLIS_PER_SECOND_I64: i64 = 1_000;

/// Price is kept within a 0.2% or 0.5% deviation threshold, depending on the feed.
/// At least one update per 30h should happen.
pub const VALID_PRICE_AGE_MS: i64 = 30 * crate::utils::SECONDS_PER_HOUR * MILLIS_PER_SECOND_I64;

pub const RESERVED_BYTE_SIZE: usize = 64;
pub const U256_BYTE_SIZE: usize = 256 / 8;
pub const U64_START_INDEX: usize = U256_BYTE_SIZE - 8;

#[account]
pub struct RedStonePriceData {
    // The size of these two arrays is `U256_BYTE_SIZE`
    pub feed_id: [u8; 32],
    pub value: [u8; 32],
    // `timestamp` is when the price was computed...
    pub timestamp: u64,
    // ... while `write_timestamp` is when the price was pushed to the account
    pub write_timestamp: Option<u64>,
    pub write_slot_number: u64,
    pub decimals: u8,
    // The size of this array is `RESERVED_BYTE_SIZE`
    pub reserved: [u8; 64],
}

fn redstone_value_to_scope_price(
    raw_be_value: &[u8; U256_BYTE_SIZE],
    decimals: u8,
) -> ScopeResult<Price> {
    if !raw_be_value.iter().take(U64_START_INDEX).all(|&v| v == 0) {
        warn!("RedStone price received overflows an u64");
        return Err(ScopeError::IntegerOverflow);
    }

    let value = u64::from_be_bytes(raw_be_value[U64_START_INDEX..].try_into().unwrap());
    Ok(Price {
        value,
        exp: u64::from(decimals),
    })
}

fn check_price_age(timestamp_ms: Option<u64>, clock: &Clock) -> ScopeResult<()> {
    let timestamp_ms = timestamp_ms.ok_or(ScopeError::BadTimestamp)?;
    let timestamp_ms_i64 = TryInto::<i64>::try_into(timestamp_ms).map_err(|_| {
        warn!("Redstone: overflow when converting timestamp to i64");
        ScopeError::BadTimestamp
    })?;

    let ms_since_last_update =
        (clock.unix_timestamp * MILLIS_PER_SECOND_I64).checked_sub(timestamp_ms_i64);
    match ms_since_last_update {
        Some(ms_since_last_update) if ms_since_last_update <= VALID_PRICE_AGE_MS => Ok(()),
        _ => Err(ScopeError::BadTimestamp),
    }
}

pub fn get_price(price_info: &AccountInfo, clock: &Clock) -> ScopeResult<DatedPrice> {
    let price_data: RedStonePriceData = account_deserialize(price_info)?;
    let price = redstone_value_to_scope_price(&price_data.value, price_data.decimals)?;

    check_price_age(price_data.write_timestamp, clock)?;

    let Some(last_update_timestamp_ms) = price_data.write_timestamp else {
        return Err(ScopeError::BadTimestamp);
    };

    let unix_timestamp = (last_update_timestamp_ms.min(price_data.timestamp) / MILLIS_PER_SECOND)
        .min(clock.unix_timestamp.try_into().unwrap());
    let last_updated_slot = price_data
        .write_slot_number
        .min(clock.slot)
        .min(math::estimate_slot_update_from_ts(clock, unix_timestamp));

    Ok(DatedPrice {
        price,
        unix_timestamp,
        last_updated_slot,
        generic_data: Default::default(),
    })
}

pub fn validate_price_account(price_data_account: &Option<AccountInfo>) -> ScopeResult<()> {
    let Some(price_data_account) = price_data_account else {
        warn!("No RedStone price account provided");
        return Err(ScopeError::ExpectedPriceAccount);
    };

    let _: RedStonePriceData = account_deserialize(price_data_account)?;
    Ok(())
}
