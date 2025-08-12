use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;
use decimal_wad::decimal::Decimal;
use solana_program::pubkey::Pubkey;

use crate::{
    utils::{account_deserialize, math::{estimate_slot_update_from_ts, ten_pow}},
    DatedPrice, Result, ScopeError, warn,
};
use unitas_itf::account::{AssetLookupTable, get_associated_token_address, UsduConfig};

const AUM_VALUE_SCALE_DECIMALS: u8 = 6;

/**
 * JLP Mint Decimals: 6
 * USDC Mint Decimals: 6
 * USDU Mint Decimals: 6
 */

/// Get the price of 1 USDU
/// This function recompute the AUM of the pool from the custodies and the oracles
/// Required extra accounts:
/// - Unitas all JLP token accounts and JLP USD price
/// - Unitas asset lookup table aum in usd
pub fn get_price<'a, 'b>(
    unitas_asset_lookup_table_acc: &AccountInfo<'a>,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<DatedPrice>
where
    'a: 'b,
{
     // 1. Get unitas accounts
    let unitas_asset_lookup_table: AssetLookupTable = account_deserialize(unitas_asset_lookup_table_acc)?;

    // 2. Get usdc token account
    let usdc_token_account_info = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    let usdc_token_account: TokenAccount = TokenAccount::try_deserialize(&mut &**usdc_token_account_info.data.borrow())?;
    check_usdc_account(&unitas_asset_lookup_table, &usdc_token_account, &usdc_token_account_info.key())?;

    // 3. Get jlp & usdc oracle price
    let jlp_oracle_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    assert_eq!(jlp_oracle_acc.key(), unitas_asset_lookup_table.jlp_oracle_account);
    let jlp_data_price = super::pyth_pull::get_price(jlp_oracle_acc, clock)?;

    let usdc_oracle_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    assert_eq!(usdc_oracle_acc.key(), unitas_asset_lookup_table.usdc_oracle_account);
    let usdc_data_price = super::pyth_pull::get_price(usdc_oracle_acc, clock)?;

    // 4. Get usdu config account
    let usdu_config_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    assert_eq!(usdu_config_acc.key(), unitas_asset_lookup_table.usdu_config);
    let usdu_config: UsduConfig = account_deserialize(usdu_config_acc)?;

    // 5. Get jlp token accounts
    let jlp_accounts = extra_accounts.take(unitas_asset_lookup_table.accounts.len()).collect::<Vec<_>>();
    check_jlp_accounts(&unitas_asset_lookup_table, &jlp_accounts)?;

    // 6. Compute unitas aum
    compute_unitas_aum(
        &unitas_asset_lookup_table,
        jlp_accounts,
        &jlp_data_price,
        &usdc_token_account,
        &usdc_data_price,
        &usdu_config,
        clock,
    )
}

fn check_jlp_accounts(
    unitas_asset_lookup_table: &AssetLookupTable,
    jlp_accounts: &[&AccountInfo],
) -> Result<()> {
    require_eq!(
        jlp_accounts.len(),
        unitas_asset_lookup_table.accounts.len(),
        ScopeError::AccountsAndTokenMismatch
    );

    let mut actual_owners: Vec<Pubkey> = Vec::new();
    for acc in jlp_accounts {
        let token_account = TokenAccount::try_deserialize(&mut &**acc.data.borrow())?;
        let at_acc = get_associated_token_address(
            &token_account.owner,
             &unitas_asset_lookup_table.jlp_mint
        );
        require_keys_eq!(
            at_acc,
            *acc.key,
            ScopeError::UnexpectedAccount
        );
        actual_owners.push(token_account.owner);
    }
    actual_owners.sort();

    let mut expected_jlp_pks = unitas_asset_lookup_table.accounts.clone();
    expected_jlp_pks.sort();

    for (expected_pk, actual_owner) in expected_jlp_pks.iter().zip(actual_owners.iter()) {
        require_keys_eq!(
            *expected_pk,
            *actual_owner,
            ScopeError::UnexpectedAccount
        );
    }

    Ok(())
}

fn check_usdc_account(
    unitas_asset_lookup_table: &AssetLookupTable,
    usdc_token_account: &TokenAccount,
    usdc_token_account_key: &Pubkey,
) -> Result<()> {
    require_keys_eq!(usdc_token_account.mint, unitas_asset_lookup_table.usdc_mint, ScopeError::UnexpectedAccount);
    let at_acc = get_associated_token_address(
        &usdc_token_account.owner,
        &unitas_asset_lookup_table.usdc_mint
    );
    require_keys_eq!(at_acc, *usdc_token_account_key, ScopeError::UnexpectedAccount);

    let mut signal = false;
    for acc in unitas_asset_lookup_table.accounts.iter() {
        if *acc == usdc_token_account.owner {
            signal = true;
            break;
        }
    }
    require!(signal, ScopeError::UnexpectedAccount);
    Ok(())
}

fn compute_unitas_aum(
    unitas_asset_lookup_table: &AssetLookupTable,
    jlp_accounts: Vec<&AccountInfo>,
    jlp_data_price: &DatedPrice,
    usdc_token_account: &TokenAccount,
    usdc_data_price: &DatedPrice,
    usdu_config: &UsduConfig,
    clock: &Clock,
) -> Result<DatedPrice> {
    // JLP value calculation
    let jlp_price = jlp_data_price.price;
    let jlp_price_value: u128 = jlp_price.value.into();
    let jlp_price_decimals: u8 = jlp_price.exp.try_into().unwrap();
    let jlp_token_decimals = 6;

    let mut total_value: u128 = unitas_asset_lookup_table.aum_usd;
    for jlp_acc in jlp_accounts {
        let token_account = TokenAccount::try_deserialize(&mut &**jlp_acc.data.borrow())?;
        let token_amount: u128 = token_account.amount.into();
        
        let total_decimals = jlp_price_decimals + jlp_token_decimals;
        let raw_value = jlp_price_value.checked_mul(token_amount).ok_or(ScopeError::MathOverflow)?;
        let token_amount_usd = if total_decimals > AUM_VALUE_SCALE_DECIMALS {
            let diff = total_decimals - AUM_VALUE_SCALE_DECIMALS;
            raw_value / ten_pow(u32::from(diff))
        } else {
            let diff = AUM_VALUE_SCALE_DECIMALS - total_decimals;
            raw_value.checked_mul(ten_pow(u32::from(diff))).ok_or(ScopeError::MathOverflow)?
        };
        total_value = total_value.checked_add(token_amount_usd).ok_or(ScopeError::MathOverflow)?;
    }

    // USDC value calculation
    let usdc_price = usdc_data_price.price;
    let usdc_price_value: u128 = usdc_price.value.into();
    let usdc_price_decimals: u8 = usdc_price.exp.try_into().unwrap();
    let usdc_token_decimals = 6;
    let usdc_amount: u128 = usdc_token_account.amount.into();
    
    let total_usdc_decimals = usdc_price_decimals + usdc_token_decimals;
    let raw_usdc_value = usdc_price_value.checked_mul(usdc_amount).ok_or(ScopeError::MathOverflow)?;
    let usdc_value_usd = if total_usdc_decimals > AUM_VALUE_SCALE_DECIMALS {
        let diff = total_usdc_decimals - AUM_VALUE_SCALE_DECIMALS;
        raw_usdc_value / ten_pow(u32::from(diff))
    } else {
        let diff = AUM_VALUE_SCALE_DECIMALS - total_usdc_decimals;
        raw_usdc_value.checked_mul(ten_pow(u32::from(diff))).ok_or(ScopeError::MathOverflow)?
    };
    total_value = total_value.checked_add(usdc_value_usd).ok_or(ScopeError::MathOverflow)?;


    let usdu_price = Decimal::from(total_value) / usdu_config.total_supply;
    
    // Use the oldest timestamp between:
    // 1. JLP oracle price timestamp
    // 2. Asset lookup table's AUM timestamp
    // 3. USDC oracle price timestamp
    let aum_timestamp = u64::try_from(unitas_asset_lookup_table.last_updated_timestamp)
        .map_err(|_| ScopeError::MathOverflow)?;
    let oldest_oracle_ts = std::cmp::min(jlp_data_price.unix_timestamp, usdc_data_price.unix_timestamp);
    let timestamp = std::cmp::min(oldest_oracle_ts, aum_timestamp);
    
    Ok(DatedPrice {
        price: usdu_price.into(),
        last_updated_slot: estimate_slot_update_from_ts(clock, timestamp),
        unix_timestamp: timestamp,
        ..Default::default()
    })
}

pub fn validate_price_account(price_data_account: &Option<AccountInfo>) -> Result<()> {
    let Some(price_data_account) = price_data_account else {
        warn!("No Unitas price account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };
    require_keys_eq!(
        *price_data_account.owner,
        unitas_itf::ID,
        ScopeError::WrongAccountOwner
    );
    let _: AssetLookupTable = account_deserialize(price_data_account)?;
    Ok(())
}
