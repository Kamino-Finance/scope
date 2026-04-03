use anchor_lang::prelude::*;

use crate::{
    oracles::check_context,
    states::{Configuration, OracleMappings},
    utils::pdas::seeds,
    ScopeError, MAX_ENTRIES,
};

/// Check if the authority is allowed to freeze/unfreeze prices.
/// Admin can freeze and unfreeze; emergency council can only freeze.
fn is_allowed_authority(
    authority: &Signer,
    configuration: &AccountLoader<Configuration>,
    freeze: bool,
) -> Result<bool> {
    let config = configuration.load()?;
    let authority = authority.key();
    let is_admin = authority == config.admin;
    let is_emergency_council =
        authority == config.emergency_council && config.emergency_council != Pubkey::default();

    Ok(is_admin || (is_emergency_council && freeze))
}

#[derive(Accounts)]
#[instruction(token: u16, feed_name: String, freeze: bool)]
pub struct FreezePrice<'info> {
    #[account(constraint = is_allowed_authority(&authority, &configuration, freeze)? @ ScopeError::UnauthorizedFreeze)]
    pub authority: Signer<'info>,

    #[account(seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, has_one = oracle_mappings)]
    pub configuration: AccountLoader<'info, Configuration>,

    #[account(mut)]
    pub oracle_mappings: AccountLoader<'info, OracleMappings>,
}

pub fn process(ctx: Context<FreezePrice>, token: u16, freeze: bool) -> Result<()> {
    check_context(&ctx)?;
    let entry_id: usize = token.into();
    require_gt!(MAX_ENTRIES, entry_id, ScopeError::BadTokenNb);

    let mut oracle_mappings = ctx.accounts.oracle_mappings.load_mut()?;

    if freeze {
        // Check entry is used and not already frozen
        require!(
            oracle_mappings.is_entry_used(entry_id),
            ScopeError::BadTokenNb
        );
        require!(
            !oracle_mappings.is_frozen(entry_id),
            ScopeError::PriceAlreadyFrozen
        );

        oracle_mappings.freeze(entry_id);
        msg!("Frozen price entry {}", entry_id);
    } else {
        // Check entry is frozen
        require!(
            oracle_mappings.is_frozen(entry_id),
            ScopeError::PriceNotFrozen
        );

        oracle_mappings.unfreeze(entry_id);
        msg!("Unfrozen price entry {}", entry_id);
    }

    Ok(())
}
