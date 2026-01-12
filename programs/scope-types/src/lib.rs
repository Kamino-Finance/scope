#![allow(clippy::result_large_err)] //Needed because we can't change Anchor result type

pub mod program_id;

// Reexports to deal with eventual conflicts
// Local use
use std::num::TryFromIntError;

pub use anchor_lang;
use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
pub use num_enum;
use num_enum::{IntoPrimitive, TryFromPrimitive, TryFromPrimitiveError};
use program_id::PROGRAM_ID;

declare_id!(PROGRAM_ID);

// Note: Need to be directly integer value to not confuse the IDL generator
pub const MAX_ENTRIES_U16: u16 = 512;
// Note: Need to be directly integer value to not confuse the IDL generator
pub const MAX_ENTRIES: usize = 512;

#[zero_copy]
#[derive(Debug, Eq, PartialEq, Default)]
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
#[derive(Debug, Eq, PartialEq)]
pub struct DatedPrice {
    pub price: Price,
    pub last_updated_slot: u64,
    pub unix_timestamp: u64,
    pub _reserved: [u64; 2],
    pub _reserved2: [u16; 3],
    // Current index of the dated price.
    pub index: u16,
}

impl Default for DatedPrice {
    fn default() -> Self {
        Self {
            price: Default::default(),
            last_updated_slot: Default::default(),
            unix_timestamp: Default::default(),
            _reserved: Default::default(),
            _reserved2: Default::default(),
            index: MAX_ENTRIES_U16,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, TryFromPrimitive, IntoPrimitive)]
#[repr(usize)]
pub enum EmaType {
    Ema1h,
    Ema8h,
    Ema24h,
}

// Account to store dated TWAP prices
#[account(zero_copy)]
pub struct OracleTwaps {
    pub oracle_prices: Pubkey,
    pub oracle_mappings: Pubkey,
    pub twaps: [EmaTwap; MAX_ENTRIES],
}

// Account to store dated prices
#[account(zero_copy)]
pub struct OraclePrices {
    pub oracle_mappings: Pubkey,
    pub prices: [DatedPrice; MAX_ENTRIES],
}

#[zero_copy]
#[derive(Debug, Eq, PartialEq)]
pub struct EmaTwap {
    pub last_update_slot: u64, // the slot when the last observation was added
    pub last_update_unix_timestamp: u64,

    pub current_ema_1h: u128,
    pub current_ema_8h: u128,
    pub current_ema_24h: u128,

    pub padding: [u128; 38],
}

impl Default for EmaTwap {
    fn default() -> Self {
        Self {
            current_ema_1h: 0,
            current_ema_8h: 0,
            current_ema_24h: 0,
            last_update_slot: 0,
            last_update_unix_timestamp: 0,
            padding: [0_u128; 38],
        }
    }
}

#[derive(Debug, Clone, Copy, AnchorDeserialize, Zeroable, Pod, PartialEq)]
#[repr(C)]
pub struct TwapEnabledBitmask {
    pub bitmask: u8,
}

impl TwapEnabledBitmask {
    pub const fn new() -> Self {
        Self { bitmask: 0 }
    }

    pub fn is_twap_enabled(&self) -> bool {
        self.bitmask > 0
    }

    pub fn is_twap_enabled_for_ema_type(&self, ema_type: EmaType) -> bool {
        let ema_type: usize = ema_type.into();
        self.bitmask & (1 << ema_type) > 0
    }
}

impl From<u8> for TwapEnabledBitmask {
    fn from(bitmask: u8) -> Self {
        Self { bitmask }
    }
}

impl From<TwapEnabledBitmask> for u8 {
    fn from(val: TwapEnabledBitmask) -> Self {
        val.bitmask
    }
}

// Accounts holding source of prices
#[account(zero_copy)]
pub struct OracleMappings {
    pub price_info_accounts: [Pubkey; MAX_ENTRIES],
    pub price_types: [u8; MAX_ENTRIES],
    pub twap_source: [u16; MAX_ENTRIES], // meaningful only if type == TWAP; the index of where we find the TWAP
    pub twap_enabled_bitmask: [TwapEnabledBitmask; MAX_ENTRIES], // true or false
    pub _reserved1: [u8; MAX_ENTRIES],
    pub _reserved2: [u32; MAX_ENTRIES],
}

impl OracleMappings {
    pub fn is_twap_enabled(&self, entry_id: usize) -> bool {
        self.twap_enabled_bitmask[entry_id].is_twap_enabled()
    }

    pub fn is_twap_enabled_for_ema_type(&self, entry_id: usize, ema_type: EmaType) -> bool {
        self.twap_enabled_bitmask[entry_id].is_twap_enabled_for_ema_type(ema_type)
    }

    pub fn get_twap_source(&self, entry_id: usize) -> usize {
        usize::from(self.twap_source[entry_id])
    }
}

// Configuration account of the program
#[account(zero_copy)]
pub struct Configuration {
    pub admin: Pubkey,
    pub oracle_mappings: Pubkey,
    pub oracle_prices: Pubkey,
    pub tokens_metadata: Pubkey,
    pub oracle_twaps: Pubkey,
    _padding: [u64; 1259],
}

#[account(zero_copy)]
pub struct TokensMetadata {
    pub metadatas_array: [TokenMetadata; MAX_ENTRIES],
}

#[zero_copy]
#[derive(Debug, PartialEq, Eq, Default)]
pub struct TokenMetadata {
    pub name: [u8; 32],
    pub max_age_price_slots: u64,
    pub group_ids_bitset: u64, // a bitset of group IDs in range [0, 64).
    pub _reserved: [u64; 15],
}

#[derive(TryFromPrimitive, PartialEq, Eq, Clone, Copy, Debug)]
#[repr(u64)]
pub enum UpdateTokenMetadataMode {
    Name = 0,
    MaxPriceAgeSlots = 1,
    GroupIds = 2,
}

#[error_code]
#[derive(PartialEq, Eq, TryFromPrimitive)]
pub enum ScopeError {
    #[msg("Integer overflow")]
    IntegerOverflow,

    #[msg("Conversion failure")]
    ConversionFailure,

    #[msg("Mathematical operation with overflow")]
    MathOverflow,

    #[msg("Out of range integral conversion attempted")]
    OutOfRangeIntegralConversion,

    #[msg("Unexpected account in instruction")]
    UnexpectedAccount,

    #[msg("Price is not valid")]
    PriceNotValid,

    #[msg("The number of tokens is different from the number of received accounts")]
    AccountsAndTokenMismatch,

    #[msg("The token index received is out of range")]
    BadTokenNb,

    #[msg("The token type received is invalid")]
    BadTokenType,

    #[msg("There was an error with the Switchboard V2 retrieval")]
    SwitchboardV2Error,

    #[msg("Invalid account discriminator")]
    InvalidAccountDiscriminator,

    #[msg("Unable to deserialize account")]
    UnableToDeserializeAccount,

    #[msg("Error while computing price with ScopeChain")]
    BadScopeChainOrPrices,

    #[msg("Refresh price instruction called in a CPI")]
    RefreshInCPI,

    #[msg("Refresh price instruction preceded by unexpected ixs")]
    RefreshWithUnexpectedIxs,
}

impl<T> From<TryFromPrimitiveError<T>> for ScopeError
where
    T: TryFromPrimitive,
{
    fn from(_: TryFromPrimitiveError<T>) -> Self {
        ScopeError::ConversionFailure
    }
}

impl From<TryFromIntError> for ScopeError {
    fn from(_: TryFromIntError) -> Self {
        ScopeError::OutOfRangeIntegralConversion
    }
}
