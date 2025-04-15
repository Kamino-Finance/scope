//! DiscountToMaturity oracle. Price at a discount based on a fixed yield to maturity.
use std::convert::TryInto;

use anchor_lang::prelude::*;

use crate::{
    utils::{
        consts::{FULL_BPS, SECONDS_PER_YEAR},
        math::ten_pow,
    },
    warn, DatedPrice, Price, ScopeError, ScopeResult,
};

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct DiscountToMaturityData {
    pub discount_per_year_bps: u16,
    pub maturity_timestamp: i64,
}

impl DiscountToMaturityData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize DiscountToMaturityData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut cursor = &mut buff[..];
        self.serialize(&mut cursor)
            .expect("Failed to serialize DiscountToMaturityData");
        buff
    }
}

const PRICE_DECIMALS: u8 = 9;

pub fn get_price(cfg_raw: &[u8], clock: &Clock) -> Result<DatedPrice> {
    let DiscountToMaturityData {
        discount_per_year_bps,
        maturity_timestamp,
    } = DiscountToMaturityData::from_generic_data(cfg_raw)?;

    let time_left_s = time_left_s(maturity_timestamp, clock);

    let price = get_discounted_price(time_left_s, discount_per_year_bps)?;

    Ok(DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: clock
            .unix_timestamp
            .try_into()
            .expect("Clock is in the past"),
        ..Default::default()
    })
}

pub fn validate_mapping_cfg(
    mapping: &Option<AccountInfo>,
    cfg_raw: &[u8],
    clock: &Clock,
) -> ScopeResult<()> {
    if mapping.is_some() {
        warn!("Mapping account is not expected for DiscountToMaturity oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }
    let DiscountToMaturityData {
        discount_per_year_bps,
        maturity_timestamp,
    } = DiscountToMaturityData::from_generic_data(cfg_raw)?;

    msg!(
        "Validate DiscountToMaturity price with discount per year set to {discount_per_year_bps} bps and expire timestamp set to {maturity_timestamp}",
    );

    let time_left_s = time_left_s(maturity_timestamp, clock);
    if u128::from(time_left_s) * u128::from(discount_per_year_bps)
        > u128::from(FULL_BPS) * u128::from(SECONDS_PER_YEAR)
    {
        msg!("Discount per year is too high for the remaining time");
        return Err(ScopeError::InvalidGenericData);
    }
    Ok(())
}

fn time_left_s(maturity_timestamp: i64, clock: &Clock) -> u64 {
    let time_left_s = maturity_timestamp.saturating_sub(clock.unix_timestamp);
    time_left_s.try_into().unwrap_or(0)
}

fn get_discounted_price(time_left_s: u64, discount_per_year_bps: u16) -> ScopeResult<Price> {
    if time_left_s == 0 {
        return Ok(Price { value: 1, exp: 0 });
    }
    let nine_dec = ten_pow(PRICE_DECIMALS);
    let discount: u128 = nine_dec * u128::from(time_left_s) * u128::from(discount_per_year_bps)
        / (u128::from(FULL_BPS) * u128::from(SECONDS_PER_YEAR));
    let price = nine_dec.saturating_sub(discount);
    Ok(Price {
        value: price.try_into().map_err(|_| {
            msg!("Overflow while computing discounted price");
            ScopeError::IntegerOverflow
        })?,
        exp: PRICE_DECIMALS.into(),
    })
}
