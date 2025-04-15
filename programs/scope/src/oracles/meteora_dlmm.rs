use std::cell::Ref;

use anchor_lang::prelude::*;
use anchor_spl::token::spl_token::state::Mint;
use decimal_wad::decimal::U192;
pub use lb_clmm_itf as lb_clmm;
use solana_program::program_pack::Pack;

use crate::{
    utils::{math, zero_copy_deserialize},
    warn, DatedPrice, Result, ScopeError,
};

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
    let lb_pair_state: Ref<'_, lb_clmm::LbPair> = zero_copy_deserialize(pool)?;

    // Check extra accounts pubkeys
    require_keys_eq!(
        lb_pair_state.token_x_mint,
        mint_token_a_account_info.key(),
        ScopeError::AccountsAndTokenMismatch
    );

    require_keys_eq!(
        lb_pair_state.token_y_mint,
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
    let q64x64_price =
        lb_clmm::get_x64_price_from_id(lb_pair_state.active_id, lb_pair_state.bin_step)
            .ok_or_else(|| {
                warn!("Math overflow when calculating dlmm price");
                ScopeError::MathOverflow
            })?;
    let q64x64_price = if a_to_b {
        U192::from(q64x64_price)
    } else {
        // Invert price - safe, since `lb_clmm::get_x64_price_from_id` never returns 0.
        (U192::one() << 128) / q64x64_price
    };

    let lamport_price = math::q64x64_price_to_price(q64x64_price).map_err(|e| {
        warn!("Error while computing the price of the tokens in the pool: {e:?}",);
        e
    })?;
    let (src_token_decimals, dst_token_decimals) = if a_to_b {
        (mint_a_decimals, mint_b_decimals)
    } else {
        (mint_b_decimals, mint_a_decimals)
    };
    let price = math::price_of_lamports_to_price_of_tokens(
        lamport_price,
        src_token_decimals.into(),
        dst_token_decimals.into(),
    );

    // Return price
    Ok(DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: clock.unix_timestamp as u64,
        ..Default::default()
    })
}

pub fn validate_pool_account(pool: &Option<AccountInfo>) -> Result<()> {
    let Some(pool) = pool else {
        warn!("No pool account provided");
        return err!(ScopeError::PriceNotValid);
    };
    let _: Ref<'_, lb_clmm::LbPair> = zero_copy_deserialize(pool)?;
    Ok(())
}
