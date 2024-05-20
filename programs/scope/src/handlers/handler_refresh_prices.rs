use std::convert::TryInto;

use anchor_lang::prelude::*;
use solana_program::{
    instruction::{get_stack_height, TRANSACTION_LEVEL_STACK_HEIGHT},
    pubkey,
    sysvar::instructions::{
        load_current_index_checked, load_instruction_at_checked, ID as SYSVAR_INSTRUCTIONS_ID,
    },
};

use crate::utils::zero_copy_deserialize;
use crate::OracleMappingsCore;
use crate::{
    oracles::{get_price, OracleType},
    ScopeError,
};

const COMPUTE_BUDGET_ID: Pubkey = pubkey!("ComputeBudget111111111111111111111111111111");

#[derive(Accounts)]
pub struct RefreshList<'info> {
    #[account(mut, has_one = oracle_mappings)]
    pub oracle_prices: AccountLoader<'info, crate::OraclePrices>,
    /// CHECK: Checked above
    #[account(owner = crate::ID)]
    pub oracle_mappings: AccountInfo<'info>,
    #[account(mut, has_one = oracle_prices, has_one = oracle_mappings)]
    pub oracle_twaps: AccountLoader<'info, crate::OracleTwaps>,
    /// CHECK: Sysvar fixed address
    #[account(address = SYSVAR_INSTRUCTIONS_ID)]
    pub instruction_sysvar_account_info: AccountInfo<'info>,
    // Note: use remaining accounts as price accounts
}

pub fn refresh_price_list<'info>(
    ctx: Context<'_, '_, '_, 'info, RefreshList<'info>>,
    tokens: &[u16],
) -> Result<()> {
    check_execution_ctx(&ctx.accounts.instruction_sysvar_account_info)?;

    let oracle_mappings =
        &zero_copy_deserialize::<OracleMappingsCore>(&ctx.accounts.oracle_mappings)?;
    let mut oracle_twaps = ctx.accounts.oracle_twaps.load_mut()?;

    // No token to refresh
    if tokens.is_empty() {
        return err!(ScopeError::EmptyTokenList);
    }

    // Check that the received token list is not too long
    if tokens.len() > crate::MAX_ENTRIES {
        return Err(ProgramError::InvalidArgument.into());
    }
    // Check the received token list is at least as long as the number of provided accounts
    if tokens.len() > ctx.remaining_accounts.len() {
        return err!(ScopeError::AccountsAndTokenMismatch);
    }

    // In case only one token is provided fail the whole transaction if the price is not valid
    let fail_tx_on_error = tokens.len() == 1;

    let zero_pk: Pubkey = Pubkey::default();

    let mut accounts_iter = ctx.remaining_accounts.iter();

    for &token_nb in tokens.iter() {
        let token_idx: usize = token_nb.into();
        let oracle_mapping = oracle_mappings
            .price_info_accounts
            .get(token_idx)
            .ok_or(ScopeError::BadTokenNb)?;
        let price_type: OracleType = oracle_mappings.price_types[token_idx]
            .try_into()
            .map_err(|_| ScopeError::BadTokenType)?;
        let received_account = accounts_iter
            .next()
            .ok_or(ScopeError::AccountsAndTokenMismatch)?;
        // Ignore unset mapping accounts
        if zero_pk == *oracle_mapping {
            msg!("Skipping token {} as no mapping is set", token_idx);
            continue;
        }
        // Check that the provided oracle accounts are the one referenced in oracleMapping
        if oracle_mappings.price_info_accounts[token_idx] != received_account.key() {
            msg!(
                "Invalid price account: {}, expected: {}",
                received_account.key(),
                oracle_mappings.price_info_accounts[token_idx]
            );
            return err!(ScopeError::UnexpectedAccount);
        }
        let clock = Clock::get()?;
        let price_res = get_price(
            price_type,
            received_account,
            &mut accounts_iter,
            &clock,
            &oracle_twaps,
            oracle_mappings,
            &ctx.accounts.oracle_prices,
            token_nb.into(),
        );
        let price = if fail_tx_on_error {
            price_res?
        } else {
            match price_res {
                Ok(price) => price,
                Err(_) => {
                    msg!(
                        "Price skipped as validation failed (token {token_idx}, type {price_type:?})",
                    );
                    continue;
                }
            }
        };

        if oracle_mappings.is_twap_enabled(token_idx) {
            let _ = crate::oracles::twap::update_twap(&mut oracle_twaps, token_idx, &price)
                .map_err(|_| msg!("Twap not found for token {}", token_idx));
        };

        // Only temporary load as mut to allow prices to be computed based on a scope chain
        // from the price feed that is currently updated

        let mut oracle_prices = ctx.accounts.oracle_prices.load_mut()?;
        let to_update = oracle_prices
            .prices
            .get_mut(token_idx)
            .ok_or(ScopeError::BadTokenNb)?;

        msg!(
            "tk {}, {:?}: {:?} to {:?} | prev_slot: {:?}, new_slot: {:?}, crt_slot: {:?}",
            token_idx,
            price_type,
            to_update.price.value,
            price.price.value,
            to_update.last_updated_slot,
            price.last_updated_slot,
            clock.slot,
        );

        *to_update = price;
        to_update.index = token_nb;
    }

    Ok(())
}

/// Ensure that the refresh instruction is executed directly to avoid any manipulation:
///
/// - Check that the current instruction is executed by our program id (not in CPI).
/// - Check that instructions preceding the refresh are compute budget instructions.
fn check_execution_ctx(instruction_sysvar_account_info: &AccountInfo) -> Result<()> {
    let current_index: usize = load_current_index_checked(instruction_sysvar_account_info)?.into();

    // 1- Check that the current instruction is executed by our program id (not in CPI).
    let current_ix = load_instruction_at_checked(current_index, instruction_sysvar_account_info)?;

    // the current ix must be executed by our program id. otherwise, it's a CPI.
    if crate::ID != current_ix.program_id {
        return err!(ScopeError::RefreshInCPI);
    }

    // The current stack height must be the initial one. Otherwise, it's a CPI.
    if get_stack_height() > TRANSACTION_LEVEL_STACK_HEIGHT {
        return err!(ScopeError::RefreshInCPI);
    }

    // 2- Check that instructions preceding the refresh are compute budget instructions.
    for ixn in 0..current_index {
        let ix = load_instruction_at_checked(ixn, instruction_sysvar_account_info)?;
        if ix.program_id != COMPUTE_BUDGET_ID {
            return err!(ScopeError::RefreshWithUnexpectedIxs);
        }
    }

    Ok(())
}
