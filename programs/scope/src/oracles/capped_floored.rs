use anchor_lang::prelude::*;

use crate::{warn, DatedPrice, OraclePrices, Price, ScopeError, ScopeResult, MAX_ENTRIES_U16};

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct CappedFlooredData {
    pub source_entry: u16,
    pub cap_entry: Option<u16>,
    pub floor_entry: Option<u16>,
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

pub fn get_price(oracle_prices: &OraclePrices, generic_data: &[u8]) -> ScopeResult<DatedPrice> {
    let CappedFlooredData {
        source_entry,
        cap_entry,
        floor_entry,
    } = CappedFlooredData::from_generic_data(generic_data)?;

    // The returned price will pick up the timestamp and slot of the source price by default
    let mut dated_price = *oracle_prices
        .prices
        .get(usize::from(source_entry))
        .ok_or(ScopeError::CompositeOracleInvalidSourceIndex)?;

    // Handy helper: turn an optional index into an optional price,
    // or bail out if the index is invalid.
    let get_price_helper = |entry: Option<u16>| -> ScopeResult<Option<Price>> {
        entry
            .map(|idx| {
                oracle_prices
                    .prices
                    .get(usize::from(idx))
                    .map(|dated_price| dated_price.price)
                    .ok_or(ScopeError::BadTokenNb)
            })
            .transpose()
    };

    // Optional cap & floor prices
    let cap_price = get_price_helper(cap_entry)?;
    let floor_price = get_price_helper(floor_entry)?;

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

pub fn validate_mapping_cfg(mapping: &Option<AccountInfo>, generic_data: &[u8]) -> ScopeResult<()> {
    if mapping.is_some() {
        warn!("No mapping account is expected for CappedFloored oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }

    let CappedFlooredData {
        source_entry,
        cap_entry: cap_entry_opt,
        floor_entry: floor_entry_opt,
    } = CappedFlooredData::from_generic_data(generic_data)?;

    msg!("Validate CappedFloored price with source_entry = {source_entry}, cap_entry = {cap_entry_opt:?}, floor_entry = {floor_entry_opt:?}",);

    if source_entry >= MAX_ENTRIES_U16 {
        warn!("Invalid source index {source_entry} for CappedFloored oracle",);
        return Err(ScopeError::CompositeOracleInvalidSourceIndex);
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

        if let Some(cap_entry) = cap_entry_opt {
            if floor_entry == cap_entry {
                warn!("Invalid floor source index {floor_entry} and cap source index {cap_entry} for CappedFloored oracle",);
                return Err(ScopeError::CompositeOracleInvalidSourceIndex);
            }
        }
    }

    if cap_entry_opt.is_none() && floor_entry_opt.is_none() {
        warn!("Can't set both `cap_entry` and `floor_entry` to None");
        return Err(ScopeError::CappedFlooredBothCapAndFloorAreNone);
    }

    Ok(())
}
