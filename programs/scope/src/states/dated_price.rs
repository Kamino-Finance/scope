use std::cmp::Ordering;

use anchor_lang::prelude::*;

const MAX_SAFE_EXP_DIFF: u64 = 19;

#[zero_copy]
#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct Price {
    // Pyth price, integer + exponent representation
    // decimal price would be
    // as integer: 6462236900000, exponent: 8
    // as float:   64622.36900000

    // value is the scaled integer
    // for example, 6462236900000 for btc
    pub value: u64,

    // exponent represents the number of decimals
    // for example, 8 for btc
    pub exp: u64,
}

impl PartialEq for Price {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Price {}

impl Ord for Price {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.exp.cmp(&other.exp) {
            Ordering::Equal => self.value.cmp(&other.value),
            Ordering::Greater => {
                let diff = self.exp - other.exp;
                // When the diff in exponents is larger, the power of ten below doesn't fit in a `u64`
                // thus we can tell that `self` is less than `other`
                if diff > MAX_SAFE_EXP_DIFF {
                    return Ordering::Less;
                }
                // When the multiplication overflows, `self` is less than `other`
                if let Some(other_value) = other.value.checked_mul(10u64.pow(diff as u32)) {
                    self.value.cmp(&other_value)
                } else {
                    Ordering::Less
                }
            }
            Ordering::Less => {
                let diff = other.exp - self.exp;
                if diff > MAX_SAFE_EXP_DIFF {
                    return Ordering::Greater;
                }
                if let Some(value) = self.value.checked_mul(10u64.pow(diff as u32)) {
                    value.cmp(&other.value)
                } else {
                    Ordering::Greater
                }
            }
        }
    }
}

impl PartialOrd for Price {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[zero_copy]
#[derive(Debug, Eq, PartialEq, Default)]
pub struct DatedPrice {
    pub price: Price,
    pub last_updated_slot: u64,
    pub unix_timestamp: u64,
    pub generic_data: [u8; 24],
}
