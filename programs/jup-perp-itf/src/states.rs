use anchor_lang::prelude::*;

#[account]
#[derive(Default)]
pub struct Pool {
    pub name: String,
    pub custodies: Vec<Pubkey>,
    /// Pool value in usd scaled by 6 decimals
    pub aum_usd: u128,
    pub limit: Limit,
    pub fees: Fees,
    pub pool_apr: PoolApr,
    pub max_request_execution_sec: i64,
    pub bump: u8,
    pub lp_token_bump: u8,
    pub inception_time: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct Limit {
    pub max_aum_usd: u128,
    pub max_individual_lp_token: u128,
    pub max_position_usd: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct Fees {
    pub increase_position_bps: u64,
    pub decrease_position_bps: u64,
    pub add_remove_liquidity_bps: u64,
    pub swap_bps: u64,
    pub tax_bps: u64,
    pub stable_swap_bps: u64,
    pub stable_swap_tax_bps: u64,
    pub liquidation_reward_bps: u64,
    pub protocol_share_bps: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct PoolApr {
    pub last_updated: i64,
    pub fee_apr_bps: u64,
    pub realized_fee_usd: u64,
}

#[account]
#[derive(Default, Debug)]
pub struct Custody {
    pub pool: Pubkey,
    pub mint: Pubkey,
    pub token_account: Pubkey,
    pub decimals: u8,
    pub is_stable: bool,
    pub oracle: OracleParams,
    pub pricing: PricingParams,
    pub permissions: Permissions,
    pub target_ratio_bps: u64,
    pub assets: Assets,
    pub funding_rate_state: FundingRateState,

    pub bump: u8,
    pub token_account_bump: u8,
}

impl Custody {
    /// Returns the traders pnl delta and if the position has profit
    ///
    /// # Arguments
    ///
    /// * `current_price` - The current price of the asset scaled to `PRICE_DECIMALS`
    ///
    /// # Returns
    ///
    /// - `None` - In case of math overflow
    /// - `Some((traders_pnl_delta, position_has_profit))` - Otherwise
    pub fn get_global_short_pnl(&self, current_price: u64) -> Option<(u128, bool)> {
        let average_price = self.assets.global_short_average_prices;
        let price_delta = average_price.abs_diff(current_price);

        // traders_pnl_delta = global_short_sizes * price_delta / average_price
        let global_short_sizes: u128 = self.assets.global_short_sizes.into();
        let price_delta: u128 = price_delta.into();
        let nom = global_short_sizes.checked_mul(price_delta)?;
        let denom: u128 = average_price.into();
        let traders_pnl_delta = nom.checked_div(denom)?;

        // if true, pool lost, trader profit
        // if false, pool profit, trader lost
        let position_has_profit = average_price > current_price;

        Some((traders_pnl_delta, position_has_profit))
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct OracleParams {
    pub oracle_account: Pubkey,
    pub oracle_type: OracleType,
    pub max_price_error: u64,
    pub max_price_age_sec: u32,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default, PartialEq, Eq)]
pub enum OracleType {
    #[default]
    None,
    Test,
    Pyth,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct PricingParams {
    trade_spread_long: u64,
    trade_spread_short: u64,
    swap_spread: u64,
    max_leverage: u64,
    max_global_long_sizes: u64,
    max_global_short_sizes: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct Assets {
    pub fees_reserves: u64,
    pub owned: u64,
    pub locked: u64,
    pub guaranteed_usd: u64,
    pub global_short_sizes: u64,
    pub global_short_average_prices: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct Permissions {
    allow_swap: bool,
    allow_add_liquidity: bool,
    allow_remove_liquidity: bool,
    allow_increase_position: bool,
    allow_decrease_position: bool,
    allow_collateral_withdrawal: bool,
    allow_liquidate_position: bool,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct FundingRateState {
    cumulative_interest_rate: u128,
    last_updated: i64,
    hourly_funding_bps: u64,
}
