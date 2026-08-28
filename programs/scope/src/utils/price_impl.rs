use anchor_lang::prelude::*;
use decimal_wad::decimal::Decimal;
use num_traits::ToPrimitive;

use super::math::ten_pow;
use crate::{utils::consts::FULL_BPS, warn, Price, ScopeError, ScopeResult};

pub const MAX_REF_RATIO_TOLERANCE_BPS: u16 = 500;

#[cfg(not(target_os = "solana"))]
impl From<Price> for f64 {
    fn from(val: Price) -> Self {
        val.value as f64 / 10u64.pow(val.exp as u32) as f64
    }
}

impl Price {
    pub fn to_scaled_value(&self, decimals: u8) -> u128 {
        let exp = u8::try_from(self.exp).expect("Price exp is too big");
        let value: u128 = self.value.into();
        if exp > decimals {
            let diff = exp - decimals;
            value / ten_pow(diff)
        } else {
            let diff = decimals - exp;
            value * ten_pow(diff)
        }
    }
}

pub fn check_ref_price_difference(
    curr_price: Price,
    ref_price: Price,
    ref_price_tolerance_bps: Option<u16>,
) -> Result<()> {
    let ref_price_decimal = Decimal::from(ref_price);
    let curr_price_decimal = Decimal::from(curr_price);
    let absolute_diff = if ref_price_decimal > curr_price_decimal {
        ref_price_decimal - curr_price_decimal
    } else {
        curr_price_decimal - ref_price_decimal
    };

    let max_ref_ratio_tolerance_bps =
        u64::from(ref_price_tolerance_bps.unwrap_or(MAX_REF_RATIO_TOLERANCE_BPS));
    if absolute_diff * FULL_BPS > ref_price_decimal * max_ref_ratio_tolerance_bps {
        warn!(
            "Price diff is too high: absolute diff is {} for ref price {};max tolerance in bps = {}",
            absolute_diff, ref_price_decimal, max_ref_ratio_tolerance_bps
        );
        return Err(ScopeError::PriceNotValid.into());
    }

    Ok(())
}

/// Returns `(exp, 10^exp)` for a number with the given integer part. Shared by the `Decimal`
/// and `f64` conversions so both normalize identically.
fn dynamic_price_exp(integer_part: u64) -> (u64, u64) {
    // this implementation aims to keep as much precision as possible
    // choose exp to be as big as possible (minimize what is needed for the integer part)

    // Use a match instead of log10 to save some CUs
    match integer_part {
        0_u64 => (18, 10_u64.pow(18)),
        1..=9 => (17, 10_u64.pow(17)),
        10..=99 => (16, 10_u64.pow(16)),
        100..=999 => (15, 10_u64.pow(15)),
        1000..=9999 => (14, 10_u64.pow(14)),
        10000..=99999 => (13, 10_u64.pow(13)),
        100000..=999999 => (12, 10_u64.pow(12)),
        1000000..=9999999 => (11, 10_u64.pow(11)),
        10000000..=99999999 => (10, 10_u64.pow(10)),
        100000000..=999999999 => (9, 10_u64.pow(9)),
        1000000000..=9999999999 => (8, 10_u64.pow(8)),
        10000000000..=99999999999 => (7, 10_u64.pow(7)),
        100000000000..=999999999999 => (6, 10_u64.pow(6)),
        1000000000000..=9999999999999 => (5, 10_u64.pow(5)),
        10000000000000..=99999999999999 => (4, 10_u64.pow(4)),
        100000000000000..=999999999999999 => (3, 10_u64.pow(3)),
        1000000000000000..=9999999999999999 => (2, 10_u64.pow(2)),
        10000000000000000..=99999999999999999 => (1, 10_u64.pow(1)),
        100000000000000000..=u64::MAX => (0, 1),
    }
}

fn decimal_to_price(decimal: Decimal) -> ScopeResult<Price> {
    let (exp, ten_pow_exp) = dynamic_price_exp(
        decimal
            .try_round::<u64>()
            .map_err(|_| ScopeError::MathOverflow)?,
    );
    let value = (decimal * ten_pow_exp)
        .try_round::<u64>()
        .map_err(|_| ScopeError::MathOverflow)?;
    Ok(Price { value, exp })
}

impl TryFrom<Decimal> for Price {
    type Error = ScopeError;

    fn try_from(val: Decimal) -> std::result::Result<Self, Self::Error> {
        decimal_to_price(val)
    }
}

/// The `f64` counterpart of [`decimal_to_price`], sharing its normalization
/// ([`dynamic_price_exp`]) and its rounding behavior. Values a `Price` can't represent are
/// rejected: NaN, infinite and negative values as `ConversionFailure`, values whose integer
/// part exceeds `u64::MAX` as `MathOverflow`.
fn f64_to_price(val: f64) -> ScopeResult<Price> {
    if !val.is_finite() || val < 0.0 {
        return Err(ScopeError::ConversionFailure);
    }
    // `to_u64` truncates, so adding 0.5 first rounds to the nearest integer - the intent is to
    // match the `Decimal` path's `try_round`, though it doesn't match it for all inputs.
    let integer_part = (val + 0.5).to_u64().ok_or(ScopeError::MathOverflow)?;
    let (exp, ten_pow_exp) = dynamic_price_exp(integer_part);
    let ten_pow_exp = ten_pow_exp.to_f64().ok_or(ScopeError::ConversionFailure)?;
    let value = (val * ten_pow_exp + 0.5)
        .to_u64()
        .ok_or(ScopeError::MathOverflow)?;
    Ok(Price { value, exp })
}

impl TryFrom<f64> for Price {
    type Error = ScopeError;

    fn try_from(val: f64) -> std::result::Result<Self, Self::Error> {
        f64_to_price(val)
    }
}

impl From<Price> for Decimal {
    fn from(val: Price) -> Self {
        Decimal::from(val.value) / 10u128.pow(val.exp as u32)
    }
}
