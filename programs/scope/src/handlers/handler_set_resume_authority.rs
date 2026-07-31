use anchor_lang::prelude::*;

use crate::{oracles::check_context, utils::pdas::seeds};

#[derive(Accounts)]
#[instruction(new_resume_authority: Pubkey, feed_name: String)]
pub struct SetResumeAuthority<'info> {
    pub admin: Signer<'info>,

    #[account(mut, seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, has_one = admin)]
    pub configuration: AccountLoader<'info, crate::states::configuration::Configuration>,
}

pub fn process(
    ctx: Context<SetResumeAuthority>,
    new_resume_authority: Pubkey,
    feed_name: String,
) -> Result<()> {
    check_context(&ctx)?;

    msg!(
        "setting resume_authority to {} feed_name {}",
        new_resume_authority,
        feed_name
    );

    let configuration = &mut ctx.accounts.configuration.load_mut()?;
    configuration.resume_authority = new_resume_authority;

    Ok(())
}
