use anchor_lang::prelude::*;

#[derive(Debug, Default, InitSpace)]
#[account]
pub struct VaultState {
    pub id: u64,
    pub admin: Pubkey,
    pub is_paused: bool,

    pub asset_vault: Pubkey,
    pub share_mint: Pubkey,

    pub liquidation_open_to_public: bool,
    pub liquidation_token_vault: Option<Pubkey>,
    pub redemption_program: Option<Pubkey>,
    pub nav_provider_program: Pubkey,

    pub vault_authority_bump: u8,
    pub bump: u8,

    #[max_len(10)]
    pub operators: Vec<Pubkey>,
    #[max_len(10)]
    pub liquidators: Vec<Pubkey>,
}
