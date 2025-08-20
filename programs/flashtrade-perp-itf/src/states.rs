use anchor_lang::prelude::*;

#[account]
#[derive(Default, Debug)]
pub struct Pool {
    pub name: String,
    pub permissions: Permissions,
    pub inception_time: i64,
    pub lp_mint: Pubkey,
    pub oracle_authority: Pubkey,
    pub staked_lp_vault: Pubkey,
    pub reward_custody: Pubkey,
    pub custodies: Vec<Pubkey>,
    pub ratios: Vec<TokenRatios>,
    pub markets: Vec<Pubkey>,
    pub max_aum_usd: u128,
    pub aum_usd: u128, // raw AUM imn USD not including cumulative unrealised pnl
    pub total_staked: StakeStats,
    pub staking_fee_share_bps: u64,
    pub bump: u8,
    pub lp_mint_bump: u8,
    pub staked_lp_vault_bump: u8,
    pub vp_volume_factor: u8,
    pub unique_custody_count: u8,
    pub padding: [u8; 3],
    pub staking_fee_boost_bps: [u64; 6],
    pub compounding_mint: Pubkey,
    pub compounding_lp_vault: Pubkey,
    pub compounding_stats: CompoundingStats,
    pub compounding_mint_bump: u8,
    pub compounding_lp_vault_bump: u8,

    pub min_lp_price_usd: u64,
    pub max_lp_price_usd: u64,

    pub lp_price: u64, // The current staked LP (sFLP) price in USD scaled by 6 decimals
    pub compounding_lp_price: u64, // The current compounding LP (FLP) price in USD scaled by 6 decimals
    pub last_updated_timestamp: i64, // The timestamp of the last LP price update
    pub padding2: [u64; 1],
}

#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct Permissions {
    pub allow_swap: bool,
    pub allow_add_liquidity: bool,
    pub allow_remove_liquidity: bool,
    pub allow_open_position: bool,
    pub allow_close_position: bool,
    pub allow_collateral_withdrawal: bool,
    pub allow_size_change: bool,
    pub allow_liquidation: bool,
    pub allow_lp_staking: bool,
    pub allow_fee_distribution: bool,
    pub allow_ungated_trading: bool,
    pub allow_fee_discounts: bool,
    pub allow_referral_rebates: bool,
}

#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct TokenRatios {
    pub target: u64,
    pub min: u64,
    pub max: u64,
}

#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct StakeStats {
    pub pending_activation: u64,
    pub active_amount: u64,
    pub pending_deactivation: u64,
    pub deactivated_amount: u64,
}

#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct CompoundingStats {
    pub active_amount: u64,
    pub total_supply: u64,
    pub reward_snapshot: u128,
    pub fee_share_bps: u64,
    pub last_compound_time: i64,
}
