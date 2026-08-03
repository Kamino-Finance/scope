use anchor_lang::prelude::*;

pub const RESERVE_SIZE: usize = 8616;

#[zero_copy]
#[derive(Debug, PartialEq, Eq)]
pub struct LastUpdate {
    pub slot: u64,
    pub stale: u8,
    pub price_status: u8,
    pub padding: [u8; 6],
}

static_assertions::const_assert_eq!(RESERVE_SIZE, std::mem::size_of::<Reserve>());
static_assertions::const_assert_eq!(0, std::mem::size_of::<Reserve>() % 8);

#[account(zero_copy)]
#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct Reserve {
    pub version: u64,
    pub last_update: LastUpdate,
    pub lending_market: Pubkey,
    // 8 (version) + 16 (last_update) + 32 (lending_market) = 56 bytes of fields
    // 8616 - 56 = 8560 bytes of padding
    pub _padding: [u8; 8560],
}

/// Exchange rate with underlying mint decimals, returned as CPI return data by klend's
/// `calculate_ctoken_exchange_rate` instruction. Must match the definition in klend exactly.
///
/// The klend instruction is fallible and reverts on any error, so this is only ever produced on
/// the success path — there is no error variant in the return data.
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct ExchangeRateWithDecimals {
    pub exchange_rate_sf: u128,
    pub mint_decimals: u8,
}
