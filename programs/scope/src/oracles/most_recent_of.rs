use std::{
    cmp::{max, min},
    u64,
};

use anchor_lang::prelude::*;

use crate::{
    states::OraclePrices,
    utils::{consts::FULL_BPS, math},
    warn, DatedPrice, Price, ScopeError, ScopeResult, MAX_ENTRIES_U16,
};

pub const MOST_RECENT_OF_CHAIN_SIZE: usize = 4;

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct MostRecentOfData {
    pub source_entries: [u16; MOST_RECENT_OF_CHAIN_SIZE],
    pub max_divergence_bps: u16,
    pub sources_max_age_s: u64,
}

impl MostRecentOfData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize MostRecentOfData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize MostRecentOfData");
        buff
    }
}

pub fn get_price(
    oracle_prices: &OraclePrices,
    generic_data: &[u8],
    clock: &Clock,
) -> ScopeResult<DatedPrice> {
    let MostRecentOfData {
        source_entries,
        max_divergence_bps,
        sources_max_age_s,
    } = MostRecentOfData::from_generic_data(generic_data)?;

    get_most_recent_price_from_sources(
        oracle_prices,
        &source_entries,
        max_divergence_bps,
        sources_max_age_s,
        clock,
    )
}

/// Helper function to find the most recent price from a list of source entries
/// with age and divergence validation
pub fn get_most_recent_price_from_sources(
    oracle_prices: &OraclePrices,
    source_entries: &[u16],
    max_divergence_bps: u16,
    sources_max_age_s: u64,
    clock: &Clock,
) -> ScopeResult<DatedPrice> {
    let now: u64 = clock
        .unix_timestamp
        .try_into()
        .expect("Clock is in the past");

    let mut min_price = Price {
        value: u64::MAX,
        exp: 0,
    };
    let mut max_price = Price { value: 0, exp: 0 };
    let mut most_recent_price = &DatedPrice::default();

    for dated_price in source_entries
        .iter()
        .filter_map(|&index| oracle_prices.prices.get(usize::from(index)))
    {
        min_price = min(dated_price.price, min_price);
        max_price = max(dated_price.price, max_price);

        if now.saturating_sub(dated_price.unix_timestamp) > sources_max_age_s {
            return Err(ScopeError::MostRecentOfMaxAgeViolated);
        }

        if dated_price.unix_timestamp > most_recent_price.unix_timestamp {
            most_recent_price = dated_price;
        }
    }

    assert_prices_within_max_divergence(min_price, max_price, max_divergence_bps)?;
    Ok(*most_recent_price)
}

fn assert_prices_within_max_divergence(
    smaller: Price,
    greater: Price,
    max_divergence_bps: u16,
) -> ScopeResult<()> {
    // We need to check that (greater - smaller) / smaller < divergence, which is equivalent to
    // (greater - smaller) / divergence < smaller, so we can use the confidence bps variant
    // of math::check_confidence_interval()
    let smaller_dec = decimal_wad::decimal::Decimal::from(smaller);
    let greater_dec = decimal_wad::decimal::Decimal::from(greater);
    let spread = greater_dec - smaller_dec;
    math::check_confidence_interval_decimal_bps(smaller_dec, spread, u32::from(max_divergence_bps))
        .map_err(|_| ScopeError::MostRecentOfMaxDivergenceBpsViolated)
}

/// Helper function to validate common MostRecentOf parameters
pub fn validate_most_recent_of_params(
    source_entries: &[u16],
    max_divergence_bps: u16,
    sources_max_age_s: u64,
) -> ScopeResult<()> {
    // Validate source entries
    if source_entries[0] >= MAX_ENTRIES_U16 {
        return Err(ScopeError::MostRecentOfInvalidSourceIndices);
    }

    // Validate max divergence
    if max_divergence_bps == 0 || max_divergence_bps > FULL_BPS {
        return Err(ScopeError::MostRecentOfInvalidMaxDivergence);
    }

    // Validate max age
    if sources_max_age_s == 0 {
        return Err(ScopeError::MostRecentOfInvalidMaxAge);
    }

    Ok(())
}

pub fn validate_mapping_cfg(mapping: Option<&AccountInfo>, generic_data: &[u8]) -> ScopeResult<()> {
    if mapping.is_some() {
        warn!("No mapping account is expected for MostRecentOf oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }

    let MostRecentOfData {
        source_entries,
        max_divergence_bps,
        sources_max_age_s,
    } = MostRecentOfData::from_generic_data(generic_data)?;

    msg!("Validate MostRecentOf price with source_entries = {source_entries:?}, max_divergence_bps = {max_divergence_bps}, sources_max_age_s = {sources_max_age_s}",);

    validate_most_recent_of_params(&source_entries, max_divergence_bps, sources_max_age_s)
}
