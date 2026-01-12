#[cfg(feature = "yvaults")]
pub mod ktokens;
#[cfg(feature = "yvaults")]
pub mod ktokens_token_x;

pub mod adrena_lp;
pub mod capped_floored;
pub mod capped_most_recent_of;
pub mod chainlink;
pub mod discount_to_maturity;
pub mod fixed_price;
pub mod flashtrade_lp;
pub mod jito_restaking;
pub mod jupiter_lp;
pub mod meteora_dlmm;
pub mod most_recent_of;
pub mod msol_stake;
pub mod orca_whirlpool;
pub mod pyth;
pub mod pyth_lazer;
pub mod pyth_pull;
pub mod pyth_pull_ema;
pub mod raydium_ammv3;
pub mod redstone;
pub mod securitize;
pub mod spl_stake;
pub mod switchboard_on_demand;
pub mod twap;

use std::{
    fmt::{Debug, DebugStruct},
    ops::Deref,
};

use anchor_lang::{accounts::account_loader::AccountLoader, prelude::*};
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "yvaults")]
use self::ktokens_token_x::TokenTypes;
use crate::{
    states::{DatedPrice, EmaType, OracleMappings, OraclePrices, OracleTwaps},
    warn, ScopeError, ScopeResult,
};

pub fn check_context<T>(ctx: &Context<T>) -> Result<()> {
    //make sure there are no extra accounts
    if !ctx.remaining_accounts.is_empty() {
        return err!(ScopeError::UnexpectedAccount);
    }

    Ok(())
}

#[derive(
    Default,
    AnchorSerialize,
    AnchorDeserialize,
    IntoPrimitive,
    TryFromPrimitive,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum OracleType {
    /// Deprecated (formerly Pyth)
    // Do not remove - breaks the typescript idl codegen
    #[default]
    Unused = 0,
    /// Deprecated (formerly SwitchboardV1)
    // Do not remove - breaks the typescript idl codegen
    DeprecatedPlaceholder1 = 1,
    /// Deprecated (formerly SwitchboardV2)
    // Do not remove - breaks the typescript idl codegen
    DeprecatedPlaceholder2 = 2,
    /// Deprecated (formerly YiToken)
    // Do not remove - breaks the typescript idl codegen
    DeprecatedPlaceholder3 = 3,
    /// Deprecated (formerly CToken/Solend tokens)
    // Do not remove - breaks the typescript idl codegen
    DeprecatedPlaceholder4 = 4,
    /// SPL Stake Pool token (giving the stake rate in SOL):
    /// This oracle type provide a reference and is not meant to be used directly
    /// to get the value of the token because of different limitations:
    /// - The stake rate is only updated once per epoch and can be delayed by one hour after a new epoch.
    /// - The stake rate does not take into account the fees that applies on staking or unstaking.
    /// - Unstaking is not immediate and the market price is often lower than the "stake price".
    SplStake = 5,
    /// KTokens from Kamino
    KToken = 6,
    /// Deprecated (formerly PythEMA)
    // Do not remove - breaks the typescript idl codegen
    DeprecatedPlaceholder5 = 7,
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
    /// Scope twap of 1h (also see [`ScopeTwap8h`] and [`ScopeTwap24h`] below)
    ScopeTwap1h = 12,
    /// Orca's whirlpool price (CLMM) A to B
    OrcaWhirlpoolAtoB = 13,
    /// Orca's whirlpool price (CLMM) B to A
    OrcaWhirlpoolBtoA = 14,
    /// Raydium's AMM v3 price (CLMM) A to B
    RaydiumAmmV3AtoB = 15,
    /// Raydium's AMM v3 price (CLMM) B to A
    RaydiumAmmV3BtoA = 16,
    /// Deprecated (formerly JupiterLpCompute)
    // Do not remove - breaks the typescript idl codegen
    DeprecatedPlaceholder6 = 17,
    /// Meteora's DLMM A to B
    MeteoraDlmmAtoB = 18,
    /// Meteora's DLMM B to A
    MeteoraDlmmBtoA = 19,
    /// Deprecated (formerly JupiterLpScope)
    // Do not remove - breaks the typescript idl codegen
    DeprecatedPlaceholder7 = 20,
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
    /// RedStone price oracle
    RedStone = 30,
    /// Adrena's perpetual LP token price
    AdrenaLp = 31,
    /// Securitize sacred price oracle
    Securitize = 32,
    /// Keeps track of a source price, capping and/or flooring to given source prices
    CappedFloored = 33,
    /// Chainlink xStocks oracle
    ChainlinkRWA = 34,
    /// Chainlink NAV oracle
    ChainlinkNAV = 35,
    /// Flashtrade's perpetual LP token price
    ///
    /// When using this price and setting a max age, care has to be taken that the max age
    /// of the source oracles is not the price's timestamp, and thus `max_age` needs
    /// to be set 10s lower than the intended target on the usage side to ensure the same liveliness.
    FlashtradeLp = 36,
    /// Chainlink xStocks oracle
    ChainlinkX = 37,
    /// Chainlink exchange rate oracle
    ChainlinkExchangeRate = 38,
    /// Keeps track of multiple source prices making sure they are recent enough and they don't
    /// diverge more than a specified limit, while also applying a cap price
    CappedMostRecentOf = 39,
    ScopeTwap8h = 40,
    ScopeTwap24h = 41,
}

impl OracleType {
    pub fn is_twap(self) -> bool {
        matches!(
            self,
            OracleType::ScopeTwap1h | OracleType::ScopeTwap8h | OracleType::ScopeTwap24h
        )
    }

    pub fn to_ema_type(&self) -> ScopeResult<EmaType> {
        match self {
            OracleType::ScopeTwap1h => Ok(EmaType::Ema1h),
            OracleType::ScopeTwap8h => Ok(EmaType::Ema8h),
            OracleType::ScopeTwap24h => Ok(EmaType::Ema24h),
            _ => Err(ScopeError::InvalidConversionToEmaTypeForOracleType),
        }
    }

    pub fn is_chainlink_provider(self) -> bool {
        match self {
            OracleType::Chainlink
            | OracleType::ChainlinkRWA
            | OracleType::ChainlinkNAV
            | OracleType::ChainlinkX
            | OracleType::ChainlinkExchangeRate => true,

            OracleType::Unused
            | OracleType::DeprecatedPlaceholder1
            | OracleType::DeprecatedPlaceholder2
            | OracleType::DeprecatedPlaceholder3
            | OracleType::DeprecatedPlaceholder4
            | OracleType::DeprecatedPlaceholder5
            | OracleType::DeprecatedPlaceholder6
            | OracleType::DeprecatedPlaceholder7
            | OracleType::SplStake
            | OracleType::KToken
            | OracleType::MsolStake
            | OracleType::KTokenToTokenA
            | OracleType::KTokenToTokenB
            | OracleType::JupiterLpFetch
            | OracleType::ScopeTwap1h
            | OracleType::ScopeTwap8h
            | OracleType::ScopeTwap24h
            | OracleType::OrcaWhirlpoolAtoB
            | OracleType::OrcaWhirlpoolBtoA
            | OracleType::RaydiumAmmV3AtoB
            | OracleType::RaydiumAmmV3BtoA
            | OracleType::MeteoraDlmmAtoB
            | OracleType::MeteoraDlmmBtoA
            | OracleType::PythPull
            | OracleType::PythPullEMA
            | OracleType::FixedPrice
            | OracleType::SwitchboardOnDemand
            | OracleType::JitoRestaking
            | OracleType::DiscountToMaturity
            | OracleType::MostRecentOf
            | OracleType::PythLazer
            | OracleType::RedStone
            | OracleType::AdrenaLp
            | OracleType::Securitize
            | OracleType::CappedFloored
            | OracleType::FlashtradeLp
            | OracleType::CappedMostRecentOf => false,
        }
    }

    /// Get the number of compute unit needed to refresh the price of a token
    pub fn get_update_cu_budget(&self) -> u32 {
        match self {
            OracleType::FixedPrice => 10_000,
            OracleType::PythPull => 20_000,
            OracleType::PythPullEMA => 20_000,
            OracleType::SwitchboardOnDemand => 30_000,
            OracleType::SplStake => 20_000,
            OracleType::KToken => 120_000,
            OracleType::KTokenToTokenA | OracleType::KTokenToTokenB => 100_000,
            OracleType::MsolStake => 20_000,
            OracleType::JupiterLpFetch => 40_000,
            OracleType::ScopeTwap1h | OracleType::ScopeTwap8h | OracleType::ScopeTwap24h => 30_000,
            OracleType::OrcaWhirlpoolAtoB
            | OracleType::OrcaWhirlpoolBtoA
            | OracleType::RaydiumAmmV3AtoB
            | OracleType::RaydiumAmmV3BtoA => 25_000,
            OracleType::MeteoraDlmmAtoB | OracleType::MeteoraDlmmBtoA => 30_000,
            OracleType::JitoRestaking => 25_000,
            OracleType::DiscountToMaturity => 30_000,
            // Chainlink oracles are not updated through normal refresh ixs
            OracleType::Chainlink
            | OracleType::ChainlinkRWA
            | OracleType::ChainlinkNAV
            | OracleType::ChainlinkX
            | OracleType::ChainlinkExchangeRate => 0,
            OracleType::MostRecentOf => 35_000,
            OracleType::CappedMostRecentOf => 40_000,
            OracleType::RedStone => 20_000,
            // PythLazer oracle is not updated through normal refresh ixs
            OracleType::PythLazer => 0,
            OracleType::CappedFloored => 20_000,
            OracleType::Unused
            | OracleType::DeprecatedPlaceholder1
            | OracleType::DeprecatedPlaceholder2
            | OracleType::DeprecatedPlaceholder3
            | OracleType::DeprecatedPlaceholder4
            | OracleType::DeprecatedPlaceholder5
            | OracleType::DeprecatedPlaceholder6
            | OracleType::DeprecatedPlaceholder7 => {
                panic!("DeprecatedPlaceholder is not a valid oracle type")
            }
            OracleType::Securitize => 30_000,
            OracleType::AdrenaLp => 20_000,
            OracleType::FlashtradeLp => 20_000,
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
        OracleType::PythPull => pyth_pull::get_price(base_account, clock),
        OracleType::PythPullEMA => pyth_pull_ema::get_price(base_account, clock),
        OracleType::SwitchboardOnDemand => {
            switchboard_on_demand::get_price(base_account, clock).map_err(Into::into)
        }
        OracleType::SplStake => spl_stake::get_price(base_account, clock),
        #[cfg(not(feature = "yvaults"))]
        OracleType::KToken => {
            panic!("yvaults feature is not enabled, KToken oracle type is not available")
        }
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
        OracleType::ScopeTwap1h | OracleType::ScopeTwap8h | OracleType::ScopeTwap24h => {
            twap::get_price(
                oracle_mappings,
                oracle_twaps,
                index,
                price_type.to_ema_type()?,
                clock,
            )
            .map_err(|e| {
                warn!("Error getting Scope TWAP price: {:?}", e);
                e.into()
            })
        }
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
        OracleType::FixedPrice => {
            let price = fixed_price::parse_generic_data(&oracle_mappings.generic[index])?;
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
        OracleType::Chainlink
        | OracleType::ChainlinkRWA
        | OracleType::ChainlinkNAV
        | OracleType::ChainlinkX
        | OracleType::ChainlinkExchangeRate => {
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
        .map_err(Into::into),
        OracleType::RedStone => {
            let oracle_prices = oracle_prices.load()?;
            let dated_price = oracle_prices.prices[index];
            redstone::get_price(base_account, &dated_price, clock).map_err(Into::into)
        }
        OracleType::PythLazer => {
            msg!("PythLazer oracle type cannot be refreshed directly");
            return err!(ScopeError::PriceNotValid);
        }
        OracleType::CappedFloored => capped_floored::get_price(
            oracle_prices.load()?.deref(),
            &oracle_mappings.generic[index],
        )
        .map_err(Into::into),
        OracleType::CappedMostRecentOf => capped_most_recent_of::get_price(
            oracle_prices.load()?.deref(),
            &oracle_mappings.generic[index],
            clock,
        )
        .map_err(Into::into),
        OracleType::Securitize => {
            let oracle_prices = oracle_prices.load()?;
            let dated_price = oracle_prices.prices[index];
            securitize::get_sacred_price(base_account, &dated_price, clock, extra_accounts)
                .map_err(Into::into)
        }
        OracleType::Unused
        | OracleType::DeprecatedPlaceholder1
        | OracleType::DeprecatedPlaceholder2
        | OracleType::DeprecatedPlaceholder3
        | OracleType::DeprecatedPlaceholder4
        | OracleType::DeprecatedPlaceholder5
        | OracleType::DeprecatedPlaceholder6
        | OracleType::DeprecatedPlaceholder7 => {
            panic!("DeprecatedPlaceholder is not a valid oracle type")
        }
        OracleType::AdrenaLp => adrena_lp::get_price(base_account, clock),
        OracleType::FlashtradeLp => flashtrade_lp::get_price(base_account, clock),
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
    price_account: Option<&AccountInfo>,
    generic_data: &[u8; 20],
    clock: &Clock,
) -> crate::Result<()> {
    // we use the default price (formerly Pyth) to indicate removal of a price
    // when we remove something from the config there is no validation needed
    if price_type == OracleType::default() && price_account.is_none() {
        return Ok(());
    }

    match price_type {
        OracleType::PythPull => pyth_pull::validate_price_update_v2_info(price_account),
        OracleType::PythPullEMA => pyth_pull::validate_price_update_v2_info(price_account),
        OracleType::SwitchboardOnDemand => {
            switchboard_on_demand::validate_price_account(price_account)
        }
        OracleType::SplStake => Ok(()),
        OracleType::KToken => Ok(()), // TODO, should validate ownership of the ktoken account
        OracleType::KTokenToTokenA => Ok(()), // TODO, should validate ownership of the ktoken account
        OracleType::KTokenToTokenB => Ok(()), // TODO, should validate ownership of the ktoken account
        OracleType::MsolStake => Ok(()),
        OracleType::JupiterLpFetch => jupiter_lp::validate_jlp_pool(price_account),
        OracleType::ScopeTwap1h | OracleType::ScopeTwap8h | OracleType::ScopeTwap24h => {
            panic!("ScopeTwap validation uses a different path")
        }
        OracleType::OrcaWhirlpoolAtoB | OracleType::OrcaWhirlpoolBtoA => {
            orca_whirlpool::validate_pool_account(price_account)
        }
        OracleType::RaydiumAmmV3AtoB | OracleType::RaydiumAmmV3BtoA => {
            raydium_ammv3::validate_pool_account(price_account)
        }
        OracleType::MeteoraDlmmAtoB | OracleType::MeteoraDlmmBtoA => {
            meteora_dlmm::validate_pool_account(price_account)
        }
        OracleType::FixedPrice => fixed_price::validate_mapping(price_account, generic_data),
        OracleType::JitoRestaking => jito_restaking::validate_account(price_account),
        OracleType::Chainlink => {
            chainlink::validate_mapping_v3(price_account, generic_data).map_err(Into::into)
        }
        OracleType::ChainlinkRWA => {
            chainlink::validate_mapping_v8_v10(price_account, generic_data).map_err(Into::into)
        }
        OracleType::ChainlinkNAV => {
            chainlink::validate_mapping_v7_v9(price_account).map_err(Into::into)
        }
        OracleType::ChainlinkX => {
            chainlink::validate_mapping_v8_v10(price_account, generic_data).map_err(Into::into)
        }
        OracleType::ChainlinkExchangeRate => {
            chainlink::validate_mapping_v7_v9(price_account).map_err(Into::into)
        }
        OracleType::DiscountToMaturity => {
            discount_to_maturity::validate_mapping_cfg(price_account, generic_data, clock)
                .map_err(Into::into)
        }
        OracleType::MostRecentOf => {
            most_recent_of::validate_mapping_cfg(price_account, generic_data).map_err(Into::into)
        }
        OracleType::RedStone => redstone::validate_price_account(price_account).map_err(Into::into),
        OracleType::PythLazer => {
            pyth_lazer::validate_mapping_cfg(price_account, generic_data).map_err(Into::into)
        }
        OracleType::CappedFloored => {
            capped_floored::validate_mapping_cfg(price_account, generic_data).map_err(Into::into)
        }
        OracleType::CappedMostRecentOf => {
            capped_most_recent_of::validate_mapping_cfg(price_account, generic_data)
                .map_err(Into::into)
        }
        OracleType::Securitize => Ok(()),
        OracleType::Unused
        | OracleType::DeprecatedPlaceholder1
        | OracleType::DeprecatedPlaceholder2
        | OracleType::DeprecatedPlaceholder3
        | OracleType::DeprecatedPlaceholder4
        | OracleType::DeprecatedPlaceholder5
        | OracleType::DeprecatedPlaceholder6
        | OracleType::DeprecatedPlaceholder7 => {
            panic!("DeprecatedPlaceholder is not a valid oracle type")
        }
        OracleType::AdrenaLp => adrena_lp::validate_adrena_pool(price_account, clock),
        OracleType::FlashtradeLp => flashtrade_lp::validate_flashtrade_pool(price_account, clock),
    }
}

pub fn update_generic_data_must_reset_price(price_type: OracleType) -> bool {
    match price_type {
        OracleType::SplStake
        | OracleType::KToken
        | OracleType::MsolStake
        | OracleType::KTokenToTokenA
        | OracleType::KTokenToTokenB
        | OracleType::JupiterLpFetch
        | OracleType::ScopeTwap1h
        | OracleType::ScopeTwap8h
        | OracleType::ScopeTwap24h
        | OracleType::OrcaWhirlpoolAtoB
        | OracleType::OrcaWhirlpoolBtoA
        | OracleType::RaydiumAmmV3AtoB
        | OracleType::RaydiumAmmV3BtoA
        | OracleType::MeteoraDlmmAtoB
        | OracleType::MeteoraDlmmBtoA
        | OracleType::PythPull
        | OracleType::PythPullEMA
        | OracleType::SwitchboardOnDemand
        | OracleType::JitoRestaking
        | OracleType::RedStone
        | OracleType::AdrenaLp
        | OracleType::Securitize
        | OracleType::ChainlinkNAV
        | OracleType::FlashtradeLp
        | OracleType::ChainlinkExchangeRate => false,

        OracleType::FixedPrice
        | OracleType::DiscountToMaturity
        | OracleType::MostRecentOf
        | OracleType::CappedFloored
        | OracleType::CappedMostRecentOf
        | OracleType::Chainlink
        | OracleType::ChainlinkRWA
        | OracleType::ChainlinkX
        | OracleType::PythLazer => true,

        OracleType::Unused
        | OracleType::DeprecatedPlaceholder1
        | OracleType::DeprecatedPlaceholder2
        | OracleType::DeprecatedPlaceholder3
        | OracleType::DeprecatedPlaceholder4
        | OracleType::DeprecatedPlaceholder5
        | OracleType::DeprecatedPlaceholder6
        | OracleType::DeprecatedPlaceholder7 => unreachable!(),
    }
}

pub fn debug_format_generic_data(
    d: &mut DebugStruct<'_, '_>,
    price_type: OracleType,
    generic_data: &[u8; 20],
) {
    match price_type {
        OracleType::PythPull
        | OracleType::PythPullEMA
        | OracleType::SplStake
        | OracleType::KToken
        | OracleType::MsolStake
        | OracleType::KTokenToTokenA
        | OracleType::KTokenToTokenB
        | OracleType::JupiterLpFetch
        | OracleType::OrcaWhirlpoolAtoB
        | OracleType::OrcaWhirlpoolBtoA
        | OracleType::RaydiumAmmV3AtoB
        | OracleType::RaydiumAmmV3BtoA
        | OracleType::MeteoraDlmmAtoB
        | OracleType::MeteoraDlmmBtoA
        | OracleType::SwitchboardOnDemand
        | OracleType::JitoRestaking
        | OracleType::RedStone
        | OracleType::Securitize
        | OracleType::AdrenaLp
        | OracleType::FlashtradeLp
        | OracleType::ScopeTwap1h
        | OracleType::ScopeTwap8h
        | OracleType::ScopeTwap24h
        | OracleType::ChainlinkNAV
        | OracleType::ChainlinkExchangeRate
        | OracleType::Unused
        | OracleType::DeprecatedPlaceholder1
        | OracleType::DeprecatedPlaceholder2
        | OracleType::DeprecatedPlaceholder3
        | OracleType::DeprecatedPlaceholder4
        | OracleType::DeprecatedPlaceholder5
        | OracleType::DeprecatedPlaceholder6
        | OracleType::DeprecatedPlaceholder7 => (), // no generic data to print

        OracleType::Chainlink => {
            d.field(
                "chainlink_v3_cfg",
                &chainlink::cfg_data::V3::from_generic_data(generic_data).ok(),
            );
        }
        OracleType::ChainlinkRWA | OracleType::ChainlinkX => {
            d.field(
                "chainlink_v8_v10_cfg",
                &chainlink::cfg_data::V8V10::from_generic_data(generic_data).ok(),
            );
        }

        OracleType::FixedPrice => {
            d.field(
                "fixed_price",
                &fixed_price::parse_generic_data(generic_data),
            );
        }
        OracleType::DiscountToMaturity => {
            d.field(
                "discount_to_maturity_cfg",
                &discount_to_maturity::DiscountToMaturityData::from_generic_data(generic_data).ok(),
            );
        }
        OracleType::MostRecentOf => {
            d.field(
                "most_recent_of_cfg",
                &most_recent_of::MostRecentOfData::from_generic_data(generic_data).ok(),
            );
        }
        OracleType::PythLazer => {
            d.field(
                "pyth_lazer_cfg",
                &pyth_lazer::PythLazerData::from_generic_data(generic_data).ok(),
            );
        }
        OracleType::CappedFloored => {
            d.field(
                "capped_floored_cfg",
                &capped_floored::CappedFlooredData::from_generic_data(generic_data).ok(),
            );
        }
        OracleType::CappedMostRecentOf => {
            d.field(
                "capped_most_recent_of_cfg",
                &capped_most_recent_of::CappedMostRecentOfData::from_generic_data(generic_data)
                    .ok(),
            );
        }
    }
}
