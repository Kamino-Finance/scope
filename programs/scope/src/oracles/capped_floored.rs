use anchor_lang::prelude::*;

use crate::{
    states::oracle_prices::OraclePrices, warn, DatedPrice, ScopeError, ScopeResult, MAX_ENTRIES_U16,
};

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct CappedFlooredData {
    pub source_entry: u16,
    pub cap_entry: Option<u16>,
    pub floor_entry: Option<u16>,
    /// Max age of the source and bound entries.
    pub sources_max_age_s: u64,
}

impl CappedFlooredData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize CappedFlooredData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize CappedFlooredData");
        buff
    }
}

pub fn get_price(
    oracle_prices: &OraclePrices,
    generic_data: &[u8],
    clock: &Clock,
) -> ScopeResult<DatedPrice> {
    let CappedFlooredData {
        source_entry,
        cap_entry,
        floor_entry,
        sources_max_age_s,
    } = CappedFlooredData::from_generic_data(generic_data)?;

    // The returned price will pick up the timestamp and slot of the source price by default
    let mut dated_price = *oracle_prices
        .prices
        .get(usize::from(source_entry))
        .ok_or(ScopeError::CompositeOracleInvalidSourceIndex)?;

    // Handy helper: turn an optional index into an optional dated price,
    // or bail out if the index is invalid.
    let get_dated_price_helper = |entry: Option<u16>| -> ScopeResult<Option<DatedPrice>> {
        entry
            .map(|idx| {
                oracle_prices
                    .prices
                    .get(usize::from(idx))
                    .copied()
                    .ok_or(ScopeError::BadTokenNb)
            })
            .transpose()
    };

    // Optional cap & floor entries
    let cap_dated_price = get_dated_price_helper(cap_entry)?;
    let floor_dated_price = get_dated_price_helper(floor_entry)?;

    let cap_price = cap_dated_price.map(|dated_price| dated_price.price);
    let floor_price = floor_dated_price.map(|dated_price| dated_price.price);

    check_entries_age(
        sources_max_age_s,
        clock,
        (source_entry, dated_price),
        cap_entry.zip(cap_dated_price),
        floor_entry.zip(floor_dated_price),
    )?;

    // Check for the edge case where we have both a floor and a cap price,
    // and the cap price is lower than the floor price
    if let (Some(cap_price), Some(floor_price)) = (cap_price, floor_price) {
        if cap_price < floor_price {
            warn!("CappedFloored: cap price is lower than floor price for token {source_entry}: cap_price={cap_price:?}, floor_price={floor_price:?}",);
            return Err(ScopeError::PriceNotValid);
        }
    }

    // Apply the bounds that do exist
    if let Some(cap) = cap_price {
        dated_price.price = dated_price.price.min(cap);
    }
    if let Some(floor) = floor_price {
        dated_price.price = dated_price.price.max(floor);
    }

    Ok(DatedPrice {
        generic_data: [0; 24],
        ..dated_price
    })
}

/// Rejects the source or any configured bound older than `sources_max_age_s`. A zero max age means
/// the mapping predates this check, which is then skipped until the entry is reconfigured.
fn check_entries_age(
    sources_max_age_s: u64,
    clock: &Clock,
    source: (u16, DatedPrice),
    cap: Option<(u16, DatedPrice)>,
    floor: Option<(u16, DatedPrice)>,
) -> ScopeResult<()> {
    if sources_max_age_s == 0 {
        return Ok(());
    }

    let now: u64 = clock
        .unix_timestamp
        .try_into()
        .expect("Clock is in the past");

    for (entry, dated_price) in [Some(source), cap, floor].into_iter().flatten() {
        let age_s = now.saturating_sub(dated_price.unix_timestamp);
        if age_s > sources_max_age_s {
            warn!(
                "CappedFloored: entry {} is too old (age {}s > max {}s). unix_timestamp = {}, now = {}",
                entry, age_s, sources_max_age_s, dated_price.unix_timestamp, now,
            );
            return Err(ScopeError::CompositeOracleMaxAgeViolated);
        }
    }

    Ok(())
}

pub fn validate_mapping_cfg(
    mapping: Option<&AccountInfo>,
    generic_data: &[u8],
    own_index: u16,
) -> ScopeResult<()> {
    if mapping.is_some() {
        warn!("No mapping account is expected for CappedFloored oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }

    let CappedFlooredData {
        source_entry,
        cap_entry: cap_entry_opt,
        floor_entry: floor_entry_opt,
        sources_max_age_s,
    } = CappedFlooredData::from_generic_data(generic_data)?;

    msg!("Validate CappedFloored price with source_entry = {source_entry}, cap_entry = {cap_entry_opt:?}, floor_entry = {floor_entry_opt:?}, sources_max_age_s = {sources_max_age_s}",);

    if source_entry >= MAX_ENTRIES_U16 {
        warn!("Invalid source index {source_entry} for CappedFloored oracle",);
        return Err(ScopeError::CompositeOracleInvalidSourceIndex);
    }

    // Reject self-reference in any of the entries
    for entry in [Some(source_entry), cap_entry_opt, floor_entry_opt]
        .into_iter()
        .flatten()
    {
        if entry == own_index {
            msg!("Source index {entry} is the entry's own index; self-reference is not allowed");
            return Err(ScopeError::OracleConfigInvalidSourceIndices);
        }
    }

    if let Some(cap_entry) = cap_entry_opt {
        if cap_entry >= MAX_ENTRIES_U16 || cap_entry == source_entry {
            warn!("Invalid cap source index {cap_entry} for CappedFloored oracle, source_entry = {source_entry}",);
            return Err(ScopeError::CompositeOracleInvalidSourceIndex);
        }
    }

    if let Some(floor_entry) = floor_entry_opt {
        if floor_entry >= MAX_ENTRIES_U16 || floor_entry == source_entry {
            warn!("Invalid floor source index {floor_entry} for CappedFloored oracle, source_entry = {source_entry}",);
            return Err(ScopeError::CompositeOracleInvalidSourceIndex);
        }
    }

    if cap_entry_opt.is_none() && floor_entry_opt.is_none() {
        warn!("Can't set both `cap_entry` and `floor_entry` to None");
        return Err(ScopeError::CappedFlooredBothCapAndFloorAreNone);
    }

    // Only entries configured before this field existed may have no max age
    if sources_max_age_s == 0 {
        warn!("Invalid `sources_max_age_s` of 0 for CappedFloored oracle");
        return Err(ScopeError::CompositeOracleInvalidMaxAge);
    }

    Ok(())
}
