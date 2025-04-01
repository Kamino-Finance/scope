use anchor_lang::prelude::*;
use decimal_wad::{common::PERCENT_SCALER, decimal::Decimal};

use super::math::ten_pow;
use crate::{warn, Price, ScopeError};

pub const MAX_REF_RATIO_TOLERANCE_PCT: u64 = 5;
pub const MAX_REF_RATIO_TOLERANCE_SCALED: u64 = MAX_REF_RATIO_TOLERANCE_PCT * PERCENT_SCALER;

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

pub fn check_ref_price_difference(curr_price: Price, ref_price: Price) -> Result<()> {
    let ref_price_decimal = Decimal::from(ref_price);
    let curr_price_decimal = Decimal::from(curr_price);
    let absolute_diff = if ref_price_decimal > curr_price_decimal {
        ref_price_decimal - curr_price_decimal
    } else {
        curr_price_decimal - ref_price_decimal
    };

    if absolute_diff * 100 > ref_price_decimal * MAX_REF_RATIO_TOLERANCE_PCT {
        warn!(
            "Price diff is too high: absolute_diff {}, tolerance = {}",
            absolute_diff,
            ref_price_decimal * Decimal::from_percent(MAX_REF_RATIO_TOLERANCE_PCT)
        );
        return Err(ScopeError::PriceNotValid.into());
    }

    Ok(())
}

fn decimal_to_price(decimal: Decimal) -> Price {
    // this implementation aims to keep as much precision as possible
    // choose exp to be as big as possible (minimize what is needed for the integer part)

    // Use a match instead of log10 to save some CUs
    let (exp, ten_pow_exp) = match decimal
        .try_round::<u64>()
        .expect("Decimal integer part is too big")
    {
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
    };
    let value = (decimal * ten_pow_exp)
        .try_round::<u64>()
        .unwrap_or_else(|e| {
            panic!("Decimal {decimal} conversion to price failed (exp:{exp}): {e:?}");
        });
    Price { value, exp }
}

impl From<Decimal> for Price {
    fn from(val: Decimal) -> Self {
        decimal_to_price(val)
    }
}

impl From<Price> for Decimal {
    fn from(val: Price) -> Self {
        Decimal::from(val.value) / 10u128.pow(val.exp as u32)
    }
}

#[cfg(not(target_os = "solana"))]
impl From<f64> for Price {
    fn from(val: f64) -> Self {
        let number_of_integer_digits = val.log10() as i64;
        let exp = if number_of_integer_digits >= 0 {
            12_u8.saturating_sub(number_of_integer_digits as u8)
        } else {
            u8::min((12 + number_of_integer_digits.abs()) as u8, 18)
        };
        let value = (val * 10f64.powi(exp.into())) as u64;
        Price {
            value,
            exp: exp.into(),
        }
    }
}

impl PartialEq for Price {
    fn eq(&self, other: &Self) -> bool {
        match self.exp.cmp(&other.exp) {
            std::cmp::Ordering::Equal => self.value == other.value,
            std::cmp::Ordering::Greater => {
                let diff = self.exp - other.exp;
                let other_value = other.value * 10u64.pow(diff as u32);
                self.value == other_value
            }
            std::cmp::Ordering::Less => {
                let diff = other.exp - self.exp;
                let self_value = self.value * 10u64.pow(diff as u32);
                self_value == other.value
            }
        }
    }
}

impl Eq for Price {}
