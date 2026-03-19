use anchor_lang::prelude::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::oracle_twaps::EmaType;
use crate::errors::{ScopeError, ScopeResult};

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
    ScopeTwap7d = 42,
}

impl OracleType {
    pub fn is_twap(self) -> bool {
        matches!(
            self,
            OracleType::ScopeTwap1h
                | OracleType::ScopeTwap8h
                | OracleType::ScopeTwap24h
                | OracleType::ScopeTwap7d
        )
    }

    pub fn to_ema_type(&self) -> ScopeResult<EmaType> {
        match self {
            OracleType::ScopeTwap1h => Ok(EmaType::Ema1h),
            OracleType::ScopeTwap8h => Ok(EmaType::Ema8h),
            OracleType::ScopeTwap24h => Ok(EmaType::Ema24h),
            OracleType::ScopeTwap7d => Ok(EmaType::Ema7d),
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
            | OracleType::ScopeTwap7d
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
}
