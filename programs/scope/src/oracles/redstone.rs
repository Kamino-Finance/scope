use anchor_lang::prelude::*;

use crate::{utils::account_deserialize, warn, DatedPrice, Price, ScopeError};

const REDSTONE_DECIMAL: u64 = 8;

#[cfg(not(feature = "skip_price_validation"))]
const VALID_PRICE_LIFE_TIME: i64 = crate::utils::SECONDS_PER_HOUR;

#[account]
pub struct PriceData {
    pub feed_id: [u8; 32],
    pub value: [u8; 32],
    pub timestamp: u64,
    pub write_timestamp: Option<u64>,
}

fn redstone_value_to_price(raw_be_value: [u8; 32]) -> Result<Price> {
    if !raw_be_value.iter().take(24).all(|&v| v == 0) {
        warn!("Price overflow u64");
        return Err(ScopeError::PriceNotValid.into());
    }

    let value = u64::from_be_bytes(raw_be_value[24..].try_into().unwrap());

    Ok(Price {
        value,
        exp: REDSTONE_DECIMAL,
    })
}

#[cfg(not(feature = "skip_price_validation"))]
fn check_price_life_time(timestamp_ms: Option<u64>, clock: &Clock) -> Result<()> {
    let timestamp_ms = match timestamp_ms {
        Some(ts) => ts,
        None => {
            warn!("Price feed was not updated yet");
            return Err(ScopeError::PriceNotValid.into());
        }
    };

    let ms_since_last_udpate = (clock.unix_timestamp * 1_000).checked_div(timestamp_ms as i64);

    match ms_since_last_udpate {
        Some(ms) if ms <= VALID_PRICE_LIFE_TIME => Ok(()),
        _ => {
            warn!(
                "Redstone price feed account data has not been refreshed for more than: {:?}ms",
                VALID_PRICE_LIFE_TIME
            );
            return Err(ScopeError::PriceNotValid.into());
        }
    }
}

pub fn get_price(price_info: &AccountInfo, _clock: &Clock) -> Result<DatedPrice> {
    let price_data: PriceData = account_deserialize(price_info)?;
    let price = redstone_value_to_price(price_data.value)?;

    #[cfg(not(feature = "skip_price_validation"))]
    check_price_life_time(price_data.write_timestamp, _clock)?;

    Ok(DatedPrice {
        price,
        last_updated_slot: 0, // fix later
        unix_timestamp: price_data.write_timestamp.unwrap(),
        generic_data: Default::default(),
    })
}

pub fn validate_account(price_data: &Option<AccountInfo>) -> Result<()> {
    if cfg!(feature = "skip_price_validation") {
        return Ok(());
    }

    let price_data = match price_data {
        Some(price_data) => price_data,
        None => {
            warn!("No price account provided");
            return Err(ScopeError::PriceNotValid.into());
        }
    };

    let _: PriceData = account_deserialize(price_data)?;
    Ok(())
}
