use anchor_lang::prelude::*;
use solana_program::pubkey;

pub static TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub static ASSOCIATED_TOKEN_PROGRAM_ID: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

#[account(zero_copy)]
#[repr(C)]
#[derive(Debug)]
pub struct AssetLookupTable {
    pub asset_mint: Pubkey,
    pub oracle_account: Pubkey,
    pub token_account_owners: [Pubkey; 16],
    pub token_account_owners_len: u32,
    pub decimals: u8,
    pub paddings: [u8; 3],
}

#[account]
#[derive(Default, Debug)]
pub struct UnitasConfig {
    pub admin: Pubkey,
    pub pending_admin: Pubkey,
    pub aum_usd: u128,
    pub last_updated_timestamp: i64,
    pub usdu_config: Pubkey,
}

#[account]
#[derive(Debug, InitSpace)]
pub struct UsduConfig {
    pub admin: Pubkey,
    pub pending_admin: Pubkey,
    pub access_registry: Pubkey,
    pub bump: u8,
    pub is_initialized: bool,

    pub usdu_token: Pubkey,
    pub usdu_token_bump: u8,
    pub is_usdu_token_initialized: bool,

    pub total_supply: u128,
}

pub fn get_associated_token_address(
    wallet_address: &Pubkey,
    mint_address: &Pubkey,
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            wallet_address.as_ref(),
            TOKEN_PROGRAM_ID.as_ref(),
            mint_address.as_ref(),
        ],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0
}