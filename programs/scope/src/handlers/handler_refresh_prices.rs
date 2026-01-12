use std::convert::TryInto;

use anchor_lang::prelude::*;
use solana_program::{
    instruction::{get_stack_height, TRANSACTION_LEVEL_STACK_HEIGHT},
    pubkey,
    sysvar::instructions::{
        load_current_index_checked, load_instruction_at_checked, ID as SYSVAR_INSTRUCTIONS_ID,
    },
};

use crate::{
    oracles::{get_non_zero_price, OracleType},
    states::{OracleMappings, OraclePrices, OracleTwaps},
    utils::price_impl::check_ref_price_difference,
    ScopeError,
};

const COMPUTE_BUDGET_ID: Pubkey = pubkey!("ComputeBudget111111111111111111111111111111");

#[derive(Accounts)]
pub struct RefreshList<'info> {
    #[account(mut, has_one = oracle_mappings)]
    pub oracle_prices: AccountLoader<'info, OraclePrices>,
    pub oracle_mappings: AccountLoader<'info, OracleMappings>,
    #[account(mut, has_one = oracle_prices, has_one = oracle_mappings)]
    pub oracle_twaps: AccountLoader<'info, OracleTwaps>,
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

    let oracle_mappings = ctx.accounts.oracle_mappings.load()?;
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
        if *oracle_mapping != received_account.key() {
            msg!(
                "Invalid price account: {}, expected: {}",
                received_account.key(),
                *oracle_mapping
            );
            return err!(ScopeError::UnexpectedAccount);
        }
        let clock = Clock::get()?;
        let price_res = get_non_zero_price(
            price_type,
            received_account,
            &mut accounts_iter,
            &clock,
            &oracle_twaps,
            &oracle_mappings,
            &ctx.accounts.oracle_prices,
            token_idx,
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
            if let Err(e) = crate::oracles::twap::update_twaps(
                &mut oracle_twaps,
                token_idx,
                &price,
                oracle_mappings.twap_enabled_bitmask[token_idx],
            ) {
                msg!("Error while updating TWAP of token {token_idx}: {e:?}",);
            }
        }

        // Only temporary load as mut to allow prices to be computed based on a scope chain
        // from the price feed that is currently updated

        let mut oracle_prices = ctx.accounts.oracle_prices.load_mut()?;

        // check that the price is close enough to the ref price if there is a ref price
        if oracle_mappings.ref_price[token_idx] != u16::MAX {
            let ref_price =
                oracle_prices.prices[usize::from(oracle_mappings.ref_price[token_idx])].price;
            if let Err(diff_err) = check_ref_price_difference(price.price, ref_price) {
                if fail_tx_on_error {
                    return Err(diff_err);
                } else {
                    msg!(
                        "Price skipped as ref price check failed (token {token_idx}, type {price_type:?})",
                    );
                    continue;
                }
            }
        }
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
