use std::u64;

use anchor_lang::prelude::*;

use crate::{
    states::OraclePrices,
    utils::{
        consts::{FULL_BPS, SOURCE_ENTRIES_CHAIN_SIZE},
        math,
        source_entries::validate_source_entries,
    },
    warn, DatedPrice, Price, ScopeError, ScopeResult, MAX_ENTRIES_U16,
};

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct MostRecentOfData {
    pub source_entries: [u16; SOURCE_ENTRIES_CHAIN_SIZE],
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
    let mut min_price_index: u16 = MAX_ENTRIES_U16;
    let mut max_price_index: u16 = MAX_ENTRIES_U16;
    let mut most_recent_price = &DatedPrice::default();

    for &index in source_entries.iter() {
        let Some(dated_price) = oracle_prices.prices.get(usize::from(index)) else {
            continue;
        };

        if dated_price.price < min_price {
            min_price = dated_price.price;
            min_price_index = index;
        }
        if dated_price.price > max_price {
            max_price = dated_price.price;
            max_price_index = index;
        }

        if now.saturating_sub(dated_price.unix_timestamp) > sources_max_age_s {
            warn!(
                "MostRecentOf: source entry {} is too old (age {}s > max {}s). unix_timestamp = {}, now = {}",
                index,
                now.saturating_sub(dated_price.unix_timestamp),
                sources_max_age_s,
                dated_price.unix_timestamp,
                now,
            );
            return Err(ScopeError::CompositeOracleMaxAgeViolated);
        }

        if dated_price.unix_timestamp > most_recent_price.unix_timestamp {
            most_recent_price = dated_price;
        }
    }

    assert_prices_within_max_divergence(
        min_price,
        min_price_index,
        max_price,
        max_price_index,
        max_divergence_bps,
    )?;
    Ok(*most_recent_price)
}

fn assert_prices_within_max_divergence(
    smaller: Price,
    smaller_index: u16,
    greater: Price,
    greater_index: u16,
    max_divergence_bps: u16,
) -> ScopeResult<()> {
    // We need to check that (greater - smaller) / smaller < divergence, which is equivalent to
    // (greater - smaller) / divergence < smaller, so we can use the confidence bps variant
    // of math::check_confidence_interval()
    let smaller_dec = decimal_wad::decimal::Decimal::from(smaller);
    let greater_dec = decimal_wad::decimal::Decimal::from(greater);
    let spread = greater_dec - smaller_dec;
    math::check_confidence_interval_decimal_bps(smaller_dec, spread, u32::from(max_divergence_bps))
        .map_err(|_| {
            warn!(
                "MostRecentOf: max divergence of {} bps violated. Smallest price (entry {}): value = {}, exp = {}. Greatest price (entry {}): value = {}, exp = {}. Spread = {}",
                max_divergence_bps,
                smaller_index,
                smaller.value,
                smaller.exp,
                greater_index,
                greater.value,
                greater.exp,
                spread,
            );
            ScopeError::MostRecentOfMaxDivergenceBpsViolated
        })
}

/// Helper function to validate common MostRecentOf parameters
pub fn validate_most_recent_of_params(
    source_entries: &[u16],
    max_divergence_bps: u16,
    sources_max_age_s: u64,
) -> ScopeResult<()> {
    // Validate at least one valid entry, sentinels only at the end
    validate_source_entries(source_entries)?;

    // Validate max divergence
    if max_divergence_bps == 0 || max_divergence_bps > FULL_BPS {
        return Err(ScopeError::MostRecentOfInvalidMaxDivergence);
    }

    // Validate max age
    if sources_max_age_s == 0 {
        return Err(ScopeError::CompositeOracleInvalidMaxAge);
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
