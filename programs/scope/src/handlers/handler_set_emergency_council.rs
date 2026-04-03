use anchor_lang::prelude::*;

use crate::{oracles::check_context, utils::pdas::seeds};

#[derive(Accounts)]
#[instruction(new_emergency_council: Pubkey, feed_name: String)]
pub struct SetEmergencyCouncil<'info> {
    pub admin: Signer<'info>,

    #[account(mut, seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, has_one = admin)]
    pub configuration: AccountLoader<'info, crate::states::configuration::Configuration>,
}

pub fn process(
    ctx: Context<SetEmergencyCouncil>,
    new_emergency_council: Pubkey,
    feed_name: String,
) -> Result<()> {
    check_context(&ctx)?;

    msg!(
        "setting emergency_council to {} feed_name {}",
        new_emergency_council,
        feed_name
    );

    let configuration = &mut ctx.accounts.configuration.load_mut()?;
    configuration.emergency_council = new_emergency_council;

    Ok(())
}
