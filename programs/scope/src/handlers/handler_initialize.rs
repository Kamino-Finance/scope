use crate::utils::pdas::seeds;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(feed_name: String)]
pub struct Initialize<'info> {
    /// CHECK: At creation admin can be anyone, this ix can't override an existing feed
    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,

    // Set space to max size here
    // The ability to create multiple feeds is mostly useful for tests
    #[account(init, seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, payer = admin, space = 8 + std::mem::size_of::<crate::Configuration>())]
    pub configuration: AccountLoader<'info, crate::Configuration>,

    #[account(zero)]
    pub token_metadatas: AccountLoader<'info, crate::TokenMetadatas>,

    #[account(zero)]
    pub oracle_twaps: AccountLoader<'info, crate::OracleTwaps>,

    // Account is pre-reserved/paid outside the program
    #[account(zero)]
    pub oracle_prices: AccountLoader<'info, crate::OraclePrices>,

    // Account is pre-reserved/paid outside the program
    #[account(zero)]
    pub oracle_mappings: AccountLoader<'info, crate::OracleMappings>,
}

pub fn process(ctx: Context<Initialize>, _: String) -> Result<()> {
    // Initialize oracle mapping account
    let _ = ctx.accounts.oracle_mappings.load_init()?;

    // Initialize oracle price account
    let oracle_pbk = ctx.accounts.oracle_mappings.key();
    let twaps_pbk = ctx.accounts.oracle_twaps.key();

    let mut oracle_prices = ctx.accounts.oracle_prices.load_init()?;
    oracle_prices.oracle_mappings = oracle_pbk;

    // Initialize configuration account
    let prices_pbk = ctx.accounts.oracle_prices.key();
    let admin = ctx.accounts.admin.key();
    let mut configuration: std::cell::RefMut<'_, crate::Configuration> =
        ctx.accounts.configuration.load_init()?;
    configuration.admin = admin;
    configuration.oracle_mappings = oracle_pbk;
    configuration.oracle_prices = prices_pbk;
    configuration.oracle_twaps = twaps_pbk;
    configuration.admin_cached = Pubkey::default();

    // Initialize oracle twap account
    let mut oracle_twaps = ctx.accounts.oracle_twaps.load_init()?;
    oracle_twaps.oracle_prices = prices_pbk;
    oracle_twaps.oracle_mappings = oracle_pbk;

    let _ = ctx.accounts.token_metadatas.load_init()?;
    configuration.tokens_metadata = ctx.accounts.token_metadatas.key();

    Ok(())
}
