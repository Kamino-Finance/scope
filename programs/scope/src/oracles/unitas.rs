use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;
use decimal_wad::decimal::Decimal;
use solana_program::pubkey;

use crate::{
    utils::{account_deserialize, math::{estimate_slot_update_from_ts, ten_pow}},
    DatedPrice, Result, ScopeError, warn,
};
use unitas_itf::account::{get_associated_token_address, AssetLookupTable, UnitasConfig, UsduConfig};

const AUM_VALUE_SCALE_DECIMALS: u8 = 6;
const ADMIN_CONFIG_SEED: &str = "admin-config";
const ASSET_LOOKUP_TABLE_SEED: &str = "asset-lookup-table";
const JLP_MINT: Pubkey = pubkey!("27G8MtK7VtTcCHkpASjSDdkWWYfoqT6ggEuKidVJidD4");
const USDC_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

pub fn get_price<'a, 'b>(
    unitas_config_acc: &AccountInfo<'a>,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<DatedPrice>
where
    'a: 'b,
{
    // 1. Get and validate unitas config
    let (expected_config_pda, _) =
        Pubkey::find_program_address(&[ADMIN_CONFIG_SEED.as_bytes()], &unitas_itf::ID);
    require_keys_eq!(
        unitas_config_acc.key(),
        expected_config_pda,
        ScopeError::UnexpectedAccount
    );
    let unitas_config: UnitasConfig = account_deserialize(unitas_config_acc)?;
    let mut total_value: u128 = unitas_config.aum_usd;
    let mut oldest_oracle_ts: u64 = u64::MAX;

    // --- Process assets ---
    // The convention is to provide assets in order:
    // 1. JLP AssetLookupTable
    // 2. JLP Oracle
    // 3. JLP Token accounts
    // 4. USDC AssetLookupTable
    // 5. USDC Oracle
    // 6. USDC Token accounts

    // 2. Process first asset (JLP)
    let jlp_lookup_table_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    let (expected_jlp_pda, _) = Pubkey::find_program_address(
        &[ASSET_LOOKUP_TABLE_SEED.as_bytes(), JLP_MINT.as_ref()],
        &unitas_itf::ID,
    );
    require_keys_eq!(
        jlp_lookup_table_acc.key(),
        expected_jlp_pda,
        ScopeError::UnexpectedAccount
    );
    let jlp_lookup_table: AssetLookupTable = account_deserialize(jlp_lookup_table_acc)?;
    require_keys_eq!(
        jlp_lookup_table.asset_mint,
        JLP_MINT,
        ScopeError::UnexpectedAccount
    );


    let jlp_oracle_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    require_keys_eq!(jlp_oracle_acc.key(), jlp_lookup_table.oracle_account, ScopeError::UnexpectedAccount);
    let jlp_price = super::pyth_pull::get_price(jlp_oracle_acc, clock)?;
    oldest_oracle_ts = std::cmp::min(oldest_oracle_ts, jlp_price.unix_timestamp);

    let num_jlp_owners = jlp_lookup_table.token_account_owners_len as usize;
    let mut jlp_token_accounts = extra_accounts.take(num_jlp_owners);

    let jlp_value = compute_asset_value(&jlp_lookup_table, &jlp_price, &mut jlp_token_accounts)?;
    total_value = total_value.checked_add(jlp_value).ok_or(ScopeError::MathOverflow)?;

    // 3. Process second asset (USDC)
    let usdc_lookup_table_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    let (expected_usdc_pda, _) = Pubkey::find_program_address(
        &[ASSET_LOOKUP_TABLE_SEED.as_bytes(), USDC_MINT.as_ref()],
        &unitas_itf::ID,
    );
    require_keys_eq!(
        usdc_lookup_table_acc.key(),
        expected_usdc_pda,
        ScopeError::UnexpectedAccount
    );
    let usdc_lookup_table: AssetLookupTable = account_deserialize(usdc_lookup_table_acc)?;
    require_keys_eq!(
        usdc_lookup_table.asset_mint,
        USDC_MINT,
        ScopeError::UnexpectedAccount
    );


    let usdc_oracle_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    require_keys_eq!(usdc_oracle_acc.key(), usdc_lookup_table.oracle_account, ScopeError::UnexpectedAccount);
    let usdc_price = super::pyth_pull::get_price(usdc_oracle_acc, clock)?;
    oldest_oracle_ts = std::cmp::min(oldest_oracle_ts, usdc_price.unix_timestamp);

    let num_usdc_owners = usdc_lookup_table.token_account_owners_len as usize;
    let mut usdc_token_accounts = extra_accounts.take(num_usdc_owners);

    let usdc_value = compute_asset_value(&usdc_lookup_table, &usdc_price, &mut usdc_token_accounts)?;
    total_value = total_value.checked_add(usdc_value).ok_or(ScopeError::MathOverflow)?;

    // 4. Get usdu config account and calculate price
    let usdu_config_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    require_keys_eq!(usdu_config_acc.key(), unitas_config.usdu_config, ScopeError::UnexpectedAccount);
    let usdu_config: UsduConfig = account_deserialize(usdu_config_acc)?;

    // The final price calculation assumes that the USDU token also has 6 decimals (AUM_VALUE_SCALE_DECIMALS).
    let usdu_price = Decimal::from(total_value) / usdu_config.total_supply;

    // 5. Determine timestamp
    let config_timestamp = u64::try_from(unitas_config.last_updated_timestamp)
        .map_err(|_| ScopeError::MathOverflow)?;
    let timestamp = std::cmp::min(oldest_oracle_ts, config_timestamp);
    
    Ok(DatedPrice {
        price: usdu_price.into(),
        last_updated_slot: estimate_slot_update_from_ts(clock, timestamp),
        unix_timestamp: timestamp,
        ..Default::default()
    })
}

fn compute_asset_value<'a, 'b>(
    asset_lookup_table: &AssetLookupTable,
    price: &DatedPrice,
    token_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<u128>
where
    'a: 'b,
{
    let price_value: u128 = price.price.value.into();
    let price_decimals: u8 = price.price.exp.try_into().unwrap();
    let token_decimals = asset_lookup_table.decimals;

    let mut total_asset_value: u128 = 0;

    let owners_len = asset_lookup_table.token_account_owners_len as usize;
    if owners_len > asset_lookup_table.token_account_owners.len() {
        return err!(ScopeError::MathOverflow);
    }

    let owners_slice = &asset_lookup_table.token_account_owners[..owners_len];

    for (owner, token_acc_info) in owners_slice.iter().zip(token_accounts) {
        if token_acc_info.owner == &solana_program::system_program::ID {
            continue;
        }

        let expected_ata = get_associated_token_address(owner, &asset_lookup_table.asset_mint);
        require_keys_eq!(*token_acc_info.key, expected_ata, ScopeError::UnexpectedAccount);
        
        let token_account = TokenAccount::try_deserialize(&mut &**token_acc_info.data.borrow())?;
        let token_amount: u128 = token_account.amount.into();
        
        let total_decimals = price_decimals + token_decimals;
        let raw_value = price_value.checked_mul(token_amount).ok_or(ScopeError::MathOverflow)?;

        let asset_value_usd = if total_decimals > AUM_VALUE_SCALE_DECIMALS {
            let diff = total_decimals - AUM_VALUE_SCALE_DECIMALS;
            raw_value / ten_pow(u32::from(diff))
        } else {
            let diff = AUM_VALUE_SCALE_DECIMALS - total_decimals;
            raw_value.checked_mul(ten_pow(u32::from(diff))).ok_or(ScopeError::MathOverflow)?
        };

        total_asset_value = total_asset_value.checked_add(asset_value_usd).ok_or(ScopeError::MathOverflow)?;
    }

    Ok(total_asset_value)
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
    let _: UnitasConfig = account_deserialize(price_data_account)?;
    Ok(())
}
