use std::ops::Deref;

use anchor_lang::{prelude::*, Result};
use yvaults::{
    self as kamino,
    clmm::Clmm,
    operations::vault_operations::common,
    state::CollateralToken,
    state::{CollateralInfos, GlobalConfig, WhirlpoolStrategy},
    utils::{enums::LiquidityCalculationMode, price::TokenPrices},
};

use crate::ScopeResult;
use crate::{
    utils::{
        math::{price_of_lamports_to_price_of_tokens, u64_div_to_price},
        zero_copy_deserialize,
    },
    DatedPrice, Price, ScopeError,
};

use super::ktokens::price_utils;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenTypes {
    TokenA,
    TokenB,
}

/// Gives the number of token (A or B) per kToken
///
/// This is the total holdings of the given underlying asset divided by the number of shares issued
/// Underlying asset is the sum of invested, uninvested and fees of either token_a or token_b
/// Reward tokens are included if equal to token_a or token_b
///
/// The kToken price timestamp is current time
pub fn get_token_x_per_share<'a, 'b>(
    k_account: &AccountInfo,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
    token: TokenTypes,
) -> ScopeResult<DatedPrice>
where
    'a: 'b,
{
    // Get the root account
    let strategy_account_ref = zero_copy_deserialize::<WhirlpoolStrategy>(k_account)?;

    // extract the accounts from extra iterator
    let global_config_account_info = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;
    // Get the global config account (checked below)
    let global_config_account_ref =
        zero_copy_deserialize::<GlobalConfig>(global_config_account_info)?;

    let collateral_infos_account_info = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;

    let pool_account_info = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;

    let position_account_info = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;

    let scope_prices_account_info = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;

    let account_check = |account: &AccountInfo, expected, name| {
        let pk = account.key();
        if pk != expected {
            msg!(
                "Ktoken token per share: received account {} for {} is not the one expected ({})",
                pk,
                name,
                expected
            );
            Err(ScopeError::UnexpectedAccount)
        } else {
            Ok(())
        }
    };

    // Check the pubkeys
    account_check(
        global_config_account_info,
        strategy_account_ref.global_config,
        "global_config",
    )?;
    account_check(
        collateral_infos_account_info,
        global_config_account_ref.token_infos,
        "collateral_infos",
    )?;
    account_check(pool_account_info, strategy_account_ref.pool, "pool")?;
    account_check(
        position_account_info,
        strategy_account_ref.position,
        "position",
    )?;
    account_check(
        scope_prices_account_info,
        strategy_account_ref.scope_prices,
        "scope_prices",
    )?;

    // Deserialize accounts
    let collateral_infos_ref =
        zero_copy_deserialize::<CollateralInfos>(collateral_infos_account_info)?;
    let scope_prices_ref =
        zero_copy_deserialize::<kamino::scope::OraclePrices>(scope_prices_account_info)?;

    let clmm = super::ktokens::get_clmm(
        pool_account_info,
        position_account_info,
        &strategy_account_ref,
    )?;

    let token_prices = kamino::utils::scope::get_prices_from_data(
        scope_prices_ref.deref(),
        &collateral_infos_ref.infos,
        &strategy_account_ref,
        None,
        clock.slot,
    )
    .map_err(|_| ScopeError::KTokenUnderlyingPriceNotValid)?;

    let num_token_x =
        holdings_of_token_x(&strategy_account_ref, clmm.as_ref(), &token_prices, token).map_err(
            |e| {
                msg!("Error while computing the Ktoken pool holdings: {:?}", e);
                ScopeError::KTokenHoldingsCalculationError
            },
        )?;
    let num_shares = strategy_account_ref.shares_issued;

    // Get the least-recently updated component price from both scope chains
    let last_updated_slot = clock.slot;
    let unix_timestamp = u64::try_from(clock.unix_timestamp).expect("Unix timestamp negative");

    let price = if num_shares == 0 {
        // Assume price is 0 without shares issued
        Price { value: 0, exp: 1 }
    } else {
        let price_lamport_to_lamport = u64_div_to_price(num_token_x, num_shares);

        // Final price need to be adjusted by the number of decimals of the kToken and the token X
        let share_decimals = strategy_account_ref.shares_mint_decimals;
        let token_decimals = match token {
            TokenTypes::TokenA => strategy_account_ref.token_a_mint_decimals,
            TokenTypes::TokenB => strategy_account_ref.token_b_mint_decimals,
        };

        price_of_lamports_to_price_of_tokens(
            price_lamport_to_lamport,
            share_decimals,
            token_decimals,
        )
    };

    Ok(DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        ..Default::default()
    })
}

/// Returns amount of token x in the strategy
/// Use a sqrt price derived from price_a and price_b, not from the pool as it cannot be considered reliable
pub fn holdings_of_token_x(
    strategy: &WhirlpoolStrategy,
    clmm: &dyn Clmm,
    prices: &TokenPrices,
    token: TokenTypes,
) -> Result<u64> {
    // https://github.com/0xparashar/UniV3NFTOracle/blob/master/contracts/UniV3NFTOracle.sol#L27
    // compute sqrt price derived from price_a and price_b
    // We still use the pool price to compute the sqrt price but print this one as a reference
    let pool_sqrt_price_from_oracle_prices = price_utils::sqrt_price_from_scope_prices(
        &prices.get(
            CollateralToken::try_from(strategy.token_a_collateral_id)
                .map_err(|_| ScopeError::ConversionFailure)?,
        )?,
        &prices.get(
            CollateralToken::try_from(strategy.token_b_collateral_id)
                .map_err(|_| ScopeError::ConversionFailure)?,
        )?,
        strategy.token_a_mint_decimals,
        strategy.token_b_mint_decimals,
    )?;

    let pool_sqrt_price = clmm.get_current_sqrt_price();

    msg!("[KToken to Token X] pool_sqrt_price: {pool_sqrt_price} vs sqrt_price_from_oracle_prices: {pool_sqrt_price_from_oracle_prices}",);

    let (available, invested, fees) = common::underlying_inventory(
        strategy,
        clmm,
        LiquidityCalculationMode::Deposit,
        clmm.get_position_liquidity()?,
        pool_sqrt_price,
    )?;

    let (available, invested, fees) = match token {
        TokenTypes::TokenA => (available.a, invested.a, fees.a),
        TokenTypes::TokenB => (available.b, invested.b, fees.b),
    };

    // rewards
    let r = clmm
        .get_position_pending_rewards(Some(strategy.token_a_mint), Some(strategy.token_b_mint))?;

    let rewards = [
        &r.reward_0,
        &r.reward_1,
        &r.reward_2,
        &r.reward_3,
        &r.reward_4,
        &r.reward_5,
    ];
    let sum_rewards_x = rewards.into_iter().fold(0_u64, |acc, x| {
        if x.is_token_a && token == TokenTypes::TokenA
            || x.is_token_b && token == TokenTypes::TokenB
        {
            acc + x.amount
        } else {
            acc
        }
    });

    Ok(available + invested + fees + sum_rewards_x)
}
