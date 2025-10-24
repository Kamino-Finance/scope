use anchor_lang::{prelude::*, Accounts};

use crate::{oracles::check_context, states::configuration::Configuration};

#[derive(Accounts)]
#[instruction(feed_name: String)]
pub struct ApproveAdminCached<'info> {
    admin_cached: Signer<'info>,

    #[account(mut, seeds = [b"conf", feed_name.as_bytes()], bump, has_one = admin_cached)]
    pub configuration: AccountLoader<'info, Configuration>,
}

pub fn process(ctx: Context<ApproveAdminCached>, feed_name: String) -> Result<()> {
    check_context(&ctx)?;

    let configuration = &mut ctx.accounts.configuration.load_mut()?;

    msg!(
        "old admin {} new admin {}, feed_name {}",
        configuration.admin,
        configuration.admin_cached,
        feed_name
    );

    configuration.admin = configuration.admin_cached;

    Ok(())
}
