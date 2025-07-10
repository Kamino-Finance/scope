pub const CONFIGURATION_SIZE: usize = 10232;
pub const ORACLE_MAPPING_SIZE: usize = 29696;
pub const ORACLE_PRICES_SIZE: usize = 28704;
pub const ORACLE_TWAPS_SIZE: usize = 344128;
pub const TOKEN_METADATA_SIZE: usize = 86016;

/// Factor used to check confidence interval of oracle prices
/// Used when calling [`crate::utils::math::check_confidence_interval`]
/// for pyth prices (confidence interval check) and switchboard prices (standard deviation check)
pub const ORACLE_CONFIDENCE_FACTOR: u32 = super::math::confidence_bps_to_factor(200); // 2%

pub const FULL_BPS: u16 = 10_000;

pub const SECONDS_PER_YEAR: u64 = 365 * 24 * 60 * 60;
pub const MILLIS_PER_SECOND: u64 = 1_000;
pub const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;
