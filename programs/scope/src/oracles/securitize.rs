use anchor_lang::prelude::*;
use anchor_spl::{token::spl_token::state::Mint, token_interface::TokenAccount};
use securitize_itf::accounts::VaultState;
use solana_program::{program_pack::Pack, pubkey};

use super::redstone;
use crate::{
    utils::{account_deserialize, math},
    DatedPrice, Price, ScopeError, ScopeResult,
};

// sAcred is not necessarily pegged 1:1 to ACRED as the vault can be undercollateralized (but it cannot overcollateralize as the vault mechanism tops the value at $1). So it may be worth $1 or less depending on the vault status.
// The way to get the price, which is secured via RedStone, because the method does as follows:
// Checks the amount of ACRED inside the sACRED vault
// Multiplies that by the ACRED price from REDSTONE feed
// Checks if that value is above or below the amount of issued sACRED tokens
// if above, then the price is $1
// if below then the price is the $ value in the vault (based on the REDSTONE price) divided by the amount of outstanding sACRED

// Only works with the sAcred vault
// There is no connection between VaultState to the Feed Account it uses for the share value.
pub const ACRED_VAULT_PK: Pubkey = pubkey!("9L4WxKkUHKBZ96EpHBc7APqvEhobmY1A2ENk5dUfdrpw");
pub const REDSTONE_FEED_PK: Pubkey = pubkey!("6sK8czVw8Xy6T8YbH6VC8p5ovNZD2mXf5vUTv8sgnUJf");

pub fn get_sacred_price<'a, 'b>(
    vault_state_account_info: &AccountInfo,
    dated_price: &DatedPrice,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<DatedPrice>
where
    'a: 'b,
{
    let vault_state: VaultState = account_deserialize(vault_state_account_info)?;

    let mint_account = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;
    let vault_account = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;
    let redstone_price_data = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;

    check_accounts(
        &vault_state,
        *vault_state_account_info.key,
        *vault_account.key,
        *mint_account.key,
        redstone_price_data,
    )?;

    let mint = {
        let mint_borrow = mint_account.data.borrow();
        Mint::unpack(&mint_borrow)
    }?;

    let vault = {
        let vault_borrow = vault_account.data.borrow();
        TokenAccount::try_deserialize(&mut &**vault_borrow)
    }?;

    let asset_dated_price = redstone::get_price(redstone_price_data, dated_price, clock)?;

    let rate = get_rate(&asset_dated_price.price, &mint)?;
    let price = get_share_value(vault.amount, rate, mint.decimals, mint.supply)?;

    Ok(DatedPrice {
        price: Price {
            value: price,
            exp: mint.decimals as u64,
        },
        // Reuse the timestamp and slot from the asset price
        ..asset_dated_price
    })
}

fn check_accounts(
    state: &VaultState,
    state_account_key: Pubkey,
    vault_account_key: Pubkey,
    mint_account_key: Pubkey,
    redstone_adapter_account: &AccountInfo,
) -> Result<()> {
    require_keys_eq!(
        state_account_key,
        ACRED_VAULT_PK,
        ScopeError::UnexpectedAccount
    );
    require_keys_eq!(
        state.asset_vault,
        vault_account_key,
        ScopeError::UnexpectedAccount
    );
    require_keys_eq!(
        state.share_mint,
        mint_account_key,
        ScopeError::UnexpectedAccount
    );
    require_keys_eq!(
        *redstone_adapter_account.owner,
        redstone_itf::ID,
        ScopeError::UnexpectedAccount
    );
    require_keys_eq!(
        *redstone_adapter_account.key,
        REDSTONE_FEED_PK,
        ScopeError::UnexpectedAccount
    );

    Ok(())
}

fn get_share_value(
    total_assets: u64,
    rate: u64,
    decimals: u8,
    total_supply: u64,
) -> ScopeResult<u64> {
    if total_supply == 0 {
        return Ok(0);
    }
    Ok(u64::min(
        10u64.pow(decimals.into()),
        math::mul_div(total_assets, rate, total_supply)?,
    ))
}

fn get_rate(price: &Price, mint: &Mint) -> ScopeResult<u64> {
    let rate = math::normalize_rate(price.value, price.exp as u8, mint.decimals)?;

    if rate == 0 {
        return Err(ScopeError::PriceNotValid);
    }

    Ok(rate)
}
