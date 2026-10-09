pub const CONFIGURATION_SIZE: usize = 10232;
pub const ORACLE_MAPPING_SIZE: usize = 29696;
pub const ORACLE_PRICES_SIZE: usize = 28704;
pub const ORACLE_TWAPS_SIZE: usize = 344128;
pub const TOKEN_METADATA_SIZE: usize = 86016;

pub const FULL_BPS: u16 = 10_000;

/// How long the approved multiplier stays the reference auto approval is measured against. Once
/// elapsed, the next published multiplier becomes the reference, so the auto approval threshold
/// bounds the cumulative change per day rather than each step.
pub const AUTO_APPROVAL_ANCHOR_PERIOD_S: u64 = 24 * 60 * 60; // 24 hours

/// Largest auto approval threshold a mapping may configure. A multiplier moving more than this in
/// a day is a corporate action an operator should look at, not something to publish unattended.
pub const MAX_DAILY_AUTO_APPROVAL_BPS: u16 = 100; // 1%

pub const SECONDS_PER_YEAR: u64 = 365 * 24 * 60 * 60;
pub const MILLIS_PER_SECOND: u64 = 1_000;
pub const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

/// Size of source entries arrays used by composite oracles (MostRecentOf, CappedMostRecentOf, MultiplicationChain)
pub const SOURCE_ENTRIES_CHAIN_SIZE: usize = 4;
