use anchor_lang::prelude::*;
use yvaults::scope::MAX_ENTRIES_U16;

use crate::{
    oracles::{debug_format_generic_data, OracleType},
    states::oracle_twaps::{EmaType, TwapEnabledBitmask},
    utils::consts::*,
    ScopeError, MAX_ENTRIES,
};

static_assertions::const_assert_eq!(ORACLE_MAPPING_SIZE, std::mem::size_of::<OracleMappings>());
static_assertions::const_assert_eq!(0, std::mem::size_of::<OracleMappings>() % 8);
#[account(zero_copy)]
#[derive(Debug, AnchorDeserialize)]
pub struct OracleMappings {
    pub price_info_accounts: [Pubkey; MAX_ENTRIES],
    pub price_types: [u8; MAX_ENTRIES],
    pub twap_source: [u16; MAX_ENTRIES], // meaningful only if type == TWAP; the index of where we find the TWAP
    pub twap_enabled_bitmask: [TwapEnabledBitmask; MAX_ENTRIES], // a bitmask determining the types of twaps we want to calculate
    /// reference price against which we check confidence within 5%
    pub ref_price: [u16; MAX_ENTRIES],
    pub generic: [[u8; 20]; MAX_ENTRIES], // generic data parsed depending on oracle type
}

impl OracleMappings {
    pub fn get_entry_type(&self, entry_id: usize) -> Result<OracleType> {
        OracleType::try_from(self.price_types[entry_id])
            .map_err(|_| error!(ScopeError::BadTokenType))
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

    pub fn get_twap_source(&self, entry_id: usize) -> usize {
        usize::from(self.twap_source[entry_id])
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
        self.twap_source[entry_id] = twap_source;
        self.generic[entry_id].fill(0);

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
        self.twap_source[entry_id] = u16::MAX;
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
            d.field("twap_source", &entry_updates.twap_source[entry_id]);
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
