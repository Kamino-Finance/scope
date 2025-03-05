use std::ops::Deref;

use anchor_lang::{prelude::*, Result};
use decimal_wad::{decimal::Decimal, rate::U128};
use kamino::{
    clmm::{orca_clmm::OrcaClmm, Clmm},
    operations::vault_operations::common::underlying_unit,
    raydium_amm_v3::states::{PersonalPositionState as RaydiumPosition, PoolState as RaydiumPool},
    raydium_clmm::RaydiumClmm,
    state::{CollateralInfos, GlobalConfig, WhirlpoolStrategy},
    utils::types::DEX,
    whirlpool::state::{Position as OrcaPosition, Whirlpool as OrcaWhirlpool},
};
use yvaults as kamino;
use yvaults::{
    operations::vault_operations::common,
    state::CollateralToken,
    utils::{
        enums::LiquidityCalculationMode,
        price::TokenPrices,
        scope::ScopePrices,
        types::{Holdings, RewardsAmounts},
    },
};

use crate::{
    utils::{account_deserialize, zero_copy_deserialize},
    warn, DatedPrice, Price, ScopeError, ScopeResult,
};

const SCALE_DECIMALS: u8 = 6;
const SCALE_FACTOR: u64 = 10_u64.pow(SCALE_DECIMALS as u32);

/// Gives the price of 1 kToken in USD
///
/// This is the price of the underlying assets in USD divided by the number of shares issued
///
/// Underlying assets is the sum of invested, uninvested and fees of token_a and token_b
///
/// Reward tokens are excluded from the calculation as they are generally lower value/mcap and can be manipulated
///
/// When calculating invested amounts, a sqrt price derived from scope price_a and price_b is used to determine the 'correct' ratio of underlying assets, the sqrt price of the pool cannot be considered reliable
///
/// The kToken price timestamp is taken from the least-recently updated price in the scope price chains of token_a and token_b
pub fn get_price<'a, 'b>(
    k_account: &AccountInfo,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
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
            warn!(
                "Ktoken received account {} for {} is not the one expected ({})",
                pk, name, expected
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

    let clmm = get_clmm(
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

    let holdings = holdings(&strategy_account_ref, clmm.as_ref(), &token_prices)?;

    let price = get_price_per_full_share(
        holdings.total_sum,
        strategy_account_ref.shares_issued,
        strategy_account_ref.shares_mint_decimals,
    );

    // Get the least-recently updated component price from both scope chains
    let (last_updated_slot, unix_timestamp) = get_component_px_last_update(
        &scope_prices_ref,
        &collateral_infos_ref,
        &strategy_account_ref,
    )
    .map_err(|e| {
        warn!("Error getting component price last update: {:?}", e);
        e
    })?;

    Ok(DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        ..Default::default()
    })
}

pub(super) fn get_clmm<'a, 'info>(
    pool: &'a AccountInfo<'info>,
    position: &'a AccountInfo<'info>,
    strategy: &WhirlpoolStrategy,
) -> ScopeResult<Box<dyn Clmm + 'a>> {
    let dex = DEX::try_from(strategy.strategy_dex).unwrap();
    let clmm: Box<dyn Clmm> = match dex {
        DEX::Orca => {
            let pool = account_deserialize::<OrcaWhirlpool>(pool)
                .map_err(|_| ScopeError::UnableToDeserializeAccount)?;
            let position = if strategy.position != Pubkey::default() {
                let position = account_deserialize::<OrcaPosition>(position)
                    .map_err(|_| ScopeError::UnableToDeserializeAccount)?;
                Some(position)
            } else {
                None
            };
            Box::new(OrcaClmm {
                pool,
                position,
                lower_tick_array: None,
                upper_tick_array: None,
            })
        }
        DEX::Raydium => {
            let pool = zero_copy_deserialize::<RaydiumPool>(pool)?;
            let position = if strategy.position != Pubkey::default() {
                let position = account_deserialize::<RaydiumPosition>(position)?;
                Some(position)
            } else {
                None
            };
            Box::new(RaydiumClmm {
                pool,
                position,
                protocol_position: None,
                lower_tick_array: None,
                upper_tick_array: None,
            })
        }
    };
    Ok(clmm)
}

/// Returns the last updated slot and unix timestamp of the least-recently updated component price
/// Excludes rewards prices as they do not form part of the calculation
fn get_component_px_last_update(
    scope_prices: &ScopePrices,
    collateral_infos: &CollateralInfos,
    strategy: &WhirlpoolStrategy,
) -> ScopeResult<(u64, u64)> {
    let token_a = yvaults::state::CollateralToken::try_from(strategy.token_a_collateral_id)
        .map_err(|_| ScopeError::ConversionFailure)?;
    let token_b = yvaults::state::CollateralToken::try_from(strategy.token_b_collateral_id)
        .map_err(|_| ScopeError::ConversionFailure)?;

    let collateral_info_a = collateral_infos.infos[token_a.to_usize()];
    let collateral_info_b = collateral_infos.infos[token_b.to_usize()];
    let token_a_chain: yvaults::utils::scope::ScopeConversionChain =
        collateral_info_a
            .try_into()
            .map_err(|_| ScopeError::BadScopeChainOrPrices)?;
    let token_b_chain: yvaults::utils::scope::ScopeConversionChain =
        collateral_info_b
            .try_into()
            .map_err(|_| ScopeError::BadScopeChainOrPrices)?;

    let price_chain = token_a_chain
        .iter()
        .chain(token_b_chain.iter())
        .map(|&token_id| scope_prices.prices[usize::from(token_id)])
        .collect::<Vec<yvaults::scope::DatedPrice>>();

    let (last_updated_slot, unix_timestamp): (u64, u64) =
        price_chain
            .iter()
            .fold((0_u64, 0_u64), |(slot, ts), price| {
                if slot == 0 || price.last_updated_slot.lt(&slot) {
                    (price.last_updated_slot, price.unix_timestamp)
                } else {
                    (slot, ts)
                }
            });

    Ok((last_updated_slot, unix_timestamp))
}

/// Returns the holdings of the strategy
/// Use a sqrt price derived from price_a and price_b, not from the pool as it cannot be considered reliable
/// Exclude rewards from the holdings calculation, as they are generally low value/mcap and can be manipulated
pub fn holdings(
    strategy: &WhirlpoolStrategy,
    clmm: &dyn Clmm,
    prices: &TokenPrices,
) -> ScopeResult<Holdings> {
    // https://github.com/0xparashar/UniV3NFTOracle/blob/master/contracts/UniV3NFTOracle.sol#L27
    // We are using the sqrt price derived from price_a and price_b
    // instead of the whirlpool price which could be manipulated/stale
    let pool_sqrt_price = price_utils::sqrt_price_from_scope_prices(
        &prices
            .get(
                CollateralToken::try_from(strategy.token_a_collateral_id)
                    .map_err(|_| ScopeError::ConversionFailure)?,
            )
            .map_err(|_| ScopeError::KTokenUnderlyingPriceNotValid)?,
        &prices
            .get(
                CollateralToken::try_from(strategy.token_b_collateral_id)
                    .map_err(|_| ScopeError::ConversionFailure)?,
            )
            .map_err(|_| ScopeError::KTokenUnderlyingPriceNotValid)?,
        strategy.token_a_mint_decimals,
        strategy.token_b_mint_decimals,
    )
    .map_err(|e| {
        warn!("Error calculating sqrt price: {:?}", e);
        ScopeError::ConversionFailure
    })?;

    if cfg!(feature = "debug") {
        let w = price_utils::calc_price_from_sqrt_price(
            clmm.get_current_sqrt_price(),
            strategy.token_a_mint_decimals,
            strategy.token_b_mint_decimals,
        );
        let o = price_utils::calc_price_from_sqrt_price(
            pool_sqrt_price,
            strategy.token_a_mint_decimals,
            strategy.token_b_mint_decimals,
        );
        let diff = (w - o).abs() / w;
        warn!("o: {} w: {} d: {}%", w, o, diff * 100.0);
    }

    holdings_no_rewards(strategy, clmm, prices, pool_sqrt_price).map_err(|e| {
        warn!("Error calculating holdings: {:?}", e);
        ScopeError::KTokenHoldingsCalculationError
    })
}

pub fn holdings_no_rewards(
    strategy: &WhirlpoolStrategy,
    clmm: &dyn Clmm,
    prices: &TokenPrices,
    pool_sqrt_price: u128,
) -> Result<Holdings> {
    let (available, invested, fees) = common::underlying_inventory(
        strategy,
        clmm,
        LiquidityCalculationMode::Deposit,
        clmm.get_position_liquidity()?,
        pool_sqrt_price,
    )?;
    // exclude rewards
    let rewards = RewardsAmounts::default();

    let holdings = common::holdings_usd(strategy, available, invested, fees, rewards, prices)?;

    Ok(holdings)
}

fn get_price_per_full_share(
    total_holdings_value_scaled: U128,
    shares_issued: u64,
    shares_decimals: u64,
) -> Price {
    if shares_issued == 0 {
        // Assume price is 0 without shares issued
        Price { value: 0, exp: 1 }
    } else {
        let price_decimal = Decimal::from(underlying_unit(shares_decimals))
            * total_holdings_value_scaled
            / (u128::from(SCALE_FACTOR) * u128::from(shares_issued));
        (price_decimal).into()
    }
}

pub(super) mod price_utils {
    use decimal_wad::rate::U128;

    use super::*;

    // Helper

    fn pow(base: u64, exp: u64) -> U128 {
        U128::from(base).pow(U128::from(exp))
    }

    fn abs_diff(a: i32, b: i32) -> u32 {
        if a > b {
            a.checked_sub(b).unwrap().try_into().unwrap()
        } else {
            b.checked_sub(a).unwrap().try_into().unwrap()
        }
    }

    fn decimals_factor(decimals_a: u64, decimals_b: u64) -> Result<(U128, u64)> {
        let decimals_a = i32::try_from(decimals_a).map_err(|_e| ScopeError::IntegerOverflow)?;
        let decimals_b = i32::try_from(decimals_b).map_err(|_e| ScopeError::IntegerOverflow)?;

        let diff = abs_diff(decimals_a, decimals_b);
        let factor = U128::from(10_u64.pow(diff));
        Ok((factor, u64::from(diff)))
    }

    pub fn a_to_b(
        a: &yvaults::utils::price::Price,
        b: &yvaults::utils::price::Price,
    ) -> Result<yvaults::utils::price::Price> {
        let a = crate::Price {
            value: a.value,
            exp: a.exp,
        };
        let b = crate::Price {
            value: b.value,
            exp: b.exp,
        };

        let price_a_dec = Decimal::from(a);
        let price_b_dec = Decimal::from(b);

        let price_a_to_b_dec = price_a_dec / price_b_dec;

        let price_a_to_b: crate::Price = price_a_to_b_dec.into();

        Ok(yvaults::utils::price::Price {
            value: price_a_to_b.value,
            exp: price_a_to_b.exp,
        })
    }

    pub fn calc_sqrt_price_from_scope_price(
        price: &yvaults::utils::price::Price,
        decimals_a: u64,
        decimals_b: u64,
    ) -> Result<u128> {
        // Normally we calculate sqrt price from a float price as following:
        // px = sqrt(price * 10 ^ (decimals_b - decimals_a)) * 2 ** 64

        // But scope price is scaled by 10 ** exp so, to obtain it, we need to divide by sqrt(10 ** exp)
        // x = sqrt(scaled_price * 10 ^ (decimals_b - decimals_a)) * 2 ** 64
        // px = x / sqrt(10 ** exp)

        let (decimals_factor, decimals_diff) = decimals_factor(decimals_a, decimals_b)?;
        let px = U128::from(price.value);
        let (scaled_price, final_exp) = if decimals_b > decimals_a {
            (px.checked_mul(decimals_factor).unwrap(), price.exp)
        } else {
            // If we divide by 10 ^ (decimals_a - decimals_b) here we lose precision
            // So instead we lift the price even more (by the diff) and assume a bigger exp
            (px, price.exp.checked_add(decimals_diff).unwrap())
        };

        let two_factor = pow(2, 64);
        let x = scaled_price
            .integer_sqrt()
            .checked_mul(two_factor)
            .ok_or(ScopeError::IntegerOverflow)?;

        let sqrt_factor = pow(10, final_exp).integer_sqrt();

        Ok(x.checked_div(sqrt_factor)
            .ok_or(ScopeError::IntegerOverflow)?
            .as_u128())
    }

    pub fn sqrt_price_from_scope_prices(
        price_a: &yvaults::utils::price::Price,
        price_b: &yvaults::utils::price::Price,
        decimals_a: u64,
        decimals_b: u64,
    ) -> Result<u128> {
        calc_sqrt_price_from_scope_price(&a_to_b(price_a, price_b)?, decimals_a, decimals_b)
    }

    pub fn calc_price_from_sqrt_price(price: u128, decimals_a: u64, decimals_b: u64) -> f64 {
        let sqrt_price_x_64 = price as f64;
        (sqrt_price_x_64 / 2.0_f64.powf(64.0)).powf(2.0)
            * 10.0_f64.powi(decimals_a as i32 - decimals_b as i32)
    }
}
