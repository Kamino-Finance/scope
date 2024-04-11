use anchor_lang::prelude::*;

use crate::{
    oracles::{check_context, validate_oracle_account, OracleType},
    OracleMappings, ScopeError,
};

#[derive(Accounts)]
#[instruction(token:u64, price_type: u8, twap_enabled: bool, twap_source: u16, feed_name: String)]
pub struct UpdateOracleMapping<'info> {
    pub admin: Signer<'info>,
    #[account(seeds = [b"conf", feed_name.as_bytes()], bump, has_one = admin, has_one = oracle_mappings)]
    pub configuration: AccountLoader<'info, crate::Configuration>,
    #[account(mut)]
    pub oracle_mappings: AccountLoader<'info, OracleMappings>,
    /// CHECK: We trust the admin to provide a trustable account here. Some basic sanity checks are done based on type
    pub price_info: Option<AccountInfo<'info>>,
}

pub fn process(
    ctx: Context<UpdateOracleMapping>,
    token: usize,
    price_type: u8,
    twap_enabled: bool,
    twap_source: u16,
    _: String,
) -> Result<()> {
    check_context(&ctx)?;

    msg!(
        "UpdateOracleMapping, token: {}, price_type: {}, twap_enabled: {}, twap_source: {}",
        token,
        price_type,
        twap_enabled,
        twap_source
    );

    let mut oracle_mappings = ctx.accounts.oracle_mappings.load_mut()?;
    let ref_price_pubkey = oracle_mappings
        .price_info_accounts
        .get_mut(token)
        .ok_or(ScopeError::BadTokenNb)?;
    let price_type: OracleType = price_type
        .try_into()
        .map_err(|_| ScopeError::BadTokenType)?;

    match &ctx.accounts.price_info {
        Some(price_info_acc) => {
            validate_oracle_account(price_type, price_info_acc)?;
            // Every check succeeded, replace current with new
            let new_price_pubkey = price_info_acc.key();
            *ref_price_pubkey = new_price_pubkey;
        }
        None => {
            if price_type == OracleType::ScopeTwap {
                *ref_price_pubkey = crate::id();
            } else {
                // if no price_info account is passed, it means that the mapping has to be removed so it is set to Pubkey::default
                *ref_price_pubkey = Pubkey::default();
            }
        }
    }

    oracle_mappings.price_types[token] = price_type.into();
    oracle_mappings.twap_enabled[token] = u8::from(twap_enabled);
    oracle_mappings.twap_source[token] = twap_source;

    Ok(())
}
