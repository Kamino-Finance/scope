use anchor_lang::prelude::*;
use redstone_itf::accounts::{PriceData, U256_BYTE_SIZE, U64_START_INDEX};

use crate::{
    utils::{account_deserialize, consts::MILLIS_PER_SECOND, math},
    warn, DatedPrice, Price, ScopeError, ScopeResult,
};

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

pub fn get_price(
    price_info: &AccountInfo,
    dated_price: &DatedPrice,
    clock: &Clock,
) -> ScopeResult<DatedPrice> {
    let price_data: PriceData = account_deserialize(price_info)?;
    let price = redstone_value_to_scope_price(&price_data.value, price_data.decimals)?;

    // Confirm the price is not older than the previously saved one
    let last_price_ts_ms = u64::from_le_bytes(dated_price.generic_data[0..8].try_into().unwrap());

    // Allow same timestamp, but not older.
    // Same timestamp is used when computing Securitize price.
    if price_data.timestamp < last_price_ts_ms {
        warn!("An outdated price was provided");
        return Err(ScopeError::BadTimestamp);
    }

    // Estimate appropriate timestamp
    let Some(write_timestamp_ms) = price_data.write_timestamp else {
        return Err(ScopeError::BadTimestamp);
    };

    let unix_timestamp = (write_timestamp_ms.min(price_data.timestamp) / MILLIS_PER_SECOND)
        .min(clock.unix_timestamp.try_into().unwrap());

    // .. and slot
    let last_updated_slot = price_data
        .write_slot_number
        .min(clock.slot)
        .min(math::estimate_slot_update_from_ts(clock, unix_timestamp));

    // Save the new price timestamp
    let mut generic_data = [0u8; 24];
    generic_data[..8].copy_from_slice(&price_data.timestamp.to_le_bytes());

    Ok(DatedPrice {
        price,
        unix_timestamp,
        last_updated_slot,
        generic_data,
    })
}

pub fn validate_price_account(price_data_account: Option<&AccountInfo>) -> Result<()> {
    let Some(price_data_account) = price_data_account else {
        warn!("No RedStone price account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };
    require_keys_eq!(
        *price_data_account.owner,
        redstone_itf::ID,
        ScopeError::WrongAccountOwner
    );
    let _: PriceData = account_deserialize(price_data_account)?;
    Ok(())
}
