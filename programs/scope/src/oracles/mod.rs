pub mod ctokens;
#[cfg(feature = "yvaults")]
pub mod ktokens;
#[cfg(feature = "yvaults")]
pub mod ktokens_token_x;

pub mod jupiter_lp;
pub mod meteora_dlmm;
pub mod msol_stake;
pub mod orca_whirlpool;
pub mod pyth;
pub mod pyth_ema;
pub mod raydium_ammv3;
pub mod spl_stake;
pub mod switchboard_v2;
pub mod twap;

use std::ops::Deref;

use anchor_lang::{accounts::account_loader::AccountLoader, prelude::*};
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{DatedPrice, OracleMappings, OraclePrices, OracleTwaps, ScopeError};

use self::ktokens_token_x::TokenTypes;

pub fn check_context<T>(ctx: &Context<T>) -> Result<()> {
    //make sure there are no extra accounts
    if !ctx.remaining_accounts.is_empty() {
        return err!(ScopeError::UnexpectedAccount);
    }

    Ok(())
}

#[derive(IntoPrimitive, TryFromPrimitive, Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum OracleType {
    Pyth = 0,
    /// Deprecated (formerly SwitchboardV1)
    // Do not remove - breaks the typescript idl codegen
    DeprecatedPlaceholder1 = 1,
    SwitchboardV2 = 2,
    /// Deprecated (formerly YiToken)
    // Do not remove - breaks the typescript idl codegen
    DeprecatedPlaceholder2 = 3,
    /// Solend tokens
    CToken = 4,
    /// SPL Stake Pool token (giving the stake rate in SOL):
    /// This oracle type provide a reference and is not meant to be used directly
    /// to get the value of the token because of different limitations:
    /// - The stake rate is only updated once per epoch and can be delayed by one hour after a new epoch.
    /// - The stake rate does not take into account the fees that applies on staking or unstaking.
    /// - Unstaking is not immediate and the market price is often lower than the "stake price".
    SplStake = 5,
    /// KTokens from Kamino
    KToken = 6,
    /// Pyth Exponentially-Weighted Moving Average
    PythEMA = 7,
    /// MSOL Stake Pool token
    /// This oracle type provide a reference and is not meant to be used directly
    /// to get the value of the token because of different limitations:
    /// - The stake rate is only updated once per epoch.
    /// - The stake rate does not take into account the fees that applies on staking or unstaking.
    /// - Unstaking is not immediate and the market price is often lower than the "stake price".
    MsolStake = 8,
    /// Number of token A for 1 kToken
    KTokenToTokenA = 9,
    /// Number of token B for 1 kToken
    KTokenToTokenB = 10,
    /// Jupiter's perpetual LP tokens
    /// This oracle type provide a reference and is not meant to be used directly because
    /// the price is just fetched from the Jupiter's pool and can be stalled.
    JupiterLpFetch = 11,
    /// Scope twap
    ScopeTwap = 12,
    /// Orca's whirlpool price (CLMM) A to B
    OrcaWhirlpoolAtoB = 13,
    /// Orca's whirlpool price (CLMM) B to A
    OrcaWhirlpoolBtoA = 14,
    /// Raydium's AMM v3 price (CLMM) A to B
    RaydiumAmmV3AtoB = 15,
    /// Raydium's AMM v3 price (CLMM) B to A
    RaydiumAmmV3BtoA = 16,
    /// Jupiter's perpetual LP tokens computed from current oracle prices
    JupiterLpCompute = 17,
    /// Meteora's DLMM A to B
    MeteoraDlmmAtoB = 18,
    /// Meteora's DLMM B to A
    MeteoraDlmmBtoA = 19,
    /// Jupiter's perpetual LP tokens computed from scope prices
    JupiterLpScope = 20,
}

impl OracleType {
    pub fn is_twap(&self) -> bool {
        matches!(self, OracleType::ScopeTwap)
    }

    /// Get the number of compute unit needed to refresh the price of a token
    pub fn get_update_cu_budget(&self) -> u32 {
        match self {
            OracleType::Pyth => 20_000,
            OracleType::SwitchboardV2 => 30_000,
            OracleType::CToken => 130_000,
            OracleType::SplStake => 20_000,
            OracleType::KToken => 120_000,
            OracleType::PythEMA => 20_000,
            OracleType::KTokenToTokenA | OracleType::KTokenToTokenB => 100_000,
            OracleType::MsolStake => 20_000,
            OracleType::JupiterLpFetch => 40_000,
            OracleType::ScopeTwap => 15_000,
            OracleType::OrcaWhirlpoolAtoB
            | OracleType::OrcaWhirlpoolBtoA
            | OracleType::RaydiumAmmV3AtoB
            | OracleType::RaydiumAmmV3BtoA => 20_000,
            OracleType::MeteoraDlmmAtoB | OracleType::MeteoraDlmmBtoA => 30_000,
            OracleType::JupiterLpCompute | OracleType::JupiterLpScope => 120_000,
            OracleType::DeprecatedPlaceholder1 | OracleType::DeprecatedPlaceholder2 => {
                panic!("DeprecatedPlaceholder is not a valid oracle type")
            }
        }
    }
}

/// Get the price for a given oracle type
///
/// The `base_account` should have been checked against the oracle mapping
/// If needed the `extra_accounts` will be extracted from the provided iterator and checked
/// with the data contained in the `base_account`
#[allow(clippy::too_many_arguments)]
pub fn get_price<'a, 'b>(
    price_type: OracleType,
    base_account: &AccountInfo<'a>,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
    clock: &Clock,
    oracle_twaps: &OracleTwaps,
    oracle_mappings: &OracleMappings,
    oracle_prices: &AccountLoader<OraclePrices>,
    index: usize,
) -> crate::Result<DatedPrice>
where
    'a: 'b,
{
    match price_type {
        OracleType::Pyth => pyth::get_price(base_account, clock),
        OracleType::SwitchboardV2 => switchboard_v2::get_price(base_account),
        OracleType::CToken => ctokens::get_price(base_account, clock),
        OracleType::SplStake => spl_stake::get_price(base_account, clock),
        #[cfg(not(feature = "yvaults"))]
        OracleType::KToken => {
            panic!("yvaults feature is not enabled, KToken oracle type is not available")
        }
        OracleType::PythEMA => pyth_ema::get_price(base_account, clock),
        #[cfg(feature = "yvaults")]
        OracleType::KToken => ktokens::get_price(base_account, clock, extra_accounts),
        #[cfg(feature = "yvaults")]
        OracleType::KTokenToTokenA => ktokens_token_x::get_token_x_per_share(
            base_account,
            clock,
            extra_accounts,
            TokenTypes::TokenA,
        ),
        #[cfg(feature = "yvaults")]
        OracleType::KTokenToTokenB => ktokens_token_x::get_token_x_per_share(
            base_account,
            clock,
            extra_accounts,
            TokenTypes::TokenB,
        ),
        #[cfg(not(feature = "yvaults"))]
        OracleType::KTokenToTokenA => {
            panic!("yvaults feature is not enabled, KToken oracle type is not available")
        }
        #[cfg(not(feature = "yvaults"))]
        OracleType::KTokenToTokenB => {
            panic!("yvaults feature is not enabled, KToken oracle type is not available")
        }
        OracleType::MsolStake => msol_stake::get_price(base_account, clock),
        OracleType::JupiterLpFetch => {
            jupiter_lp::get_price_no_recompute(base_account, clock, extra_accounts)
        }
        OracleType::ScopeTwap => twap::get_price(oracle_mappings, oracle_twaps, index, clock),
        OracleType::OrcaWhirlpoolAtoB => {
            orca_whirlpool::get_price(true, base_account, clock, extra_accounts)
        }
        OracleType::OrcaWhirlpoolBtoA => {
            orca_whirlpool::get_price(false, base_account, clock, extra_accounts)
        }
        OracleType::RaydiumAmmV3AtoB => raydium_ammv3::get_price(true, base_account, clock),
        OracleType::RaydiumAmmV3BtoA => raydium_ammv3::get_price(false, base_account, clock),
        OracleType::MeteoraDlmmAtoB => {
            meteora_dlmm::get_price(true, base_account, clock, extra_accounts)
        }
        OracleType::MeteoraDlmmBtoA => {
            meteora_dlmm::get_price(false, base_account, clock, extra_accounts)
        }
        OracleType::JupiterLpCompute => {
            jupiter_lp::get_price_recomputed(base_account, clock, extra_accounts)
        }
        OracleType::JupiterLpScope => jupiter_lp::get_price_recomputed_scope(
            index,
            base_account,
            clock,
            &oracle_prices.key(),
            oracle_prices.load()?.deref(),
            extra_accounts,
        ),
        OracleType::DeprecatedPlaceholder1 | OracleType::DeprecatedPlaceholder2 => {
            panic!("DeprecatedPlaceholder is not a valid oracle type")
        }
    }
}

/// Validate the given account as being an appropriate price account for the
/// given oracle type.
///
/// This function shall be called before update of oracle mappings
pub fn validate_oracle_account(
    price_type: OracleType,
    price_account: &AccountInfo,
) -> crate::Result<()> {
    match price_type {
        OracleType::Pyth => pyth::validate_pyth_price_info(price_account),
        OracleType::SwitchboardV2 => Ok(()), // TODO at least check account ownership?
        OracleType::CToken => Ok(()),        // TODO how shall we validate ctoken account?
        OracleType::SplStake => Ok(()),      // TODO, should validate ownership of the account
        OracleType::KToken => Ok(()), // TODO, should validate ownership of the ktoken account
        OracleType::KTokenToTokenA => Ok(()), // TODO, should validate ownership of the ktoken account
        OracleType::KTokenToTokenB => Ok(()), // TODO, should validate ownership of the ktoken account
        OracleType::PythEMA => pyth::validate_pyth_price_info(price_account),
        OracleType::MsolStake => Ok(()),
        OracleType::JupiterLpFetch | OracleType::JupiterLpCompute | OracleType::JupiterLpScope => {
            jupiter_lp::validate_jlp_pool(price_account)
        }
        OracleType::ScopeTwap => twap::validate_price_account(price_account),
        OracleType::OrcaWhirlpoolAtoB | OracleType::OrcaWhirlpoolBtoA => {
            orca_whirlpool::validate_pool_account(price_account)
        }
        OracleType::RaydiumAmmV3AtoB | OracleType::RaydiumAmmV3BtoA => {
            raydium_ammv3::validate_pool_account(price_account)
        }
        OracleType::MeteoraDlmmAtoB | OracleType::MeteoraDlmmBtoA => {
            meteora_dlmm::validate_pool_account(price_account)
        }
        OracleType::DeprecatedPlaceholder1 | OracleType::DeprecatedPlaceholder2 => {
            panic!("DeprecatedPlaceholder is not a valid oracle type")
        }
    }
}
