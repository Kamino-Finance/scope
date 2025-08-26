use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;
use decimal_wad::decimal::Decimal;

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

    // 2. Get jlp & usdc oracle price
    let jlp_oracle_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    assert_eq!(jlp_oracle_acc.key(), unitas_asset_lookup_table.jlp_oracle_account);
    let jlp_data_price = super::pyth_pull::get_price(jlp_oracle_acc, clock)?;

    let usdc_oracle_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    assert_eq!(usdc_oracle_acc.key(), unitas_asset_lookup_table.usdc_oracle_account);
    let usdc_data_price = super::pyth_pull::get_price(usdc_oracle_acc, clock)?;

    // 3. Get usdu config account
    let usdu_config_acc = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    assert_eq!(usdu_config_acc.key(), unitas_asset_lookup_table.usdu_config);
    let usdu_config: UsduConfig = account_deserialize(usdu_config_acc)?;

    // 4. Get jlp & usdc token accounts
    let num_owners = unitas_asset_lookup_table.token_account_owners.len();
    let jlp_accounts = extra_accounts.take(num_owners).collect::<Vec<_>>();
    let usdc_accounts = extra_accounts.take(num_owners).collect::<Vec<_>>();

    // 5. Compute unitas aum
    compute_unitas_aum(
        &unitas_asset_lookup_table,
        jlp_accounts,
        &jlp_data_price,
        usdc_accounts,
        &usdc_data_price,
        &usdu_config,
        clock,
    )
}

fn compute_unitas_aum(
    unitas_asset_lookup_table: &AssetLookupTable,
    jlp_accounts: Vec<&AccountInfo>,
    jlp_data_price: &DatedPrice,
    usdc_accounts: Vec<&AccountInfo>,
    usdc_data_price: &DatedPrice,
    usdu_config: &UsduConfig,
    clock: &Clock,
) -> Result<DatedPrice> {
    // Check lengths
    require_eq!(
        jlp_accounts.len(),
        unitas_asset_lookup_table.token_account_owners.len(),
        ScopeError::AccountsAndTokenMismatch
    );
    require_eq!(
        usdc_accounts.len(),
        unitas_asset_lookup_table.token_account_owners.len(),
        ScopeError::AccountsAndTokenMismatch
    );
    
    // JLP value calculation
    let jlp_price = jlp_data_price.price;
    let jlp_price_value: u128 = jlp_price.value.into();
    let jlp_price_decimals: u8 = jlp_price.exp.try_into().unwrap();
    let jlp_token_decimals = 6;

    let mut total_value: u128 = unitas_asset_lookup_table.aum_usd;
    for (idx, jlp_acc) in jlp_accounts.iter().enumerate() {
        // Skip uninitialized accounts
        if jlp_acc.owner == &solana_program::system_program::ID {
            continue;
        }

        let owner = unitas_asset_lookup_table.token_account_owners[idx];
        let expected_ata = get_associated_token_address(&owner, &unitas_asset_lookup_table.jlp_mint);
        require_keys_eq!(*jlp_acc.key, expected_ata, ScopeError::UnexpectedAccount);

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
    
    for (idx, usdc_acc) in usdc_accounts.iter().enumerate() {
        // Skip uninitialized accounts
        if usdc_acc.owner == &solana_program::system_program::ID {
            continue;
        }
        
        let owner = unitas_asset_lookup_table.token_account_owners[idx];
        let expected_ata = get_associated_token_address(&owner, &unitas_asset_lookup_table.usdc_mint);
        require_keys_eq!(*usdc_acc.key, expected_ata, ScopeError::UnexpectedAccount);

        let usdc_token_account = TokenAccount::try_deserialize(&mut &**usdc_acc.data.borrow())?;
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
    }

    // The final price calculation assumes that the USDU token also has 6 decimals (AUM_VALUE_SCALE_DECIMALS).
    // This is because both `total_value` (the AUM) and `usdu_config.total_supply` are scaled
    // by 10^6, so the scaling factors cancel each other out upon division.
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
