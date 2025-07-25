use anchor_lang::prelude::*;
use decimal_wad::decimal::Decimal;
use anchor_spl::token::TokenAccount;

use unitas_itf::account::{
    AssetLookupTable, get_associated_token_address, UsduConfig,
};

use crate::{
    utils::{account_deserialize, math::ten_pow},
    DatedPrice, Result, ScopeError, warn,
};

pub const AUM_VALUE_SCALE_DECIMALS: u8 = 6;

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
    let mint_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;

    let jlp_accounts = extra_accounts.take(unitas_asset_lookup_table.accounts.len()).collect::<Vec<_>>();
    check_accounts(&unitas_asset_lookup_table, mint_acc, &jlp_accounts)?;

    // 2. Get jlp oracle price
    let jlp_oracle_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    let data_price = super::pyth::get_price(jlp_oracle_acc, clock)?;

    // 3. Get usdu config account
    let usdu_config_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    let usdu_config: UsduConfig = account_deserialize(usdu_config_acc)?;

    // 4. Compute unitas aum
    compute_unitas_aum(
        &unitas_asset_lookup_table,
        jlp_accounts,
        &data_price,
        &usdu_config,
        clock,
    )
}

fn check_accounts(
    unitas_asset_lookup_table: &AssetLookupTable,
    mint_acc: &AccountInfo,
    jlp_accounts: &[&AccountInfo],
) -> Result<()> {
    require_eq!(
        jlp_accounts.len(),
        unitas_asset_lookup_table.accounts.len(),
        ScopeError::AccountsAndTokenMismatch
    );

    require_eq!(
        unitas_asset_lookup_table.mint,
        *mint_acc.key,
        ScopeError::UnexpectedAccount
    );

    let mut actual_owners: Vec<Pubkey> = Vec::new();
    for acc in jlp_accounts {
        let token_account = TokenAccount::try_deserialize(&mut &**acc.data.borrow())?;
        let at_acc = get_associated_token_address(
            &token_account.owner,
             &unitas_asset_lookup_table.mint
        );
        require_keys_eq!(
            at_acc,
            *acc.key,
            ScopeError::UnexpectedAccount
        );
        actual_owners.push(at_acc);
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

fn compute_unitas_aum(
    unitas_asset_lookup_table: &AssetLookupTable,
    jlp_accounts: Vec<&AccountInfo>,
    data_price: &DatedPrice,
    usdu_config: &UsduConfig,
    clock: &Clock,
) -> Result<DatedPrice> {
    let price = data_price.price;
    let price_value: u128 = price.value.into();
    let price_decimals: u8 = price.exp.try_into().unwrap();
    let token_decimals = unitas_asset_lookup_table.decimals;
    
    let mut total_value: u128 = unitas_asset_lookup_table.aum_usd;
    for jlp_acc in jlp_accounts {
        let token_account = TokenAccount::try_deserialize(&mut &**jlp_acc.data.borrow())?;
        let token_amount: u128 = token_account.amount.into();
        
        let total_decimals = price_decimals + token_decimals;
        let raw_value = price_value.checked_mul(token_amount).ok_or(ScopeError::MathError)?;
        let token_amount_usd = if total_decimals > AUM_VALUE_SCALE_DECIMALS {
            let diff = total_decimals - AUM_VALUE_SCALE_DECIMALS;
            raw_value / ten_pow(u32::from(diff))
        } else {
            let diff = AUM_VALUE_SCALE_DECIMALS - total_decimals;
            raw_value * ten_pow(u32::from(diff))
        };
        total_value += token_amount_usd;
    }

    let usdu_price = Decimal::from(total_value) / usdu_config.total_supply;
    
    Ok(DatedPrice {
        price: usdu_price.into(),
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp).unwrap(),
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
