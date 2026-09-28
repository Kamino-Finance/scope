pub mod accounts;

use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    },
};

#[cfg(feature = "staging")]
declare_id!("sCanpzodQ7MEC7mpi7ehMtYEkbindzjJQE3JQMAo7fR");

// The mainnet deployment, live since 2026-08-27; canary-private declares the same id per
// cluster (canary-private#32).
#[cfg(not(feature = "staging"))]
declare_id!("CanarFxHDSnbrPmrE79Qq6hL2p7ZMyyV4ZLTKQ6g7tpK");

/// Anchor `global:get_price` discriminator (`sha256("global:get_price")[0..8]`).
pub const GET_PRICE_DISCRIMINATOR: [u8; 8] = [238, 38, 193, 106, 228, 32, 210, 33];

/// Price returned as Canary `get_price` CPI return data. `value` is a u128 mantissa at `10^-exp`.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub struct PriceData {
    pub value: u128,
    pub unix_timestamp: u64,
    pub exp: u8,
}

/// Account-only `get_price`: one `price_feed` account, discriminator-only instruction data.
pub fn get_price(program_id: &Pubkey, price_feed: &Pubkey) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: vec![AccountMeta::new_readonly(*price_feed, false)],
        data: GET_PRICE_DISCRIMINATOR.to_vec(),
    }
}
