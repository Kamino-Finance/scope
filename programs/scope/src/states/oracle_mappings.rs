use anchor_lang::prelude::*;
use yvaults::scope::MAX_ENTRIES_U16;

use crate::{
    oracles::{debug_format_generic_data, OracleType},
    states::oracle_twaps::{EmaType, TwapEnabledBitmask},
    utils::consts::*,
    ScopeError, ScopeResult, MAX_ENTRIES,
};

static_assertions::const_assert_eq!(ORACLE_MAPPING_SIZE, std::mem::size_of::<OracleMappings>());
static_assertions::const_assert_eq!(0, std::mem::size_of::<OracleMappings>() % 8);
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
        self.twap_source_or_ref_price_tolerance_bps[entry_id] = twap_source;
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
        if !is_twap {
            self.twap_source_or_ref_price_tolerance_bps[entry_id] =
                ref_price_tolerance_bps.unwrap_or(u16::MAX);
            Ok(())
        } else {
            Err(ScopeError::OperationNotSupported)
        }
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

    pub fn to_debug_print_entry(&self, entry_id: usize) -> DebugPrintMappingEntry {
        DebugPrintMappingEntry {
            entry_id,
            entry_updates: self,
        }
    }
}

pub struct DebugPrintMappingEntry<'a> {
    pub entry_id: usize,
    pub entry_updates: &'a OracleMappings,
}

impl std::fmt::Debug for DebugPrintMappingEntry<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let entry_id = self.entry_id;
        let entry_updates = self.entry_updates;

        let pk = entry_updates.price_info_accounts[entry_id];
        let price_type = entry_updates.get_entry_type(entry_id).ok();
        let twap_enabled_bitmask = entry_updates.twap_enabled_bitmask[entry_id];

        let ref_price = entry_updates.ref_price[entry_id];
        let generic_data = entry_updates.generic[entry_id];

        let mut d = f.debug_struct("OracleMappingEntry");
        d.field("entry_id", &entry_id)
            .field("price_type", &price_type);

        if price_type
            .map(OracleType::is_chainlink_provider)
            .unwrap_or(false)
        {
            d.field(
                "chainlink_feed_id",
                &chainlink_streams_report::feed_id::ID(pk.to_bytes()).to_hex_string(),
            );
        } else if price_type.map(OracleType::is_twap).unwrap_or(false) {
            d.field(
                "twap_source",
                &entry_updates.twap_source_or_ref_price_tolerance_bps[entry_id],
            );
        } else {
            d.field("price_info_account", &pk);
        }

        if let Some(price_type) = price_type {
            debug_format_generic_data(&mut d, price_type, &generic_data);
        } else {
            d.field("generic_data", &generic_data);
        }

        d.field("twap_enabled", &twap_enabled_bitmask.to_debug_print_entry())
            .field(
                "ref_price_index",
                if ref_price == u16::MAX {
                    &"None"
                } else {
                    &ref_price
                },
            )
            .finish()
    }
}
