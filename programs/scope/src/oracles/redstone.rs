use core::mem::size_of;

use anchor_lang::prelude::*;

use crate::{utils::account_deserialize, warn, DatedPrice, Price, ScopeError};

#[cfg(not(feature = "skip_price_validation"))]
/// Price is kept in 0.2% or 0.05% deviation threshold, depending on the feed, all the time.
/// At least one update per 30h will happen.
const VALID_PRICE_LIFETIME: i64 = 30 * crate::utils::SECONDS_PER_HOUR;

const MS_PER_SECONDS: u64 = 1_000;
const RESERVED_BYTE_SIZE: usize = 64;

#[account]
struct PriceData {
    pub feed_id: [u8; 32],
    pub value: [u8; 32],
    pub timestamp: u64,
    pub write_timestamp: Option<u64>,
    pub write_slot_number: u64,
    pub decimals: u8,
    pub _reserved: [u8; RESERVED_BYTE_SIZE],
}

fn redstone_value_to_price(raw_be_value: [u8; 32], decimals: u8) -> Result<Price> {
    let u64_start_index = 32 - size_of::<u64>();

    if !raw_be_value.iter().take(u64_start_index).all(|&v| v == 0) {
        warn!("Price overflow u64");
        return Err(ScopeError::PriceNotValid.into());
    }

    let value = u64::from_be_bytes(raw_be_value[u64_start_index..].try_into().unwrap());

    Ok(Price {
        value,
        exp: decimals as u64,
    })
}

#[cfg(not(feature = "skip_price_validation"))]
fn check_price_lifetime(timestamp_ms: Option<u64>, clock: &Clock) -> Result<()> {
    let timestamp_ms = match timestamp_ms {
        Some(ts) => ts,
        None => {
            warn!("Price feed was not updated yet");
            return Err(ScopeError::PriceNotValid.into());
        }
    };

    let ms_since_last_udpate =
        (clock.unix_timestamp * MS_PER_SECONDS).checked_div(timestamp_ms as i64);

    match ms_since_last_udpate {
        Some(ms) if ms <= VALID_PRICE_LIFETIME => Ok(()),
        _ => {
            warn!(
                "RedStone price feed account data has not been refreshed for more than: {:?}ms",
                VALID_PRICE_LIFETIME
            );
            return Err(ScopeError::PriceNotValid.into());
        }
    }
}

pub fn get_price(price_info: &AccountInfo, _clock: &Clock) -> Result<DatedPrice> {
    let price_data: PriceData = account_deserialize(price_info)?;
    let price = redstone_value_to_price(price_data.value, price_data.decimals)?;

    #[cfg(not(feature = "skip_price_validation"))]
    check_price_lifetime(price_data.write_timestamp, _clock)?;

    Ok(DatedPrice {
        price,
        last_updated_slot: price_data.write_slot_number,
        unix_timestamp: price_data
            .write_timestamp
            .expect("Checked in `check_price_life_time`")
            / MS_PER_SECONDS,
        generic_data: Default::default(),
    })
}

pub fn validate_account(price_data: &Option<AccountInfo>) -> Result<()> {
    if cfg!(feature = "skip_price_validation") {
        return Ok(());
    }

    let price_data_account = match price_data {
        Some(price_data_account) => price_data_account,
        None => {
            warn!("No price account provided");
            return Err(ScopeError::PriceNotValid.into());
        }
    };

    let _: PriceData = account_deserialize(price_data_account)?;
    Ok(())
}
