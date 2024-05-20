use crate::ScopeError;
use anchor_lang::prelude::*;

use crate::utils::consts::ORACLE_MAPPING_EXTENDED_SIZE;
use crate::utils::pdas::seeds;

#[derive(Accounts)]
#[instruction(feed_name: String)]
pub struct ExtendMapping<'info> {
    /// CHECK: At creation admin can be anyone, this ix can't override an existing feed
    #[account(mut)]
    pub admin: Signer<'info>,

    // Set space to max size here
    // The ability to create multiple feeds is mostly useful for tests
    #[account(seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, has_one = admin, has_one = oracle_mappings)]
    pub configuration: AccountLoader<'info, crate::Configuration>,

    /// CHECK: checked above
    #[account(mut, owner = crate::ID)]
    pub oracle_mappings: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

pub fn process(ctx: Context<ExtendMapping>, _: String) -> Result<()> {
    resize_account_and_make_rent_exempt(
        &mut ctx.accounts.oracle_mappings,
        &ctx.accounts.admin,
        &ctx.accounts.system_program,
        ORACLE_MAPPING_EXTENDED_SIZE + 8,
    )
}

fn resize_account_and_make_rent_exempt<'a>(
    account: &mut AccountInfo<'a>,
    payer: &Signer<'a>,
    system_program: &Program<'a, System>,
    new_size: usize,
) -> Result<()> {
    if new_size <= account.data_len() {
        // It can be resized even when new_size <= old_size, but we want to exclude
        // this use case in order to catch human errors.
        return Err(error!(ScopeError::CannotResizeAccount));
    }
    // Make account rent exempt for the new size by transfering enough lamports.
    let rent = Rent::get()?;
    let new_minimum_balance = rent.minimum_balance(new_size);

    let lamports_diff = new_minimum_balance.saturating_sub(account.lamports());
    solana_program::program::invoke(
        &solana_program::system_instruction::transfer(payer.key, account.key, lamports_diff),
        &[
            payer.to_account_info().clone(),
            account.clone(),
            system_program.to_account_info(),
        ],
    )?;

    // No need to zero init as we always grow memory.
    account.realloc(new_size, false).map_err(Into::into)
}
