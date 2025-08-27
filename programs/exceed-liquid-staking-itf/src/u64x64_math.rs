// Taken from Meteora DLMMM Program

use ruint::aliases::U256;

// Precision when converting from decimal to fixed point. Or the other way around. 10^12
pub const PRECISION: u128 = 1_000_000_000_000;

// Number of bits to scale. This will decide the position of the radix point.
pub const SCALE_OFFSET: u8 = 64;

// Where does this value come from ?
// When smallest bin is used (1 bps), the maximum of bin limit is 887272 (Check: https://docs.traderjoexyz.com/concepts/bin-math).
// But in solana, the token amount is represented in 64 bits, therefore, it will be (1 + 0.0001)^n < 2 ** 64, solve for n, n ~= 443636
// Then we calculate bits needed to represent 443636 exponential, 2^n >= 443636, ~= 19
// If we convert 443636 to binary form, it will be 1101100010011110100 (19 bits).
// Which, the 19 bits are the bits the binary exponential will loop through.
// The 20th bit will be 0x80000,  which the exponential already > the maximum number of bin Q64.64 can support
const MAX_EXPONENTIAL: u32 = 0x80000; // 1048576

// 1.0000... representation of 64x64
pub const ONE: u128 = 1u128 << SCALE_OFFSET;

pub fn pow(base: u128, exp: i32) -> Option<u128> {
    let mut invert = exp.is_negative();

    if exp == 0 {
        return Some(1u128 << 64);
    }

    let exp: u32 = if invert { exp.abs() as u32 } else { exp as u32 };

    if exp >= MAX_EXPONENTIAL {
        return None;
    }

    let mut squared_base = base;
    let mut result = ONE;

    if squared_base >= result {
        squared_base = u128::MAX.checked_div(squared_base)?;
        invert = !invert;
    }

    if exp & 0x1 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x2 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x4 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x8 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x10 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x20 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x40 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x80 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x100 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x200 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x400 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x800 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x1000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x2000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x4000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x8000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x10000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x20000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;

    if exp & 0x40000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }

    if result == 0 {
        return None;
    }

    if invert {
        result = u128::MAX.checked_div(result)?;
    }

    Some(result)
}

// pub fn calculate_eight_hour_apr(basis_points: u128) -> Option<u128> {
//     // Calculate the annual rate in fixed-point representation
//     let annual_rate = ONE + (basis_points * ONE) / (PRECISION * 10000);
//     msg!("annual_rate: {}", annual_rate);

//     // Number of 8-hour intervals in a year
//     let intervals_per_year = 3 * 365;

//     // Calculate the natural logarithm of the annual rate
//     let ln_annual_rate = (annual_rate - ONE) * ONE / annual_rate;
//     msg!("ln_annual_rate: {}", ln_annual_rate);

//     // Scale the logarithm by the fraction of the year represented by 8 hours
//     let scaled_ln = ln_annual_rate / intervals_per_year;
//     msg!("scaled_ln: {}", scaled_ln);

//     // Exponentiate the result to get the 8-hour rate
//     let eight_hour_rate = ONE + scaled_ln;
//     msg!("eight_hour_rate: {}", eight_hour_rate);

//     Some(eight_hour_rate)
// }

pub fn to_decimal(value: u128) -> Option<u128> {
    let value = U256::from(value);
    let precision = U256::from(PRECISION);
    let scaled_value = value.checked_mul(precision)?;
    // ruint checked math is different with the rust std u128. If there's bit with 1 value being shifted out, it will return None. Therefore, we use overflowing_shr
    let (scaled_down_value, _) = scaled_value.overflowing_shr(SCALE_OFFSET.into());
    scaled_down_value.try_into().ok()
}

// Helper function to convert decimal with 10^12 precision to fixed point number
pub fn from_decimal(value: u128) -> Option<u128> {
    let value = U256::from(value);
    let precision = U256::from(PRECISION);
    let (q_value, _) = value.overflowing_shl(SCALE_OFFSET.into());
    let fp_value = q_value.checked_div(precision)?;
    fp_value.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // fn test_calculate_eight_hour_apr() {
    //     // Test with 2000 basis points (20% APY)
    //     let basis_points = 2000;
    //     let expected_eight_hour_apr = 1_000_182_648_401; // Adjust this value based on expected result

    //     let result = calculate_eight_hour_apr(basis_points).unwrap();
    //     let result_decimal = to_decimal(result).unwrap();

    //     assert_eq!(result_decimal, expected_eight_hour_apr);
    // }
    #[test]
    fn test_pow_positive_exponent() {
        // Test base^exp with positive exponent
        assert_eq!(
            to_decimal(pow(from_decimal(2 * PRECISION).unwrap(), 30).unwrap()).unwrap() / PRECISION,
            1073741824
        );
        assert_eq!(
            to_decimal(pow(from_decimal(3 * PRECISION).unwrap(), 2).unwrap()).unwrap(),
            9 * PRECISION
        );

        assert_eq!(
            to_decimal(pow(from_decimal(1_200_000_000_000).unwrap(), 2).unwrap()).unwrap(),
            1_440_000_000_000
        );

        // Auto-compounded on 8 hour rate
        assert_eq!(
            to_decimal(pow(from_decimal(1_000_166_517_567).unwrap(), 1095).unwrap()).unwrap(),
            1_199_999_999_545
        );

        // 2-year test
        assert_eq!(
            to_decimal(pow(from_decimal(1_000_166_517_567).unwrap(), 1095 * 2).unwrap()).unwrap(),
            1_439_999_998_909
        );

        // Auto-compounded hourly
        // assert_eq!(
        //     to_decimal(pow(from_decimal(1_000_020_813_179).unwrap(), 8760).unwrap()).unwrap(),
        //     1_200_000_000_000
        // );

        // Auto-compounded minutely -> Overflow Error
        // assert_eq!(
        //     to_decimal(pow(from_decimal(1_000_000_346_882).unwrap(), 525600).unwrap()).unwrap(),
        //     1_200_000_000_000
        // );

        // Daily Compounding
        // assert_eq!(
        //     to_decimal(pow(from_decimal(1_000_499_635_891).unwrap(), 365).unwrap()).unwrap(),
        //     1_200_000_000_000
        // );
    }

    #[test]
    fn test_pow_zero_exponent() {
        // Test base^0 which should always be 1
        assert_eq!(
            to_decimal(pow(from_decimal(2 * PRECISION).unwrap(), 0).unwrap()).unwrap(),
            PRECISION
        );
        assert_eq!(
            to_decimal(pow(from_decimal(3 * PRECISION).unwrap(), 0).unwrap()).unwrap(),
            PRECISION
        );
    }

    #[test]
    fn test_pow_overflow_exponent() {
        // Test exponent that exceeds MAX_EXPONENTIAL
        assert_eq!(
            pow(
                from_decimal(2 * PRECISION).unwrap(),
                MAX_EXPONENTIAL as i32 + 1
            ),
            None
        );
    }

    #[test]
    fn test_pow_large_base() {
        // Test large base with small exponent
        assert_eq!(
            to_decimal(pow(from_decimal(2 * PRECISION).unwrap(), 1).unwrap()).unwrap(),
            2 * PRECISION
        );
    }
}
