use std::cmp::min;

use anchor_lang::prelude::*;
use decimal_wad::{common::TryMul, decimal::Decimal};

use crate::{
    states::OraclePrices, utils::source_entries::validate_source_entries, warn, DatedPrice, Price,
    ScopeError, ScopeResult, MAX_ENTRIES_U16,
};

/// Max number of source entries for MultiplicationChain.
pub const MULTIPLICATION_CHAIN_SOURCE_ENTRIES_SIZE: usize = 6;

/// Multiply two prices together using Decimal
fn mul_prices(price1: Price, price2: Price) -> ScopeResult<Price> {
    let dec1 = Decimal::from(price1);
    let dec2 = Decimal::from(price2);
    let product = dec1.try_mul(dec2).map_err(|_| ScopeError::MathOverflow)?;
    product.try_into()
}

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct MultiplicationChainData {
    pub source_entries: [u16; MULTIPLICATION_CHAIN_SOURCE_ENTRIES_SIZE],
    pub sources_max_age_s: u64,
}

impl MultiplicationChainData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize MultiplicationChainData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize MultiplicationChainData");
        buff
    }
}

pub fn get_price(
    oracle_prices: &OraclePrices,
    generic_data: &[u8],
    clock: &Clock,
) -> ScopeResult<DatedPrice> {
    let MultiplicationChainData {
        source_entries,
        sources_max_age_s,
    } = MultiplicationChainData::from_generic_data(generic_data)?;

    let now: u64 = clock
        .unix_timestamp
        .try_into()
        .expect("Clock is in the past");

    let mut result_price = Price { value: 1, exp: 0 };
    let mut oldest_timestamp = u64::MAX;
    let mut oldest_slot = u64::MAX;

    for &index in source_entries.iter() {
        // Skip invalid/unused entries (using MAX_ENTRIES_U16 as sentinel)
        if index >= MAX_ENTRIES_U16 {
            break;
        }

        let dated_price = oracle_prices
            .prices
            .get(usize::from(index))
            .ok_or(ScopeError::CompositeOracleInvalidSourceIndex)?;

        if now.saturating_sub(dated_price.unix_timestamp) > sources_max_age_s {
            return Err(ScopeError::CompositeOracleMaxAgeViolated);
        }

        // Track oldest timestamp/slot
        oldest_timestamp = min(oldest_timestamp, dated_price.unix_timestamp);
        oldest_slot = min(oldest_slot, dated_price.last_updated_slot);

        // Multiply prices
        result_price = mul_prices(result_price, dated_price.price)?;
    }

    // Ensure we had at least one valid source
    if oldest_timestamp == u64::MAX {
        warn!("No valid source entries for MultiplicationChain oracle");
        return Err(ScopeError::OracleConfigInvalidSourceIndices);
    }

    Ok(DatedPrice {
        price: result_price,
        unix_timestamp: oldest_timestamp,
        last_updated_slot: oldest_slot,
        ..Default::default()
    })
}

pub fn validate_mapping_cfg(mapping: Option<&AccountInfo>, generic_data: &[u8]) -> ScopeResult<()> {
    if mapping.is_some() {
        warn!("No mapping account is expected for MultiplicationChain oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }

    let MultiplicationChainData {
        source_entries,
        sources_max_age_s,
    } = MultiplicationChainData::from_generic_data(generic_data)?;

    msg!("Validate MultiplicationChain price with source_entries = {source_entries:?}, sources_max_age_s = {sources_max_age_s}",);

    // Validate at least one valid entry, sentinels only at the end
    validate_source_entries(&source_entries)?;

    if sources_max_age_s == 0 {
        return Err(ScopeError::CompositeOracleInvalidMaxAge);
    }

    Ok(())
}
