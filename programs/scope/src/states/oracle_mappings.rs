use anchor_lang::prelude::*;

use super::{
    oracle_twaps::{EmaType, TwapEnabledBitmask},
    oracle_type::OracleType,
};
use crate::{
    errors::{ScopeError, ScopeResult},
    MAX_ENTRIES, MAX_ENTRIES_U16,
};

#[account(zero_copy)]
#[derive(Debug, AnchorDeserialize)]
pub struct OracleMappings {
    pub price_info_accounts: [Pubkey; MAX_ENTRIES],
    pub price_types: [u8; MAX_ENTRIES],
    pub twap_source_or_ref_price_tolerance_bps: [u16; MAX_ENTRIES], //if type == TWAP, then is the index of where we find the TWAP; otherwise, is the tolerance bps for ref price check
    pub twap_enabled_bitmask: [TwapEnabledBitmask; MAX_ENTRIES], // a bitmask determining the types of twaps we want to calculate
    pub ref_price: [u16; MAX_ENTRIES],
    pub generic: [[u8; 20]; MAX_ENTRIES], // generic data parsed depending on oracle type
}

pub enum RefPriceToleranceOrTwapSource {
    None,
    RefPriceToleranceBps(u16),
    TwapSource(u16),
}

impl OracleMappings {
    pub fn get_entry_type(&self, entry_id: usize) -> ScopeResult<OracleType> {
        OracleType::try_from(self.price_types[entry_id]).map_err(|_| ScopeError::BadTokenType)
    }

    pub fn is_twap(&self, entry_id: usize) -> ScopeResult<bool> {
        let price_type = self.get_entry_type(entry_id)?;
        Ok(price_type.is_twap())
    }

    pub fn is_twap_enabled(&self, entry_id: usize) -> bool {
        self.twap_enabled_bitmask[entry_id].is_twap_enabled()
    }

    pub fn is_twap_enabled_for_ema_type(&self, entry_id: usize, ema_type: EmaType) -> bool {
        self.twap_enabled_bitmask[entry_id].is_twap_enabled_for_ema_type(ema_type)
    }

    pub fn get_twap_enabled_bitmask(&self, entry_id: usize) -> TwapEnabledBitmask {
        self.twap_enabled_bitmask[entry_id]
    }

    pub fn set_twap_enabled_bitmask(
        &mut self,
        entry_id: usize,
        twap_enabled_bitmask: TwapEnabledBitmask,
    ) {
        self.twap_enabled_bitmask[entry_id] = twap_enabled_bitmask;
    }

    fn get_twap_source_or_ref_price_tolerance_bps(
        &self,
        entry_id: usize,
    ) -> ScopeResult<RefPriceToleranceOrTwapSource> {
        let tolerance_or_source = self.twap_source_or_ref_price_tolerance_bps[entry_id];
        let is_twap = self.is_twap(entry_id)?;
        if is_twap {
            if tolerance_or_source >= MAX_ENTRIES_U16 {
                Err(ScopeError::TwapSourceIndexOutOfRange)
            } else {
                Ok(RefPriceToleranceOrTwapSource::TwapSource(
                    tolerance_or_source,
                ))
            }
        } else if self.ref_price[entry_id] < MAX_ENTRIES_U16 {
            match tolerance_or_source {
                u16::MAX => Ok(RefPriceToleranceOrTwapSource::None),
                _ => Ok(RefPriceToleranceOrTwapSource::RefPriceToleranceBps(
                    tolerance_or_source,
                )),
            }
        } else {
            Ok(RefPriceToleranceOrTwapSource::None)
        }
    }
    pub fn get_twap_source(&self, entry_id: usize) -> Option<usize> {
        self.get_twap_source_or_ref_price_tolerance_bps(entry_id)
            .ok()
            .and_then(|source| match source {
                RefPriceToleranceOrTwapSource::TwapSource(index) => Some(usize::from(index)),
                _ => None,
            })
    }

    pub fn set_twap_source(
        &mut self,
        entry_id: usize,
        new_twap_type: OracleType,
        twap_source: u16,
    ) -> Result<()> {
        require_gt!(
            MAX_ENTRIES_U16,
            twap_source,
            ScopeError::TwapSourceIndexOutOfRange
        );
        self.price_info_accounts[entry_id] = crate::ID;
        self.price_types[entry_id] = new_twap_type.into();
        self.twap_source_or_ref_price_tolerance_bps[entry_id] = twap_source;
        self.generic[entry_id].fill(0);
        Ok(())
    }

    pub fn get_ref_price_tolerance_bps(&self, entry_id: usize) -> Option<u16> {
        self.get_twap_source_or_ref_price_tolerance_bps(entry_id)
            .ok()
            .and_then(|tolerance| match tolerance {
                RefPriceToleranceOrTwapSource::RefPriceToleranceBps(bps) => Some(bps),
                _ => None,
            })
    }

    pub fn set_ref_price_tolerance_bps(
        &mut self,
        entry_id: usize,
        ref_price_tolerance_bps: Option<u16>,
    ) -> ScopeResult<()> {
        let is_twap = self.is_twap(entry_id)?;
        if is_twap {
            // For TWAP entries, the field stores the twap source, not the tolerance.
            // Setting a tolerance on a TWAP entry is not supported, but None is a no-op.
            if ref_price_tolerance_bps.is_some() {
                return Err(ScopeError::OperationNotSupported);
            }
        } else {
            self.twap_source_or_ref_price_tolerance_bps[entry_id] =
                ref_price_tolerance_bps.unwrap_or(u16::MAX);
        }
        Ok(())
    }

    pub fn is_entry_used(&self, entry_id: usize) -> bool {
        self.price_types[entry_id] != 0 || self.price_info_accounts[entry_id] != Pubkey::default()
    }

    pub fn get_entry_mapping_pk(&self, entry_id: usize) -> Option<Pubkey> {
        let pk = self.price_info_accounts[entry_id];
        if pk == Pubkey::default() || pk == crate::ID {
            None
        } else {
            Some(pk)
        }
    }

    pub fn reset_entry(&mut self, entry_id: usize) {
        self.price_info_accounts[entry_id] = Pubkey::default();
        self.price_types[entry_id] = 0;
        self.twap_enabled_bitmask[entry_id] = TwapEnabledBitmask::new();
        self.twap_source_or_ref_price_tolerance_bps[entry_id] = u16::MAX;
        self.ref_price[entry_id] = u16::MAX;
        self.generic[entry_id].fill(0);
    }

    pub fn set_entry_mapping(
        &mut self,
        entry_id: usize,
        price_info: Option<Pubkey>,
        price_type: OracleType,
        generic_data: [u8; 20],
    ) {
        self.price_info_accounts[entry_id] = price_info.unwrap_or(crate::ID);
        self.price_types[entry_id] = price_type.into();
        self.generic[entry_id] = generic_data;
    }

    pub fn get_ref_price(&self, entry_id: usize) -> Option<u16> {
        let raw_ref_price = self.ref_price[entry_id];
        if raw_ref_price == u16::MAX {
            None
        } else {
            Some(raw_ref_price)
        }
    }

    pub fn set_ref_price(&mut self, entry_id: usize, ref_price_index: Option<u16>) {
        self.ref_price[entry_id] = ref_price_index.unwrap_or(u16::MAX);
    }
}
