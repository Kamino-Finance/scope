use anchor_lang::prelude::*;

use crate::states::mints_to_scope_chains::MintsToScopeChains;

#[derive(Accounts)]
pub struct CloseMintMap<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(has_one = admin)]
    pub configuration: AccountLoader<'info, crate::states::Configuration>,
    #[account(mut, close = admin, constraint = mappings.oracle_prices == configuration.load()?.oracle_prices)]
    pub mappings: Account<'info, MintsToScopeChains>,

    pub system_program: Program<'info, System>,
}

pub fn process(_ctx: Context<CloseMintMap>) -> Result<()> {
    Ok(())
}
