use decimal_wad::{decimal::Decimal, error::DecimalError};

use super::{
    dated_price::DatedPrice,
    oracle_twaps::{EmaTwap, EmaType, TwapEnabledBitmask},
};
use crate::{errors::ScopeError, utils::consts::*};

impl From<DecimalError> for ScopeError {
    fn from(err: DecimalError) -> ScopeError {
        match err {
            DecimalError::MathOverflow => ScopeError::IntegerOverflow,
        }
    }
}

// --- Configuration static assertions ---
static_assertions::const_assert_eq!(
    CONFIGURATION_SIZE,
    std::mem::size_of::<super::Configuration>()
);
static_assertions::const_assert_eq!(0, std::mem::size_of::<super::Configuration>() % 8);

// --- OraclePrices static assertions ---
static_assertions::const_assert_eq!(
    ORACLE_PRICES_SIZE,
    std::mem::size_of::<super::OraclePrices>()
);
static_assertions::const_assert_eq!(0, std::mem::size_of::<super::OraclePrices>() % 8);

// --- OracleTwaps static assertions ---
static_assertions::const_assert_eq!(ORACLE_TWAPS_SIZE, std::mem::size_of::<super::OracleTwaps>());
static_assertions::const_assert_eq!(0, std::mem::size_of::<super::OracleTwaps>() % 8);
static_assertions::const_assert_eq!(std::mem::size_of::<TwapEnabledBitmask>(), 1);

// --- OracleMappings static assertions ---
static_assertions::const_assert_eq!(
    ORACLE_MAPPING_SIZE,
    std::mem::size_of::<super::OracleMappings>()
);
static_assertions::const_assert_eq!(0, std::mem::size_of::<super::OracleMappings>() % 8);

// --- TokenMetadatas static assertions ---
static_assertions::const_assert_eq!(
    TOKEN_METADATA_SIZE,
    std::mem::size_of::<super::token_metadatas::TokenMetadatas>()
);
static_assertions::const_assert_eq!(
    0,
    std::mem::size_of::<super::token_metadatas::TokenMetadatas>() % 8
);

// --- EmaTwap::as_dated_price (needs Decimal) ---
impl EmaTwap {
    pub fn as_dated_price(&self, ema_type: EmaType) -> DatedPrice {
        let ema_to_use = match ema_type {
            EmaType::Ema1h => self.current_ema_1h,
            EmaType::Ema8h => self.current_ema_8h,
            EmaType::Ema24h => self.current_ema_24h,
            EmaType::Ema7d => self.current_ema_7d,
        };
        DatedPrice {
            price: Decimal::from_scaled_val(ema_to_use).into(),
            last_updated_slot: self.last_update_slot,
            unix_timestamp: self.last_update_unix_timestamp,
            generic_data: Default::default(),
        }
    }
}

// --- OracleMappings debug helpers ---
use super::{oracle_mappings::OracleMappings, oracle_type::OracleType};
use crate::oracles::debug_format_generic_data;

pub struct DebugPrintMappingEntry<'a> {
    pub entry_id: usize,
    pub entry_updates: &'a OracleMappings,
}

impl OracleMappings {
    pub fn to_debug_print_entry(&self, entry_id: usize) -> DebugPrintMappingEntry {
        DebugPrintMappingEntry {
            entry_id,
            entry_updates: self,
        }
    }
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

// --- TwapEnabledBitmask debug helpers ---
pub struct DebugPrintTwapEnabledBitmaskEntry {
    pub bitmask: TwapEnabledBitmask,
}

impl TwapEnabledBitmask {
    pub fn to_debug_print_entry(&self) -> DebugPrintTwapEnabledBitmaskEntry {
        DebugPrintTwapEnabledBitmaskEntry { bitmask: *self }
    }
}

impl std::fmt::Debug for DebugPrintTwapEnabledBitmaskEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut enabled_types = Vec::new();
        if self.bitmask.is_twap_enabled_for_ema_type(EmaType::Ema1h) {
            enabled_types.push("1h");
        }
        if self.bitmask.is_twap_enabled_for_ema_type(EmaType::Ema8h) {
            enabled_types.push("8h");
        }
        if self.bitmask.is_twap_enabled_for_ema_type(EmaType::Ema24h) {
            enabled_types.push("24h");
        }
        if self.bitmask.is_twap_enabled_for_ema_type(EmaType::Ema7d) {
            enabled_types.push("7d");
        }

        if enabled_types.is_empty() {
            write!(f, "[]")
        } else {
            write!(f, "[{}]", enabled_types.join(", "))
        }
    }
}
