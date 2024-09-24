use anchor_lang::prelude::*;

use crate::{
    oracles::{check_context, validate_oracle_cfg, OracleType},
    utils::{pdas::seeds, zero_copy_deserialize_mut},
    OracleMappings, ScopeError,
};

#[derive(Accounts)]
#[instruction(
    token_id: u16,
    price_type: u8,
    twap_enabled: bool,
    twap_source: u16,
    ref_price_index: u16,
    feed_name: String,
    generic_data: [u8; 20],
)]
pub struct UpdateOracleMapping<'info> {
    pub admin: Signer<'info>,
    #[account(seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, has_one = admin, has_one = oracle_mappings)]
    pub configuration: AccountLoader<'info, crate::Configuration>,

    /// CHECK: checked above + on deserialize
    #[account(mut, owner = crate::ID)]
    pub oracle_mappings: AccountInfo<'info>,
    /// CHECK: We trust the admin to provide a trustable account here. Some basic sanity checks are done based on type
    pub price_info: Option<AccountInfo<'info>>,
}

pub fn process(
    ctx: Context<UpdateOracleMapping>,
    entry_id: usize,
    price_type: u8,
    twap_enabled: bool,
    twap_source: u16,
    ref_price_index: u16,
    generic_data: &[u8; 20],
) -> Result<()> {
    check_context(&ctx)?;

    msg!(
        "UpdateOracleMapping, token: {}, price_type: {}, twap_enabled: {}, twap_source: {}, ref_price_index: {}",
        entry_id,
        price_type,
        twap_enabled,
        twap_source,
        ref_price_index
    );

    let mut oracle_mappings =
        zero_copy_deserialize_mut::<OracleMappings>(&ctx.accounts.oracle_mappings)?;
    let price_pubkey = oracle_mappings
        .price_info_accounts
        .get_mut(entry_id)
        .ok_or(ScopeError::BadTokenNb)?;
    let price_type: OracleType = price_type
        .try_into()
        .map_err(|_| ScopeError::BadTokenType)?;

    validate_oracle_cfg(
        price_type,
        &ctx.accounts.price_info,
        twap_source,
        generic_data,
    )?;

    match &ctx.accounts.price_info {
        Some(price_info_acc) => {
            // Every check succeeded, replace current with new
            let new_price_pubkey = price_info_acc.key();
            *price_pubkey = new_price_pubkey;
        }
        None => {
            match price_type {
                OracleType::ScopeTwap | OracleType::FixedPrice => *price_pubkey = crate::id(),

                _ => {
                    // if no price_info account is passed, it means that the mapping has to be removed so it is set to Pubkey::default
                    *price_pubkey = Pubkey::default();
                }
            }
        }
    }

    oracle_mappings.price_types[entry_id] = price_type.into();
    oracle_mappings.twap_enabled[entry_id] = u8::from(twap_enabled);
    oracle_mappings.twap_source[entry_id] = twap_source;
    oracle_mappings.ref_price[entry_id] = ref_price_index;
    oracle_mappings.generic[entry_id].copy_from_slice(generic_data);

    Ok(())
}
