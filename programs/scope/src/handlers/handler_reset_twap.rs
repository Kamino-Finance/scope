use anchor_lang::prelude::*;
use solana_program::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

use crate::{oracles::check_context, utils::pdas::seeds};

#[derive(Accounts)]
#[instruction(token:u64, feed_name: String)]
pub struct ResetTwap<'info> {
    pub admin: Signer<'info>,

    #[account()]
    pub oracle_prices: AccountLoader<'info, crate::OraclePrices>,
    #[account(seeds = [seeds::CONFIG, feed_name.as_bytes()], bump,
        has_one = admin,
        has_one = oracle_prices,
        has_one = oracle_twaps,
    )]
    pub configuration: AccountLoader<'info, crate::Configuration>,
    #[account(mut, has_one = oracle_prices)]
    pub oracle_twaps: AccountLoader<'info, crate::OracleTwaps>,
    /// CHECK: Sysvar fixed address
    #[account(address = SYSVAR_INSTRUCTIONS_ID)]
    pub instruction_sysvar_account_info: AccountInfo<'info>,
}

pub fn process(ctx: Context<ResetTwap>, token: usize, _: String) -> Result<()> {
    check_context(&ctx)?;

    let oracle = ctx.accounts.oracle_prices.load()?;
    let mut oracle_twaps = ctx.accounts.oracle_twaps.load_mut()?;

    let clock = Clock::get()?;

    let price = oracle.prices[token].price;

    crate::oracles::twap::reset_twap(
        &mut oracle_twaps,
        token,
        price,
        clock.unix_timestamp as u64,
        clock.slot,
    )?;

    Ok(())
}
