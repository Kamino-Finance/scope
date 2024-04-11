//! Utils to save onchain a price chain to reuse in scope
//!
//! Scope codebase is not aware of the token configuration. This module defines how to store onchain
//! a price chain that allows to compute a price in a different quotation indexed with a foreign token id.
//!
//! This tools used for ktokens for now but can be reused in the future for other price based on others available in scope.
//!
//! An account can store up to `crate::MAX_ENTRIES` chains.
//! One chain is composed of at most 4 prices.
//!
//! ## Example
//!
//! ### Scenario
//!
//! Given a scope configuration with the following prices:
//!
//! 0. USDH/USD
//! 1. SOL/USDH
//! 2. mSOL/SOL
//!
//! The program using this configuration has two tokens identified by their respective index:
//! 0. SOL
//! 1. mSOL
//!
//! The program needs the prices in USD.
//!
//! ### Simple code example
//!
//! The scope chain can be declared like this:
//!
//! ```ignore
//! use scope::utils::scope_chain::ScopeChainAccount;
//!
//! let raw_chain: &[&[u16]] = &[
//!     // SOL/USD
//!     &[1_u16, 0],
//!     // mSOL/USD
//!     &[2, 1, 0],
//! ];
//! let chain = ScopeChainAccount::new(raw_chain).unwrap();
//! ```
//! ### Advanced code example
//!
//! ```ignore
//! use scope::utils::scope_chain::{PriceChain, ScopeChainAccount, ScopeChainError};
//! use strum::EnumIter;
//!
//! #[derive(EnumIter)]
//! enum CollateralToken {
//!     SOL,
//!     MSOL,
//! }
//!
//! #[repr(u16)]
//! #[allow(non_camel_case_types)]
//! #[derive(Copy, Clone)]
//! enum ScopeId {
//!     USDH,
//!     SOL_USDH,
//!     MSOL_SOL,
//! }
//!
//! impl From<ScopeId> for u16 {
//!     fn from(v: ScopeId) -> u16 {
//!         v as u16
//!     }
//! }
//!
//! impl TryFrom<CollateralToken> for PriceChain<ScopeId> {
//!     type Error = ScopeChainError;
//!     fn try_from(t: CollateralToken) -> Result<PriceChain<ScopeId>, ScopeChainError> {
//!         let chain_base: &[ScopeId] = match t {
//!             SOL => &[ScopeId::SOL_USDH, ScopeId::USDH],
//!             MSOL => &[ScopeId::MSOL_SOL, ScopeId::SOL_USDH, ScopeId::USDH],
//!         };
//!         chain_base.try_into()
//!     }
//! }
//!
//! let chain = ScopeChainAccount::auto_chain::<CollateralToken, ScopeId>().unwrap();
//! ```

use std::fmt::Debug;

use anchor_lang::Discriminator;
use bytemuck;
use decimal_wad::rate::U128;
pub use strum::IntoEnumIterator;

use crate::{DatedPrice, OraclePrices, Price, ScopeError, MAX_ENTRIES};

/// Maximum length of a chain (4 so the size of one chain is the same as `u64`)
pub const MAX_CHAIN_LENGTH: usize = 4;

type RawChain = [u16; MAX_CHAIN_LENGTH];

#[derive(Clone, Debug)]
pub struct PriceChain<T>([Option<T>; MAX_CHAIN_LENGTH])
where
    T: Into<u16>;

impl<T> TryFrom<&[T]> for PriceChain<T>
where
    T: Into<u16> + Clone + Copy,
{
    type Error = ScopeChainError;

    fn try_from(arr: &[T]) -> Result<Self, Self::Error> {
        if arr.len() > MAX_CHAIN_LENGTH {
            return Err(ScopeChainError::PriceChainTooLong);
        }
        let mut res = [None; MAX_CHAIN_LENGTH];
        for (input, output) in arr.iter().zip(res.iter_mut()) {
            *output = Some(*input);
        }
        Ok(Self(res))
    }
}

impl<T> From<&PriceChain<T>> for RawChain
where
    T: Into<u16> + Copy,
{
    fn from(chain: &PriceChain<T>) -> Self {
        let mut res = RawChain::default();
        for (u16_id, t_id) in res.iter_mut().zip(chain.0.iter()) {
            *u16_id = match t_id {
                Some(v) => (*v).into(),
                None => MAX_ENTRIES as u16,
            }
        }
        res
    }
}

impl<T> From<PriceChain<T>> for RawChain
where
    T: Into<u16>,
{
    fn from(chain: PriceChain<T>) -> Self {
        chain.0.map(|v| match v {
            Some(v) => v.into(),
            None => MAX_ENTRIES as u16,
        })
    }
}

pub struct RawChainWrap(RawChain);

impl<T> TryFrom<&[T]> for RawChainWrap
where
    T: Into<u16> + Clone + Copy,
{
    type Error = ScopeChainError;

    fn try_from(arr: &[T]) -> Result<Self, Self::Error> {
        let scope_chain: PriceChain<T> = arr.try_into()?;
        let raw_chain: RawChain = scope_chain.into();
        Ok(Self(raw_chain))
    }
}

#[derive(PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
#[repr(C)]
pub struct ScopeChainAccount {
    // Its an array of `RawChain` but anchor does not support type alias when generating IDL
    chain_array: [[u16; MAX_CHAIN_LENGTH]; MAX_ENTRIES],
}

impl Discriminator for ScopeChainAccount {
    const DISCRIMINATOR: [u8; 8] = [180, 51, 138, 247, 240, 173, 119, 79];

    fn discriminator() -> [u8; 8] {
        Self::DISCRIMINATOR
    }
}

impl ScopeChainAccount {
    pub fn auto_chain_update<Token, ScopeId>(&mut self) -> std::result::Result<(), ScopeChainError>
    where
        Token: TryInto<PriceChain<ScopeId>> + IntoEnumIterator,
        ScopeId: Into<u16> + Copy + Clone,
    {
        let mut chain_iter_mut = self.chain_array.iter_mut();
        for (token, chain) in Token::iter().zip(chain_iter_mut.by_ref()) {
            let scope_chain: PriceChain<ScopeId> = token
                .try_into()
                .map_err(|_| ScopeChainError::PriceChainConversionFailure)?;
            *chain = scope_chain.into();
        }
        // Set all remaining to default value
        for dst in chain_iter_mut.flatten() {
            *dst = MAX_ENTRIES as u16;
        }
        Ok(())
    }

    pub fn update_entry(
        &mut self,
        price_id: usize,
        price_chain: impl TryInto<RawChainWrap>,
    ) -> Result<(), ScopeChainError> {
        let chain: RawChainWrap = price_chain
            .try_into()
            .map_err(|_| ScopeChainError::PriceChainConversionFailure)?;
        self.chain_array[price_id] = chain.0;
        Ok(())
    }

    pub fn update<T>(&mut self, base_chain_array: &[T]) -> Result<(), ScopeChainError>
    where
        T: TryInto<RawChainWrap> + Copy,
    {
        let mut chain_iter_mut = self.chain_array.iter_mut();
        for (price_chain, dst) in base_chain_array.iter().zip(chain_iter_mut.by_ref()) {
            let chain: RawChainWrap = (*price_chain)
                .try_into()
                .map_err(|_| ScopeChainError::PriceChainConversionFailure)?;
            *dst = chain.0;
        }
        // Set all remaining to default value
        for dst in chain_iter_mut.flatten() {
            *dst = MAX_ENTRIES as u16;
        }
        Ok(())
    }

    pub fn get_price(
        &self,
        prices: &OraclePrices,
        token_id: usize,
    ) -> Result<DatedPrice, ScopeChainError> {
        let chain = self
            .chain_array
            .get(token_id)
            .ok_or(ScopeChainError::NoChainForToken)?;
        get_price_from_chain(prices, chain)
    }
}

// TODO not working with latest prices that have a lot of decimals. Backport yvault version here.
pub fn get_price_from_chain(
    prices: &OraclePrices,
    chain: &[u16; MAX_CHAIN_LENGTH],
) -> Result<DatedPrice, ScopeChainError> {
    let price_chain = chain.map(usize::from).map(|id| prices.prices.get(id));

    let last_updated_slot = price_chain
        .iter()
        .filter_map(|&opt| opt.map(|price| price.last_updated_slot))
        .reduce(|acc, val| acc.min(val))
        .ok_or(ScopeChainError::NoChainForToken)?;

    let unix_timestamp = price_chain
        .iter()
        .filter_map(|&opt| opt.map(|price| price.unix_timestamp))
        .reduce(|acc, val| acc.min(val))
        .ok_or(ScopeChainError::NoChainForToken)?;

    let total_decimals: u64 = price_chain
        .iter()
        .filter_map(|&opt| opt.map(|price| price.price.exp))
        .try_fold(0u64, |acc, exp| acc.checked_add(exp))
        .ok_or(ScopeChainError::MathOverflow)?;

    // Final number of decimals is the last element one's which should be the quotation price.
    let exp = price_chain
        .iter()
        .filter_map(|&opt| opt.map(|price| price.price.exp))
        .last()
        .unwrap(); // chain is never empty here by construction

    // Compute token value by multiplying all value of the chain
    let product = price_chain
        .iter()
        .filter_map(|&opt| opt.map(|price| price.price.value))
        .try_fold(U128::from(1u128), |acc, value| {
            acc.checked_mul(value.into())
        })
        .ok_or(ScopeChainError::MathOverflow)?;

    // Compute final value by removing extra decimals
    let scale_down_decimals: u32 = total_decimals.checked_sub(exp).unwrap().try_into().unwrap(); // Cannot fail by construction of `total_decimals`
    let scale_down_factor = U128::from(10u128)
        .checked_pow(U128::from(scale_down_decimals))
        .unwrap();
    let value: u64 = product
        .checked_div(scale_down_factor)
        .unwrap() // Cannot fail thanks to the early return
        .try_into()
        .map_err(|_| ScopeChainError::IntegerConversionOverflow)?;

    Ok(DatedPrice {
        last_updated_slot,
        unix_timestamp,
        price: Price { value, exp },
        ..Default::default()
    })
}

/// Errors that can be raised while creating or manipulating a scope chain
#[derive(Debug)]
pub enum ScopeChainError {
    /// Too many prices in a chain, cannot be stored
    PriceChainTooLong,
    /// Conversion to a price chain failed
    PriceChainConversionFailure,
    /// The token has not a valid chain associated
    NoChainForToken,
    /// No valid price computed from the provided chain and prices
    InvalidPricesInChain,
    MathOverflow,
    IntegerConversionOverflow,
}

impl From<ScopeChainError> for ScopeError {
    fn from(chain_error: ScopeChainError) -> Self {
        match chain_error {
            ScopeChainError::PriceChainTooLong => ScopeError::BadScopeChainOrPrices,
            ScopeChainError::PriceChainConversionFailure => ScopeError::BadScopeChainOrPrices,
            ScopeChainError::NoChainForToken => ScopeError::BadScopeChainOrPrices,
            ScopeChainError::InvalidPricesInChain => ScopeError::BadScopeChainOrPrices,
            ScopeChainError::MathOverflow => ScopeError::MathOverflow,
            ScopeChainError::IntegerConversionOverflow => ScopeError::IntegerOverflow,
        }
    }
}
