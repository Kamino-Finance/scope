use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};

use super::{LimitedString, U128Split};

pub const MAX_CUSTODIES: usize = 8;

#[account(zero_copy)]
#[derive(Default, Debug)]
#[repr(C)]
pub struct Pool {
    pub bump: u8,
    pub lp_token_bump: u8,
    pub nb_stable_custody: u8,
    pub initialized: u8,
    pub allow_trade: u8,
    pub allow_swap: u8,
    pub liquidity_state: u8,
    pub registered_custody_count: u8,
    pub name: LimitedString,
    pub custodies: [Pubkey; MAX_CUSTODIES],
    pub fees_debt_usd: u64,
    pub referrers_fee_debt_usd: u64,
    pub cumulative_referrer_fee_usd: u64,
    pub lp_token_price_usd: u64, // <--- The price
    pub whitelisted_swapper: Pubkey,
    pub ratios: [TokenRatios; MAX_CUSTODIES],
    pub last_aum_and_lp_token_price_usd_update: i64, // <--- The price update time
    pub unique_limit_order_id_counter: u64,
    pub aum_usd: U128Split,
    pub inception_time: i64,
    pub aum_soft_cap_usd: u64,
}

#[derive(
    Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug, Zeroable, Pod,
)]
#[repr(C)]
pub struct TokenRatios {
    pub target: u16,
    pub min: u16,
    pub max: u16,
    pub _padding: [u8; 2],
}
