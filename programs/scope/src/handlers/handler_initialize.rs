use anchor_lang::prelude::*;

use crate::{
    states::{
        configuration::Configuration, OracleMappings, OraclePrices, OracleTwaps, TokenMetadatas,
    },
    utils::pdas::seeds,
};

#[derive(Accounts)]
#[instruction(feed_name: String)]
pub struct Initialize<'info> {
    /// CHECK: At creation admin can be anyone, this ix can't override an existing feed
    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,

    // Set space to max size here
    // The ability to create multiple feeds is mostly useful for tests
    #[account(init, seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, payer = admin, space = 8 + std::mem::size_of::<crate::states::configuration::Configuration>())]
    pub configuration: AccountLoader<'info, Configuration>,

    #[account(zero)]
    pub token_metadatas: AccountLoader<'info, TokenMetadatas>,

    #[account(zero)]
    pub oracle_twaps: AccountLoader<'info, OracleTwaps>,

    // Account is pre-reserved/paid outside the program
    #[account(zero)]
    pub oracle_prices: AccountLoader<'info, OraclePrices>,

    // Account is pre-reserved/paid outside the program
    #[account(zero)]
    pub oracle_mappings: AccountLoader<'info, OracleMappings>,
}

pub fn process(ctx: Context<Initialize>, _: String) -> Result<()> {
    let _ = ctx.accounts.oracle_mappings.load_init()?;
    let _ = ctx.accounts.token_metadatas.load_init()?;
    let mut oracle_prices = ctx.accounts.oracle_prices.load_init()?;
    let mut oracle_twaps = ctx.accounts.oracle_twaps.load_init()?;
    let mut configuration: std::cell::RefMut<'_, Configuration> =
        ctx.accounts.configuration.load_init()?;

    let admin = ctx.accounts.admin.key();
    let oracle_pbk = ctx.accounts.oracle_mappings.key();
    let twaps_pbk = ctx.accounts.oracle_twaps.key();
    let prices_pbk = ctx.accounts.oracle_prices.key();
    let metadata_pbk = ctx.accounts.token_metadatas.key();

    // Initialize oracle mapping account
    oracle_prices.oracle_mappings = oracle_pbk;

    // Initialize configuration account
    configuration.admin = admin;
    configuration.oracle_mappings = oracle_pbk;
    configuration.oracle_prices = prices_pbk;
    configuration.oracle_twaps = twaps_pbk;
    configuration.tokens_metadata = metadata_pbk;
    configuration.admin_cached = Pubkey::default();

    // Initialize oracle twap account
    oracle_twaps.oracle_prices = prices_pbk;
    oracle_twaps.oracle_mappings = oracle_pbk;

    Ok(())
}
