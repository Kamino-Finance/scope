#![allow(clippy::result_large_err)] //Needed because we can't change Anchor result type

pub mod errors;
pub mod program_id;
pub mod states;

pub use anchor_lang;
use anchor_lang::prelude::*;
pub use num_enum;
use num_enum::TryFromPrimitive;
use program_id::PROGRAM_ID;

declare_id!(PROGRAM_ID);

// Note: Need to be directly integer value to not confuse the IDL generator
pub const MAX_ENTRIES_U16: u16 = 512;
// Note: Need to be directly integer value to not confuse the IDL generator
pub const MAX_ENTRIES: usize = 512;

// Re-exports for backward compat
pub use errors::{ScopeError, ScopeResult};
pub use states::{
    token_metadatas::TokenMetadatas as TokensMetadata, Configuration, DatedPrice, EmaTwap, EmaType,
    OracleMappings, OraclePrices, OracleTwaps, OracleType, Price, TokenMetadata,
    TwapEnabledBitmask,
};

#[derive(TryFromPrimitive, PartialEq, Eq, Clone, Copy, Debug)]
#[repr(u64)]
pub enum UpdateTokenMetadataMode {
    Name = 0,
    MaxPriceAgeSlots = 1,
    GroupIds = 2,
}
