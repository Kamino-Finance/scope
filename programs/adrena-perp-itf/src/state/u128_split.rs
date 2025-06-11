use std::ops::{AddAssign, SubAssign};

use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};

// U128Split is a struct that represents a u128 as two u64s for zero_copy needs
#[derive(
    Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug, Pod, Zeroable,
)]
#[repr(C)]
pub struct U128Split {
    high: u64,
    low: u64,
}

impl U128Split {
    // Create a new U128Split from a u128
    pub fn new(value: u128) -> Self {
        let high = (value >> 64) as u64;
        let low = value as u64;
        U128Split { high, low }
    }

    // Convert the U128Split back into a u128
    pub fn to_u128(&self) -> u128 {
        ((self.high as u128) << 64) | (self.low as u128)
    }

    // Add a u128 to this U128Split
    pub fn add(&mut self, other: u128) {
        let other_split = U128Split::new(other);
        let (low, carry) = self.low.overflowing_add(other_split.low);
        let high = self.high + other_split.high + (carry as u64);
        self.high = high;
        self.low = low;
    }

    // Method to perform left shift operation
    pub fn left_shift(&mut self, shift: u32) {
        if shift == 0 {
            return;
        }

        let total_bits = 128;
        let u64_bits = 64;

        if shift >= total_bits {
            self.high = 0;
            self.low = 0;
        } else if shift >= u64_bits {
            self.high = self.low << (shift - u64_bits);
            self.low = 0;
        } else {
            self.high = (self.high << shift) | (self.low >> (u64_bits - shift));
            self.low <<= shift;
        }
    }
}

impl std::ops::Add for U128Split {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        let (low, carry) = self.low.overflowing_add(other.low);
        let high = self.high + other.high + (carry as u64);
        U128Split { high, low }
    }
}

impl std::ops::Sub for U128Split {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        let (low, borrow) = self.low.overflowing_sub(other.low);
        let high = self.high - other.high - (borrow as u64);
        U128Split { high, low }
    }
}

impl From<u128> for U128Split {
    fn from(item: u128) -> Self {
        U128Split::new(item)
    }
}

impl From<u64> for U128Split {
    fn from(item: u64) -> Self {
        U128Split::new(item as u128)
    }
}

impl From<i32> for U128Split {
    fn from(item: i32) -> Self {
        U128Split::new(item as u128)
    }
}

impl AddAssign for U128Split {
    fn add_assign(&mut self, other: Self) {
        let (low, carry) = self.low.overflowing_add(other.low);
        let high = self.high + other.high + (carry as u64);
        self.high = high;
        self.low = low;
    }
}

impl AddAssign<u64> for U128Split {
    fn add_assign(&mut self, other: u64) {
        let (low, carry) = self.low.overflowing_add(other);
        self.high += carry as u64; // Only add to high if there's a carry
        self.low = low;
    }
}

impl AddAssign<u128> for U128Split {
    fn add_assign(&mut self, other: u128) {
        let other_split = U128Split::new(other);
        self.add_assign(other_split);
    }
}

impl SubAssign for U128Split {
    fn sub_assign(&mut self, other: Self) {
        let (low, borrow) = self.low.overflowing_sub(other.low);
        let high = self.high - other.high - (borrow as u64);
        self.high = high;
        self.low = low;
    }
}

impl SubAssign<u64> for U128Split {
    fn sub_assign(&mut self, other: u64) {
        let (low, borrow) = self.low.overflowing_sub(other);
        self.high -= borrow as u64; // Only subtract from high if there's a borrow
        self.low = low;
    }
}

impl SubAssign<u128> for U128Split {
    fn sub_assign(&mut self, other: u128) {
        let other_split = U128Split::new(other);
        self.sub_assign(other_split);
    }
}
