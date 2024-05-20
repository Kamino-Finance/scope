use crate::utils::pdas::seeds;
use crate::ScopeError;
use anchor_lang::prelude::*;
use num_enum::TryFromPrimitive;
#[derive(TryFromPrimitive, PartialEq, Eq, Clone, Copy, Debug)]
#[repr(u64)]
pub enum UpdateTokenMetadataMode {
    Name = 0,
    MaxPriceAgeSlots = 1,
}

impl UpdateTokenMetadataMode {
    pub fn to_u64(self) -> u64 {
        self.to_u16().into()
    }

    pub fn to_u16(self) -> u16 {
        match self {
            UpdateTokenMetadataMode::Name => 0,
            UpdateTokenMetadataMode::MaxPriceAgeSlots => 1,
        }
    }
}

#[derive(Accounts)]
#[instruction(index: u64, mode: u64,  feed_name: String, value: Vec<u8>)]
pub struct UpdateTokensMetadata<'info> {
    pub admin: Signer<'info>,
    #[account(seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, has_one = admin, has_one = tokens_metadata)]
    pub configuration: AccountLoader<'info, crate::Configuration>,

    #[account(mut)]
    pub tokens_metadata: AccountLoader<'info, crate::TokenMetadatas>,
}

pub fn process(
    ctx: Context<UpdateTokensMetadata>,
    index: usize,
    mode: u64,
    value: Vec<u8>,
    _: String,
) -> Result<()> {
    let mut tokens_metadata = ctx.accounts.tokens_metadata.load_mut()?;

    let token_metadata = tokens_metadata
        .metadatas_array
        .get_mut(index)
        .ok_or(ScopeError::BadTokenNb)?;

    let mode: UpdateTokenMetadataMode = mode
        .try_into()
        .map_err(|_| ScopeError::InvalidTokenUpdateMode)?;
    match mode {
        UpdateTokenMetadataMode::MaxPriceAgeSlots => {
            let value = u64::from_le_bytes(value[..8].try_into().unwrap());
            msg!("Setting token max age for index {:?} to {}", index, value);
            token_metadata.max_age_price_slots = value;
        }
        UpdateTokenMetadataMode::Name => {
            assert!(
                value.len() <= 32,
                "Name is longer should be less than 32 bytes"
            );
            token_metadata.name.fill(0);
            token_metadata
                .name
                .iter_mut()
                .zip(value.iter())
                .for_each(|(a, b)| *a = *b);
            let str_name = std::str::from_utf8(&token_metadata.name).unwrap();
            msg!("Setting token name for index {} to {}", index, str_name);
        }
    }

    Ok(())
}
