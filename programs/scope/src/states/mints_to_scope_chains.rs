use std::mem::size_of;

use anchor_lang::prelude::*;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Map of mints to scope chain only valid for a given price feed
#[derive(Default)]
#[account]
pub struct MintsToScopeChains {
    pub oracle_prices: Pubkey,
    pub seed_pk: Pubkey,
    pub seed_id: u64,
    pub bump: u8,
    pub mapping: Vec<MintToScopeChain>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Default, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MintToScopeChain {
    #[cfg_attr(feature = "serde", serde(with = "serde_string"))] // Use bs58 for serialization
    pub mint: Pubkey,
    pub scope_chain: [u16; 4],
}

impl MintsToScopeChains {
    pub const fn size_from_len(len: usize) -> usize {
        const MINT_TO_SCOPE_CHAIN_SERIALIZED_SIZE: usize =
            size_of::<Pubkey>() + size_of::<[u16; 4]>();

        size_of::<Pubkey>() // oracle_prices
            + size_of::<Pubkey>() // seed_pk
            + size_of::<u64>() // seed_id
            + size_of::<u8>() // bump
            + size_of::<u32>() // Vec length
            + len * MINT_TO_SCOPE_CHAIN_SERIALIZED_SIZE // Vec data
    }
}

#[cfg(feature = "serde")]
pub mod serde_string {
    use std::{fmt::Display, str::FromStr};

    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}
