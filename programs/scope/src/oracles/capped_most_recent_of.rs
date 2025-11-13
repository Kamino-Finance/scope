use anchor_lang::prelude::*;

use crate::{
    oracles::most_recent_of::{
        get_most_recent_price_from_sources, validate_most_recent_of_params,
        MOST_RECENT_OF_CHAIN_SIZE,
    },
    states::OraclePrices,
    warn, DatedPrice, ScopeError, ScopeResult, MAX_ENTRIES_U16,
};

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct CappedMostRecentOfData {
    pub source_entries: [u16; MOST_RECENT_OF_CHAIN_SIZE],
    pub max_divergence_bps: u16,
    pub sources_max_age_s: u64,
    pub cap_entry: u16,
}

impl CappedMostRecentOfData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize CappedMostRecentOfData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize CappedMostRecentOfData");
        buff
    }
}

pub fn get_price(
    oracle_prices: &OraclePrices,
    generic_data: &[u8],
    clock: &Clock,
) -> ScopeResult<DatedPrice> {
    let CappedMostRecentOfData {
        source_entries,
        max_divergence_bps,
        sources_max_age_s,
        cap_entry,
    } = CappedMostRecentOfData::from_generic_data(generic_data)?;

    // Get the most recent price from source entries
    let mut result_price = get_most_recent_price_from_sources(
        oracle_prices,
        &source_entries,
        max_divergence_bps,
        sources_max_age_s,
        clock,
    )?;

    // Apply cap
    let cap_price = oracle_prices
        .prices
        .get(usize::from(cap_entry))
        .ok_or(ScopeError::CompositeOracleInvalidSourceIndex)?
        .price;

    result_price.price = result_price.price.min(cap_price);

    Ok(DatedPrice {
        generic_data: [0; 24],
        ..result_price
    })
}

pub fn validate_mapping_cfg(mapping: Option<&AccountInfo>, generic_data: &[u8]) -> ScopeResult<()> {
    if mapping.is_some() {
        warn!("No mapping account is expected for CappedMostRecentOf oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }

    let CappedMostRecentOfData {
        source_entries,
        max_divergence_bps,
        sources_max_age_s,
        cap_entry,
    } = CappedMostRecentOfData::from_generic_data(generic_data)?;

    msg!("Validate CappedMostRecentOf price with source_entries = {source_entries:?}, max_divergence_bps = {max_divergence_bps}, sources_max_age_s = {sources_max_age_s}, cap_entry = {cap_entry}",);

    // Validate common MostRecentOf parameters using shared helper
    validate_most_recent_of_params(&source_entries, max_divergence_bps, sources_max_age_s)?;

    // Validate cap entry
    if cap_entry >= MAX_ENTRIES_U16 {
        warn!("Invalid cap source index {cap_entry} for CappedMostRecentOf oracle");
        return Err(ScopeError::CompositeOracleInvalidSourceIndex);
    }

    // Cap entry should not be the same as any source entry
    if source_entries.contains(&cap_entry) {
        warn!("Cap source index {cap_entry} cannot be the same as any source entry for CappedMostRecentOf oracle");
        return Err(ScopeError::CompositeOracleInvalidSourceIndex);
    }

    Ok(())
}
