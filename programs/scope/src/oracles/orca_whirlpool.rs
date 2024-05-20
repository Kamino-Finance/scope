use anchor_lang::prelude::*;
use anchor_spl::token::spl_token::state::Mint;
use solana_program::program_pack::Pack;
use whirlpool::state::Whirlpool;

use crate::utils::account_deserialize;
use crate::utils::math::sqrt_price_to_price;
use crate::{DatedPrice, Result, ScopeError};

/// Gives the price of the given token pair in the given pool
pub fn get_price<'a, 'b>(
    a_to_b: bool,
    pool: &AccountInfo,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<DatedPrice>
where
    'a: 'b,
{
    // Get extra accounts
    let mint_token_a_account_info = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;
    let mint_token_b_account_info = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;

    // Load main account
    let pool_data: Whirlpool = account_deserialize(pool)?;

    // Check extra accounts pubkeys
    require_keys_eq!(
        pool_data.token_mint_a,
        mint_token_a_account_info.key(),
        ScopeError::AccountsAndTokenMismatch
    );

    require_keys_eq!(
        pool_data.token_mint_b,
        mint_token_b_account_info.key(),
        ScopeError::AccountsAndTokenMismatch
    );

    // Load extra accounts
    let mint_a_decimals = {
        let mint_borrow = mint_token_a_account_info.data.borrow();
        Mint::unpack(&mint_borrow)?.decimals
    };

    let mint_b_decimals = {
        let mint_borrow = mint_token_b_account_info.data.borrow();
        Mint::unpack(&mint_borrow)?.decimals
    };

    // Compute price
    let price = sqrt_price_to_price(
        a_to_b,
        pool_data.sqrt_price,
        mint_a_decimals,
        mint_b_decimals,
    )
    .map_err(|e| {
        msg!("Error while computing the price of the tokens in the pool: {e:?}",);
        e
    })?;

    // Return price
    Ok(DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: clock.unix_timestamp as u64,
        ..Default::default()
    })
}

pub fn validate_pool_account(pool: &AccountInfo) -> Result<()> {
    let _: Whirlpool = account_deserialize(pool)?;
    Ok(())
}
