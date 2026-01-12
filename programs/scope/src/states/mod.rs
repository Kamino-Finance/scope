use anchor_lang::prelude::*;

pub mod configuration;
pub mod mints_to_scope_chains;
pub mod oracle_mappings;
pub mod oracle_prices;
pub mod oracle_twaps;
pub mod token_metadatas;
pub use configuration::Configuration;
pub use oracle_mappings::OracleMappings;
pub use oracle_prices::OraclePrices;
pub use oracle_twaps::{EmaTwap, EmaType, OracleTwaps, TwapEnabledBitmask};
pub use token_metadatas::{TokenMetadata, TokenMetadatas};

#[zero_copy]
#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct Price {
    // Pyth price, integer + exponent representation
    // decimal price would be
    // as integer: 6462236900000, exponent: 8
    // as float:   64622.36900000

    // value is the scaled integer
    // for example, 6462236900000 for btc
    pub value: u64,

    // exponent represents the number of decimals
    // for example, 8 for btc
    pub exp: u64,
}

#[zero_copy]
#[derive(Debug, Eq, PartialEq, Default)]
pub struct DatedPrice {
    pub price: Price,
    pub last_updated_slot: u64,
    pub unix_timestamp: u64,
    pub generic_data: [u8; 24],
}
