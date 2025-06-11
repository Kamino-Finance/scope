use anchor_lang::prelude::*;
use redstone_itf::accounts::PriceData;
use securitize_itf::accounts::VaultState;

use crate::{
    oracles::redstone::redstone_value_to_scope_price,
    utils::{account_deserialize, consts::MILLIS_PER_SECOND},
    DatedPrice, Price, ScopeError, ScopeResult,
};
use anchor_spl::token::spl_token::state::Mint;
use anchor_spl::token_interface::TokenAccount;

use solana_program::program_pack::Pack;

// sAcred is not necessarily pegged 1:1 to ACRED as the vault can be undercollateralized (but it cannot overcollateralize as the vault mechanism tops the value at $1). So it may be worth $1 or less depending on the vault status.
// The way to get the price, which is secured via RedStone, because the method does as follows:
// Checks the amount of ACRED inside the sACRED vault
// Multiplies that by the ACRED price from REDSTONE feed
// Checks if that value is above or below the amount of issued sACRED tokens
// if above, then the price is $1
// if below then the price is the $ value in the vault (based on the REDSTONE price) divided by the amount of outstanding sACRED

/// There is no connection between VaultState to the Feed Account it uses for the share value.
/// Program `nav_provider_program` from VaultState account uses below account (RedStone Acred Feed).
const ACRED_FEED_ADDRESS: Pubkey = Pubkey::new_from_array([
    87, 45, 249, 239, 149, 76, 238, 9, 125, 13, 122, 250, 108, 212, 185, 54, 210, 92, 118, 153,
    224, 126, 27, 246, 142, 115, 239, 161, 28, 61, 250, 196,
]);

pub fn get_sacred_price<'a, 'b>(
    price_info: &AccountInfo,
    _dated_price: &DatedPrice,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<DatedPrice>
where
    'a: 'b,
{
    let vault_state: VaultState = account_deserialize(price_info)?;

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
        mint_account.key,
        vault_account.key,
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
    let price_data: PriceData = account_deserialize(redstone_price_data)?;

    let rate = get_rate(&price_data, &mint)?;
    let price = get_share_value(vault.amount, rate, mint.decimals, mint.supply)?;
    let Some(write_timestamp_ms) = price_data.write_timestamp else {
        return Err(ScopeError::BadTimestamp)?;
    };

    let unix_timestamp = (write_timestamp_ms.min(price_data.timestamp) / MILLIS_PER_SECOND)
        .min(clock.unix_timestamp.try_into().unwrap());

    Ok(DatedPrice {
        price: Price {
            value: price,
            exp: mint.decimals as u64,
        },
        last_updated_slot: price_data.write_slot_number,
        unix_timestamp,
        generic_data: [0; 24],
    })
}

fn check_accounts(
    state: &VaultState,
    mint_account: &Pubkey,
    vault_account: &Pubkey,
    redstone_adapter_account: &AccountInfo,
) -> ScopeResult {
    if state.asset_vault != *vault_account
        || state.share_mint != *mint_account
        || *redstone_adapter_account.owner != redstone_itf::ID
        || *redstone_adapter_account.key != ACRED_FEED_ADDRESS
    {
        return Err(ScopeError::UnexpectedAccount);
    }

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
        mul_div(total_assets, rate, total_supply)?,
    ))
}

fn get_rate(price_data: &PriceData, mint: &Mint) -> ScopeResult<u64> {
    let price = redstone_value_to_scope_price(&price_data.value, price_data.decimals)?;

    let rate = normalize_rate(price.value, price.exp as u8, mint.decimals)?;

    if rate == 0 {
        return Err(ScopeError::PriceNotValid);
    }

    Ok(rate)
}

pub fn mul_div(value: u64, multiplier: u64, divisor: u64) -> ScopeResult<u64> {
    if divisor == 0 {
        return Err(ScopeError::MathOverflow);
    }

    let value = value as u128;
    let multiplier = multiplier as u128;
    let divisor = divisor as u128;

    let product = value
        .checked_mul(multiplier)
        .ok_or_else(|| ScopeError::MathOverflow)?;

    let result = product.checked_div(divisor);

    result
        .ok_or_else(|| ScopeError::MathOverflow)?
        .try_into()
        .map_err(|_| ScopeError::MathOverflow)
}

fn normalize_rate(value: u64, from_decimals: u8, to_decimals: u8) -> ScopeResult<u64> {
    if from_decimals == to_decimals {
        return Ok(value);
    }
    let (diff, is_div) = if from_decimals > to_decimals {
        (from_decimals.checked_sub(to_decimals), true)
    } else {
        (to_decimals.checked_sub(from_decimals), false)
    };

    let diff = diff.ok_or_else(|| ScopeError::MathOverflow)?;
    let factor = 10u64
        .checked_pow(diff as u32)
        .ok_or_else(|| ScopeError::MathOverflow)?;
    let result = if is_div {
        value.checked_div(factor)
    } else {
        value.checked_mul(factor)
    };
    result.ok_or_else(|| ScopeError::MathOverflow)
}
