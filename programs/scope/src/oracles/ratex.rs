use anchor_lang::prelude::*;
use raydium_amm_v3::libraries::U256;
use arrayref::{array_ref, array_refs};
use crate::utils::tick_math::tick_index_to_sqrt_price_x64;

use crate::{
    DatedPrice, Price, Result, ScopeError
};

fn get_pt_price(tick: i32) -> Result<Price> {
    let sqrt_price_x64 = tick_index_to_sqrt_price_x64(tick);
    let price_x64 = (sqrt_price_x64 * sqrt_price_x64) >> U256::from(64);

    let pt_price_x64 = U256::from(1u128 << 64) - price_x64 * U256::from(950_000_000) / U256::from(1_000_000_000);
    let pt_price = (pt_price_x64 * U256::from(1_000_000_000u128)) >> U256::from(64);

    if pt_price < U256::from(u64::MAX) {
        Ok(Price{value: pt_price.as_u64(), exp: 9})
    } else {
        Err(ScopeError::IntegerOverflow.into())
    }
}

fn unpack_yield_market(data: &[u8]) -> (i64, i32, Pubkey) {
    let src = array_ref![&data[8..], 0, 1450];
    let (
        _pubkey,
        oracle,
        _name,
        _quote_asset_vault,
        _base_asset_vault,
        // Amm pool starts
        _ammpools_config,
        _liquidity,
        _sqrt_price,
        _protocol_fee_owed_a,
        _protocol_fee_owed_b,
        _token_mint_base,
        _token_vault_base,
        _fee_growth_global_a,
        _token_mint_quote,
        _token_vault_quote,
        _fee_growth_global_b,
        _reward_last_updated_timestamp,
        _reward_infos,
        _ammpool_oracle,
        _tick_current_index,
        _observation_index,
        _observation_update_duration,
        _tick_spacing,
        _tick_spacing_seed,
        _fee_rate,
        _protocol_fee_rate,
        _ammpool_bump,
        _ammpool_padding,
        // Amm pool ends
        _start_ts,
        expire_ts,
        _order_step_size,
        _min_order_size,
        _min_lp_amount,
        _min_liquidation_size,
        _market_index,
        _margin_index,
        _lp_margin_index,
        _margin_type,
        _lp_margin_type,
        _margin_decimals,
        _lp_margin_decimals,
        _collateral_ratio_initial,
        _collateral_ratio_initial_pre_expiry,
        _collateral_ratio_maintenance,
        _active_ratio_coef,
        _max_open_interest,
        _open_interest,
        _number_of_active_users,
        _number_of_active_lps,
        _status,
        _market_type,
        _padding2,
        _net_quote_amount,
        _net_base_amount,
        _last_rate,
        _total_quote_asset_amount,
        _total_margin_amount,
        _net_quote_amount_realized,
        _social_loss_margin_position,
        _social_loss_yield_position,
        _tick_lower_index,
        tick_upper_index,
        _insurance_margin_position,
        _insurance_yield_position,
        _keeper_fee,
        _lp_accounts_processed,
        _implied_rate,
        _lp_quote_amount,
        _lp_base_amount,
        _number_of_processed_users,
        _expire_update_ts,
        _expire_total_debt,
        _expire_total_margin,
        _expire_total_pos_quote_amount,
        _expire_total_debt_covered,
        _total_reserve_base_amount,
        _liq_fee_rate,
        _protocol_fee,
        _epoch_update_status,
        _padding3,
        _earn_net_quote_amount_realized,
        _total_base_quota,
        _epoch_update_end_ts,
        _padding,
    ) = array_refs![
        src, 32, 32, 32, 32, 32, 32, 16, 16, 8, 8, 32, 32, 16, 32, 32, 16, 8, 384, 32, 4, 2, 2, 2,
        2, 2, 2, 1, 7, 8, 8, 8, 8, 8, 8, 4, 4, 4, 1, 1, 1, 1, 8, 8, 8, 8, 8, 8, 8, 8, 1, 1, 6, 8,
        8, 8, 8, 8, 8, 48, 64, 4, 4, 48, 64, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 1, 7, 8, 8, 8,
        42
    ];

    (
        i64::from_le_bytes(*expire_ts),
        i32::from_le_bytes(*tick_upper_index),
        Pubkey::new_from_array(*oracle),
    )
}

fn unpack_oracle(data: &[u8]) -> i64 {
    let src = array_ref![data[8..], 0, 120];
    let (
        _admin,
        _name,
        _last_rate,
        _rate,
        _market_rate,
        _ts,
        _decimals,
        _padding,
        epoch_start_timestamp,
        _last_epoch_start_timestamp,
    ) = array_refs![src, 32, 32, 8, 8, 8, 8, 4, 4, 8, 8];    

    i64::from_le_bytes(*epoch_start_timestamp)
}

pub fn get_price<'a, 'b>(
    yield_market: &AccountInfo,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>, 
    clock: &Clock
) -> Result<DatedPrice> 
where
    'a: 'b,
{
    let (expire_ts, upper_tick, oracle_pubkey) = unpack_yield_market(&yield_market.data.borrow());
    let oracle = extra_accounts.next().ok_or(ScopeError::AccountsAndTokenMismatch)?;
    require!(oracle_pubkey == oracle.key(), ScopeError::UnexpectedAccount);

    let epoch_start_ts = unpack_oracle(&oracle.data.borrow());

    let price = if epoch_start_ts >= expire_ts {
        Price {value: 1_000_000_000u64, exp: 9}
    } else {
        get_pt_price(upper_tick)?
    };

    let dated_price = DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp).unwrap(),
        ..Default::default()
    };
    Ok(dated_price)
}

pub fn validate_account(account: &Option<AccountInfo>) -> Result<()> {
    let Some(_yield_market) = account else {
        msg!("No pool account provided");
        return err!(ScopeError::PriceNotValid);
    };

    // Check account

    Ok(())
}