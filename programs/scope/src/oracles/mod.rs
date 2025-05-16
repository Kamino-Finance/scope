pub mod ctokens;
#[cfg(feature = "yvaults")]
pub mod ktokens;
#[cfg(feature = "yvaults")]
pub mod ktokens_token_x;

pub mod chainlink;
pub mod discount_to_maturity;
pub mod jito_restaking;
pub mod jupiter_lp;
pub mod meteora_dlmm;
pub mod most_recent_of;
pub mod msol_stake;
pub mod orca_whirlpool;
pub mod pyth;
pub mod pyth_ema;
pub mod pyth_lazer;
pub mod pyth_pull;
pub mod pyth_pull_ema;
pub mod raydium_ammv3;
pub mod spl_stake;
pub mod switchboard_on_demand;
pub mod switchboard_v2;
pub mod twap;

use std::ops::Deref;

use anchor_lang::{accounts::account_loader::AccountLoader, prelude::*};
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "yvaults")]
use self::ktokens_token_x::TokenTypes;
use crate::{warn, DatedPrice, OracleMappings, OraclePrices, OracleTwaps, Price, ScopeError};

pub fn check_context<T>(ctx: &Context<T>) -> Result<()> {
    //make sure there are no extra accounts
    if !ctx.remaining_accounts.is_empty() {
        return err!(ScopeError::UnexpectedAccount);
    }

    Ok(())
}

#[derive(Default, IntoPrimitive, TryFromPrimitive, Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum OracleType {
    #[default]
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
    /// Pyth Pull oracles
    PythPull = 21,
    /// Pyth Pull oracles EMA
    PythPullEMA = 22,
    /// Fixed price oracle
    FixedPrice = 23,
    /// Switchboard on demand
    SwitchboardOnDemand = 24,
    /// Jito restaking tokens
    JitoRestaking = 25,
    /// Chainlink oracles
    Chainlink = 26,
    /// Discount oracle, compute the price with a linear discount rate until a maturity date
    /// After maturity date the price is set to 1
    DiscountToMaturity = 27,
    /// Keeps track of a few prices and makes sure they are recent enough and they are in sync,
    /// ie. they don't diverge more than a specified limit
    MostRecentOf = 28,
    /// Pyth Lazer oracle
    PythLazer = 29,
}

impl OracleType {
    pub fn is_twap(&self) -> bool {
        matches!(self, OracleType::ScopeTwap)
    }

    /// Get the number of compute unit needed to refresh the price of a token
    pub fn get_update_cu_budget(&self) -> u32 {
        match self {
            OracleType::FixedPrice => 10_000,
            OracleType::PythPull => 20_000,
            OracleType::PythPullEMA => 20_000,
            OracleType::Pyth => 30_000,
            OracleType::SwitchboardV2 => 30_000,
            OracleType::SwitchboardOnDemand => 30_000,
            OracleType::CToken => 130_000,
            OracleType::SplStake => 20_000,
            OracleType::KToken => 120_000,
            OracleType::PythEMA => 30_000,
            OracleType::KTokenToTokenA | OracleType::KTokenToTokenB => 100_000,
            OracleType::MsolStake => 20_000,
            OracleType::JupiterLpFetch => 40_000,
            OracleType::ScopeTwap => 30_000,
            OracleType::OrcaWhirlpoolAtoB
            | OracleType::OrcaWhirlpoolBtoA
            | OracleType::RaydiumAmmV3AtoB
            | OracleType::RaydiumAmmV3BtoA => 25_000,
            OracleType::MeteoraDlmmAtoB | OracleType::MeteoraDlmmBtoA => 30_000,
            OracleType::JupiterLpCompute | OracleType::JupiterLpScope => 120_000,
            OracleType::JitoRestaking => 25_000,
            OracleType::DiscountToMaturity => 30_000,
            // Chainlink oracles are not updated through normal refresh ixs
            OracleType::Chainlink => 0,
            OracleType::MostRecentOf => 35_000,
            // PythLazer oracle is not updated through normal refresh ixs
            OracleType::PythLazer => 0,
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
pub fn get_non_zero_price<'a, 'b>(
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
    let price = match price_type {
        OracleType::Pyth => pyth::get_price(base_account, clock),
        OracleType::PythPull => pyth_pull::get_price(base_account, clock),
        OracleType::PythPullEMA => pyth_pull_ema::get_price(base_account, clock),
        OracleType::SwitchboardV2 => switchboard_v2::get_price(base_account).map_err(Into::into),
        OracleType::SwitchboardOnDemand => {
            switchboard_on_demand::get_price(base_account, clock).map_err(Into::into)
        }
        OracleType::CToken => ctokens::get_price(base_account, clock),
        OracleType::SplStake => spl_stake::get_price(base_account, clock),
        #[cfg(not(feature = "yvaults"))]
        OracleType::KToken => {
            panic!("yvaults feature is not enabled, KToken oracle type is not available")
        }
        OracleType::PythEMA => pyth_ema::get_price(base_account, clock),
        #[cfg(feature = "yvaults")]
        OracleType::KToken => {
            ktokens::get_price(base_account, clock, extra_accounts).map_err(|e| {
                warn!("Error getting KToken price: {:?}", e);
                e.into()
            })
        }
        #[cfg(feature = "yvaults")]
        OracleType::KTokenToTokenA => ktokens_token_x::get_token_x_per_share(
            base_account,
            clock,
            extra_accounts,
            TokenTypes::TokenA,
        )
        .map_err(|e| {
            warn!("Error getting KToken share ratio: {:?}", e);
            e.into()
        }),
        #[cfg(feature = "yvaults")]
        OracleType::KTokenToTokenB => ktokens_token_x::get_token_x_per_share(
            base_account,
            clock,
            extra_accounts,
            TokenTypes::TokenB,
        )
        .map_err(|e| {
            warn!("Error getting KToken share ratio: {:?}", e);
            e.into()
        }),
        #[cfg(not(feature = "yvaults"))]
        OracleType::KTokenToTokenA => {
            panic!("yvaults feature is not enabled, KToken oracle type is not available")
        }
        #[cfg(not(feature = "yvaults"))]
        OracleType::KTokenToTokenB => {
            panic!("yvaults feature is not enabled, KToken oracle type is not available")
        }
        OracleType::MsolStake => msol_stake::get_price(base_account, clock).map_err(Into::into),
        OracleType::JupiterLpFetch => {
            jupiter_lp::get_price_no_recompute(base_account, clock, extra_accounts).map_err(|e| {
                warn!("Error getting Jupiter LP price: {:?}", e);
                e
            })
        }
        OracleType::ScopeTwap => twap::get_price(oracle_mappings, oracle_twaps, index, clock)
            .map_err(|e| {
                warn!("Error getting Scope TWAP price: {:?}", e);
                e.into()
            }),
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
        OracleType::FixedPrice => {
            let mut price_data: &[u8] = &oracle_mappings.generic[index];
            let price = AnchorDeserialize::deserialize(&mut price_data).unwrap();
            Ok(DatedPrice {
                price,
                last_updated_slot: clock.slot,
                unix_timestamp: clock.unix_timestamp.try_into().unwrap(),
                ..Default::default()
            })
        }
        OracleType::JitoRestaking => {
            jito_restaking::get_price(base_account, clock).map_err(Into::into)
        }
        OracleType::Chainlink => {
            msg!("Chainlink oracle type cannot be refreshed directly");
            return err!(ScopeError::PriceNotValid);
        }
        OracleType::DiscountToMaturity => {
            discount_to_maturity::get_price(&oracle_mappings.generic[index], clock)
        }
        OracleType::MostRecentOf => most_recent_of::get_price(
            oracle_prices.load()?.deref(),
            &oracle_mappings.generic[index],
            clock,
        )
        .map_err(|e| e.into()),
        OracleType::PythLazer => {
            msg!("PythLazer oracle type cannot be refreshed directly");
            return err!(ScopeError::PriceNotValid);
        }
        OracleType::DeprecatedPlaceholder1 | OracleType::DeprecatedPlaceholder2 => {
            panic!("DeprecatedPlaceholder is not a valid oracle type")
        }
    }?;
    // The price providers above are performing their type-specific validations, but are still free
    // to return 0, which we can only tolerate in case of explicit fixed price:
    if price.price.value == 0 && price_type != OracleType::FixedPrice {
        warn!("Price is 0 (token {index}, type {price_type:?}): {price:?}",);
        return err!(ScopeError::PriceNotValid);
    }
    Ok(price)
}

/// Validate the given account as being an appropriate price account for the
/// given oracle type.
///
/// This function shall be called before update of oracle mappings
pub fn validate_oracle_cfg(
    price_type: OracleType,
    price_account: &Option<AccountInfo>,
    twap_source: u16,
    generic_data: &[u8; 20],
    clock: &Clock,
) -> crate::Result<()> {
    // when we remove something from the config there is no validation needed
    if price_type == OracleType::Pyth && price_account.is_none() {
        return Ok(());
    }

    match price_type {
        OracleType::Pyth => pyth::validate_pyth_price_info(price_account),
        OracleType::PythPull => pyth_pull::validate_price_update_v2_info(price_account),
        OracleType::PythPullEMA => pyth_pull::validate_price_update_v2_info(price_account),
        OracleType::SwitchboardOnDemand => {
            switchboard_on_demand::validate_price_account(price_account)
        }
        OracleType::SwitchboardV2 => Ok(()), // TODO at least check account ownership?
        OracleType::CToken => Ok(()),        // TODO how shall we validate ctoken account?
        OracleType::SplStake => Ok(()),
        OracleType::KToken => Ok(()), // TODO, should validate ownership of the ktoken account
        OracleType::KTokenToTokenA => Ok(()), // TODO, should validate ownership of the ktoken account
        OracleType::KTokenToTokenB => Ok(()), // TODO, should validate ownership of the ktoken account
        OracleType::PythEMA => pyth::validate_pyth_price_info(price_account),
        OracleType::MsolStake => Ok(()),
        OracleType::JupiterLpFetch | OracleType::JupiterLpCompute | OracleType::JupiterLpScope => {
            jupiter_lp::validate_jlp_pool(price_account)
        }
        OracleType::ScopeTwap => twap::validate_price_account(price_account, twap_source),
        OracleType::OrcaWhirlpoolAtoB | OracleType::OrcaWhirlpoolBtoA => {
            orca_whirlpool::validate_pool_account(price_account)
        }
        OracleType::RaydiumAmmV3AtoB | OracleType::RaydiumAmmV3BtoA => {
            raydium_ammv3::validate_pool_account(price_account)
        }
        OracleType::MeteoraDlmmAtoB | OracleType::MeteoraDlmmBtoA => {
            meteora_dlmm::validate_pool_account(price_account)
        }
        OracleType::FixedPrice => {
            if price_account.is_some() {
                warn!("No account is expected with a fixed price oracle");
                return err!(ScopeError::PriceNotValid);
            }
            let mut price_data: &[u8] = generic_data;
            let _price: Price = AnchorDeserialize::deserialize(&mut price_data)
                .map_err(|_| error!(ScopeError::FixedPriceInvalid))?;
            Ok(())
        }
        OracleType::JitoRestaking => jito_restaking::validate_account(price_account),
        OracleType::Chainlink => {
            chainlink::validate_mapping(price_account, generic_data).map_err(Into::into)
        }
        OracleType::DiscountToMaturity => {
            discount_to_maturity::validate_mapping_cfg(price_account, generic_data, clock)
                .map_err(Into::into)
        }
        OracleType::MostRecentOf => {
            most_recent_of::validate_mapping_cfg(price_account, generic_data).map_err(Into::into)
        }
        OracleType::PythLazer => {
            pyth_lazer::validate_mapping_cfg(price_account, generic_data).map_err(Into::into)
        }
        OracleType::DeprecatedPlaceholder1 | OracleType::DeprecatedPlaceholder2 => {
            panic!("DeprecatedPlaceholder is not a valid oracle type")
        }
    }
}
