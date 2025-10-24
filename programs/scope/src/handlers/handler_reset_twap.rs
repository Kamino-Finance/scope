use anchor_lang::prelude::*;
use solana_program::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

use crate::{oracles::check_context, states::OracleTwaps, utils::pdas::seeds};

#[derive(Accounts)]
#[instruction(token:u64, feed_name: String)]
pub struct ResetTwap<'info> {
    pub admin: Signer<'info>,

    #[account(seeds = [seeds::CONFIG, feed_name.as_bytes()], bump,
        has_one = admin,
        has_one = oracle_twaps,
    )]
    pub configuration: AccountLoader<'info, crate::states::configuration::Configuration>,
    #[account(mut)]
    pub oracle_twaps: AccountLoader<'info, OracleTwaps>,
    /// CHECK: Sysvar fixed address
    #[account(address = SYSVAR_INSTRUCTIONS_ID)]
    pub instruction_sysvar_account_info: AccountInfo<'info>,
}

pub fn process(ctx: Context<ResetTwap>, token: usize, _: String) -> Result<()> {
    check_context(&ctx)?;

    let mut oracle_twaps = ctx.accounts.oracle_twaps.load_mut()?;

    crate::oracles::twap::reset_twap(&mut oracle_twaps, token)?;

    Ok(())
}
