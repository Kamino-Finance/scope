use anchor_lang::prelude::*;
use chainlink_streams_report::{
    feed_id::ID as FeedID,
    report::{
        v10::ReportDataV10, v3::ReportDataV3, v7::ReportDataV7, v8::ReportDataV8, v9::ReportDataV9,
    },
};
use decimal_wad::{
    common::TryMul,
    decimal::{Decimal, U192},
};
use num_bigint::BigInt;
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    debug,
    errors::ScopeError,
    info,
    utils::{
        consts::NANOSECONDS_PER_SECOND,
        math::{check_confidence_interval_decimal, estimate_slot_update_from_ts},
    },
    warn, DatedPrice, Price, ScopeResult,
};

const PRICE_STALENESS_S: u64 = 60;
const NAV_REPORT_STALENESS_IN_MS: u64 = 7 * 24 * 60 * 60 * 1000; // 7 days in milliseconds

// For xStocks the Chainlink report contains a `activation_date_time` timestamp that points to
// when a corporate action would take place (stock split, dividend) and a multiplier becomes current
// Our current design is to suspend the refresh of the price a certain period of time (given by this constant)
// before the `activation_date_time` and manually resume it later
const V10_TIME_PERIOD_BEFORE_ACTIVATION_TO_SUSPEND_S: i64 = 24 * 60 * 60; // 24 hours

#[derive(IntoPrimitive, TryFromPrimitive, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ReportDataMarketStatus {
    Unknown = 0,
    Closed,
    Open,
}

#[derive(
    IntoPrimitive,
    TryFromPrimitive,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    AnchorSerialize,
    AnchorDeserialize,
    Default,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum MarketStatusBehavior {
    #[default]
    AllUpdates = 0,
    Open,
    OpenAndPrePost,
}

/// # Ripcord Flag
/// - `0` (false): Feed's data provider is OK. Fund's data provider and accuracy is as expected.
/// - `1` (true): Feed's data provider is flagging a pause. Data provider detected outliers,
///   deviated thresholds, or operational issues. **DO NOT consume NAV data when ripcord=1.**
#[derive(IntoPrimitive, TryFromPrimitive, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ReportDataV9RipcordFlag {
    Normal = 0,
    Paused,
}

// This gives the result of a price update refresh operation
#[derive(Debug, PartialEq, Eq)]
pub enum PriceUpdateResult {
    // The result of a standard price update operation with a new updated price
    Updated,
    // For xStocks, when we've entered the blackout period before an `activation_date_time`
    // the existing price is suspended rather than updating to a new price
    SuspendExistingPrice,
}

pub trait GenericDataConvertible<const N: usize>
where
    Self: AnchorSerialize + AnchorDeserialize + Sized + 'static,
{
    const TYPE_NAME: &'static str;

    fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        Self::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize {}", Self::TYPE_NAME);
            ScopeError::InvalidGenericData
        })
    }

    fn to_generic_data(&self) -> [u8; N] {
        let mut buff = [0u8; N];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .unwrap_or_else(|_| panic!("Failed to serialize {}", Self::TYPE_NAME));
        buff
    }
}

/// Price data for standard Chainlink types (v3, v7, v8, v9)
#[derive(Debug, AnchorDeserialize, AnchorSerialize)]
pub struct ChainlinkStandardPriceData {
    pub observations_timestamp: u64,
}

impl ChainlinkStandardPriceData {
    pub fn new(observations_timestamp: u64) -> Self {
        Self {
            observations_timestamp,
        }
    }
}

/// Price data for ChainlinkX type (v10)
#[derive(Debug, AnchorDeserialize, AnchorSerialize)]
pub struct ChainlinkXPriceData {
    pub observations_timestamp: u64,
    pub suspended: bool,
    pub activation_date_time: u64,
}

impl ChainlinkXPriceData {
    pub fn new(observations_timestamp: u64, suspended: bool, activation_date_time: u64) -> Self {
        Self {
            observations_timestamp,
            suspended,
            activation_date_time,
        }
    }
}

impl GenericDataConvertible<24> for ChainlinkStandardPriceData {
    const TYPE_NAME: &'static str = "ChainlinkStandardPriceData";
}

impl GenericDataConvertible<24> for ChainlinkXPriceData {
    const TYPE_NAME: &'static str = "ChainlinkXPriceData";
}

impl GenericDataConvertible<20> for MarketStatusBehavior {
    const TYPE_NAME: &'static str = "MarketStatusBehavior";
}

fn validate_report_feed_id(feed_id: &FeedID, mapping: &Pubkey) -> ScopeResult<()> {
    if feed_id.0 != mapping.to_bytes() {
        warn!("The chainlink report provided {} does not match the expected feed id in the mapping {}",
            feed_id.to_hex_string(),
            FeedID(mapping.to_bytes()).to_hex_string()
        );
        return Err(ScopeError::PriceNotValid);
    }
    Ok(())
}

fn validate_observations_timestamp(
    observations_ts: u64,
    last_observations_ts: u64,
    clock: &Clock,
) -> ScopeResult<(u64, u64, u64)> {
    let current_onchain_ts: u64 = clock
        .unix_timestamp
        .try_into()
        .expect("Invalid clock timestamp");

    if observations_ts <= last_observations_ts {
        warn!("An outdated report was provided");
        return Err(ScopeError::BadTimestamp);
    }

    let price_ts = u64::min(observations_ts, current_onchain_ts);
    let last_updated_slot = estimate_slot_update_from_ts(clock, price_ts);

    Ok((price_ts, last_updated_slot, last_observations_ts))
}

fn validate_report_based_on_market_status(
    report_market_status: u32,
    report_last_update_timestamp: u64,
    mapping_generic_data: &[u8],
    clock: &Clock,
) -> ScopeResult<()> {
    // `last_update_timestamp` is in nanoseconds
    let now_timestamp_u64 =
        u64::try_from(clock.unix_timestamp).map_err(|_| ScopeError::ConversionFailure)?;
    let price_is_stale = report_last_update_timestamp
        < (now_timestamp_u64.saturating_sub(PRICE_STALENESS_S)) * NANOSECONDS_PER_SECOND;

    let market_status_behavior = MarketStatusBehavior::from_generic_data(mapping_generic_data)
        .map_err(|_| ScopeError::ConversionFailure)?;
    let market_status = ReportDataMarketStatus::try_from(report_market_status)
        .map_err(|_| ScopeError::ConversionFailure)?;

    match market_status_behavior {
        MarketStatusBehavior::Open => {
            // Reject all prices that are not during market open, or are during market open but price is stale
            // (which means unexpected market pause)
            if market_status != ReportDataMarketStatus::Open {
                debug!("ChainlinkRWA type DuringOpen: price received outside of market hours, rejecting update");
                return Err(ScopeError::OutsideMarketHours);
            }
            if price_is_stale {
                warn!("ChainlinkRWA type DuringOpen: price is stale (unexpected market pause), rejecting update");
                return Err(ScopeError::PriceNotValid);
            }
        }
        MarketStatusBehavior::OpenAndPrePost => {
            // Accept all prices that are not stale, which means that the update is either during market open,
            // or during pre and post market hours
            if market_status == ReportDataMarketStatus::Unknown {
                warn!("ChainlinkRWA type DuringOpenAndPrePost: market status is unknown, rejecting update");
                return Err(ScopeError::PriceNotValid);
            }
            if price_is_stale {
                if market_status == ReportDataMarketStatus::Closed {
                    debug!("ChainlinkRWA type DuringOpenAndPrePost: price is stale during market closed hours");
                    return Err(ScopeError::OutsideMarketHours);
                } else {
                    warn!(
                        "ChainlinkRWA type DuringOpenAndPrePost: price is stale, rejecting update"
                    );
                    return Err(ScopeError::PriceNotValid);
                }
            }
        }
        MarketStatusBehavior::AllUpdates => (),
    }

    Ok(())
}

pub fn update_price_v3(
    dated_price: &mut DatedPrice,
    mapping: Pubkey,
    mapping_generic_data: &[u8],
    clock: &Clock,
    chainlink_report: &ReportDataV3,
) -> ScopeResult<PriceUpdateResult> {
    validate_report_feed_id(&chainlink_report.feed_id, &mapping)?;

    // Parse existing price data
    let existing_price_data =
        ChainlinkStandardPriceData::from_generic_data(&dated_price.generic_data)?;
    let (unix_timestamp, last_updated_slot, _) = validate_observations_timestamp(
        chainlink_report.observations_timestamp.into(),
        existing_price_data.observations_timestamp,
        clock,
    )?;

    let price_dec = chainlink_bigint_value_parse(&chainlink_report.benchmark_price)?;

    let bid_dec = chainlink_bigint_value_parse(&chainlink_report.bid)?;
    let ask_dec = chainlink_bigint_value_parse(&chainlink_report.ask)?;

    let spread = ask_dec - bid_dec;

    let confidence_factor: u32 =
        AnchorDeserialize::try_from_slice(&mapping_generic_data[..4]).unwrap();
    check_confidence_interval_decimal(price_dec, spread, confidence_factor).map_err(|e| {
        warn!(
            "Chainlink provided a price '{price_dec}' with bid '{bid_dec}' and ask\
         '{ask_dec}' not fitting the configured '{confidence_factor}' confidence factor",
        );
        e
    })?;

    let price: Price = price_dec.into();
    let price_data =
        ChainlinkStandardPriceData::new(chainlink_report.observations_timestamp.into());

    *dated_price = DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        generic_data: price_data.to_generic_data(),
    };

    Ok(PriceUpdateResult::Updated)
}

pub fn update_price_v7(
    dated_price: &mut DatedPrice,
    mapping: Pubkey,
    clock: &Clock,
    chainlink_report: &ReportDataV7,
) -> ScopeResult<PriceUpdateResult> {
    validate_report_feed_id(&chainlink_report.feed_id, &mapping)?;

    // Parse existing price data
    let existing_price_data =
        ChainlinkStandardPriceData::from_generic_data(&dated_price.generic_data)?;
    let (unix_timestamp, last_updated_slot, _) = validate_observations_timestamp(
        chainlink_report.observations_timestamp.into(),
        existing_price_data.observations_timestamp,
        clock,
    )?;

    let price_dec = chainlink_bigint_value_parse(&chainlink_report.exchange_rate)?;
    let price: Price = price_dec.into();
    let price_data =
        ChainlinkStandardPriceData::new(chainlink_report.observations_timestamp.into());

    *dated_price = DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        generic_data: price_data.to_generic_data(),
    };

    Ok(PriceUpdateResult::Updated)
}

pub fn update_price_v8(
    dated_price: &mut DatedPrice,
    mapping: Pubkey,
    mapping_generic_data: &[u8],
    clock: &Clock,
    chainlink_report: &ReportDataV8,
) -> ScopeResult<PriceUpdateResult> {
    validate_report_feed_id(&chainlink_report.feed_id, &mapping)?;

    // Parse existing price data
    let existing_price_data =
        ChainlinkStandardPriceData::from_generic_data(&dated_price.generic_data)?;
    let (unix_timestamp, last_updated_slot, _) = validate_observations_timestamp(
        chainlink_report.observations_timestamp.into(),
        existing_price_data.observations_timestamp,
        clock,
    )?;

    validate_report_based_on_market_status(
        chainlink_report.market_status,
        chainlink_report.last_update_timestamp,
        mapping_generic_data,
        clock,
    )?;

    let price_dec = chainlink_bigint_value_parse(&chainlink_report.mid_price)?;
    let price: Price = price_dec.into();
    let price_data =
        ChainlinkStandardPriceData::new(chainlink_report.observations_timestamp.into());

    *dated_price = DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        generic_data: price_data.to_generic_data(),
    };

    Ok(PriceUpdateResult::Updated)
}

pub fn update_price_v9(
    dated_price: &mut DatedPrice,
    mapping: Pubkey,
    clock: &Clock,
    chainlink_report: &ReportDataV9,
) -> ScopeResult<PriceUpdateResult> {
    validate_report_feed_id(&chainlink_report.feed_id, &mapping)?;

    // Parse existing price data
    let existing_price_data =
        ChainlinkStandardPriceData::from_generic_data(&dated_price.generic_data)?;
    let (unix_timestamp, last_updated_slot, _) = validate_observations_timestamp(
        chainlink_report.observations_timestamp.into(),
        existing_price_data.observations_timestamp,
        clock,
    )?;

    // Check if nav_date is older than a week
    let current_time_ms =
        u64::try_from(clock.unix_timestamp).map_err(|_| ScopeError::ConversionFailure)? * 1000;
    if chainlink_report.nav_date + NAV_REPORT_STALENESS_IN_MS < current_time_ms {
        warn!("ChainlinkNAV: nav_date is too old, rejecting nav data");
        return Err(ScopeError::PriceNotValid);
    }

    let ripcord = ReportDataV9RipcordFlag::try_from(chainlink_report.ripcord)
        .map_err(|_| ScopeError::ConversionFailure)?;
    if ripcord == ReportDataV9RipcordFlag::Paused {
        warn!("ChainlinkNAV: feed's data provider is flagging a pause, rejecting nav data");
        return Err(ScopeError::PriceNotValid);
    }

    let price_dec = chainlink_bigint_value_parse(&chainlink_report.nav_per_share)?;
    let price: Price = price_dec.into();
    let price_data =
        ChainlinkStandardPriceData::new(chainlink_report.observations_timestamp.into());

    *dated_price = DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        generic_data: price_data.to_generic_data(),
    };

    Ok(PriceUpdateResult::Updated)
}

pub fn update_price_v10(
    dated_price: &mut DatedPrice,
    mapping: Pubkey,
    mapping_generic_data: &[u8],
    clock: &Clock,
    chainlink_report: &ReportDataV10,
) -> ScopeResult<PriceUpdateResult> {
    validate_report_feed_id(&chainlink_report.feed_id, &mapping)?;

    // Parse existing price data
    let existing_price_data = ChainlinkXPriceData::from_generic_data(&dated_price.generic_data)?;
    let (unix_timestamp, last_updated_slot, last_observations_ts) =
        validate_observations_timestamp(
            chainlink_report.observations_timestamp.into(),
            existing_price_data.observations_timestamp,
            clock,
        )?;

    // Check if this price was suspended
    if existing_price_data.suspended {
        warn!(
            "Price suspended, rejecting update: feed_id={} activation_date_time={} current_multiplier={} new_multiplier={} price={} last_valid_price={:?} market_status={}",
            chainlink_report.feed_id,
            existing_price_data.activation_date_time,
            chainlink_report.current_multiplier,
            chainlink_report.new_multiplier,
            chainlink_report.price,
            dated_price.price,
            chainlink_report.market_status
        );

        return Err(ScopeError::PriceNotValid);
    }

    // Check if the price report contains an activation time, and if we've entered the
    // blackout period, suspend the price refresh
    if chainlink_report.activation_date_time > 0 {
        let activation_time_i64 = i64::from(chainlink_report.activation_date_time);
        let activation_time_lower_bound = activation_time_i64
            .checked_sub(V10_TIME_PERIOD_BEFORE_ACTIVATION_TO_SUSPEND_S)
            .ok_or(ScopeError::BadTimestamp)?;

        if clock.unix_timestamp >= activation_time_lower_bound {
            warn!(
                "Entering the blackout period, suspending price: feed_id={} activation_date_time={} current_multiplier={} new_multiplier={} price={} last_valid_price={:?} market_status={}",
                chainlink_report.feed_id,
                chainlink_report.activation_date_time,
                chainlink_report.current_multiplier,
                chainlink_report.new_multiplier,
                chainlink_report.price,
                dated_price.price,
                chainlink_report.market_status
            );

            // Suspend the price refresh
            let price_data = ChainlinkXPriceData::new(
                last_observations_ts,
                true,
                chainlink_report.activation_date_time.into(),
            );
            dated_price.generic_data = price_data.to_generic_data();
            return Ok(PriceUpdateResult::SuspendExistingPrice);
        }
    }

    validate_report_based_on_market_status(
        chainlink_report.market_status,
        chainlink_report.last_update_timestamp,
        mapping_generic_data,
        clock,
    )?;

    let price_dec = chainlink_bigint_value_parse(&chainlink_report.price)?;
    let current_multiplier_dec =
        chainlink_bigint_value_parse(&chainlink_report.current_multiplier)?;
    // TODO(liviuc): once Chainlink has added the `total_return_price`, use that
    let multiplied_price: Price = price_dec
        .try_mul(current_multiplier_dec)
        .map_err(|_| ScopeError::MathOverflow)?
        .into();

    // Create ChainlinkX price data with activation_date_time from current report
    let price_data = ChainlinkXPriceData::new(
        chainlink_report.observations_timestamp.into(),
        false,
        chainlink_report.activation_date_time.into(),
    );

    *dated_price = DatedPrice {
        price: multiplied_price,
        last_updated_slot,
        unix_timestamp,
        generic_data: price_data.to_generic_data(),
    };

    Ok(PriceUpdateResult::Updated)
}

pub fn validate_mapping_v3(
    price_account: Option<&AccountInfo>,
    generic_data: &[u8],
) -> ScopeResult<()> {
    let Some(account) = price_account else {
        warn!("Chainlink requires a price id as account");
        return Err(ScopeError::UnexpectedAccount);
    };

    let confidence_factor: u32 = AnchorDeserialize::try_from_slice(&generic_data[..4]).unwrap();
    if confidence_factor < 1 {
        warn!("Confidence factor must be a positive integer");
        return Err(ScopeError::InvalidGenericData);
    }

    let feed_id = FeedID(account.key.to_bytes());
    info!(
        "Validating mapping for Chainlink price with feed id: {} and confidence factor of {confidence_factor}",
        feed_id.to_hex_string()
    );

    Ok(())
}

pub fn validate_mapping_v8_v10(
    price_account: Option<&AccountInfo>,
    generic_data: &[u8],
) -> ScopeResult<()> {
    let Some(account) = price_account else {
        warn!("ChainlinkRWA and ChainlinkX require a price id as account");
        return Err(ScopeError::UnexpectedAccount);
    };

    if MarketStatusBehavior::from_generic_data(generic_data).is_err() {
        warn!("Invalid market status behavior passed in");
        return Err(ScopeError::InvalidGenericData);
    }

    let feed_id = FeedID(account.key.to_bytes());
    info!(
        "Validating mapping for ChainlinkRWA/ChainlinkX price with feed id: {}",
        feed_id.to_hex_string()
    );

    Ok(())
}

pub fn validate_mapping_v7_v9(price_account: Option<&AccountInfo>) -> ScopeResult<()> {
    let Some(account) = price_account else {
        warn!("ChainlinkNAV/ChainlinkExchangeRate requires a price id as account");
        return Err(ScopeError::UnexpectedAccount);
    };

    let feed_id = FeedID(account.key.to_bytes());
    info!(
        "Validating mapping for ChainlinkNAV/ChainlinkExchangeRate price with feed id: {}",
        feed_id.to_hex_string()
    );

    Ok(())
}

fn chainlink_bigint_value_parse(value: &BigInt) -> ScopeResult<Decimal> {
    // One of the BigInt values in the Chainlink report is the price,
    // which has 18 decimals like `Decimal`
    let (sign, bytes) = value.to_bytes_le();
    if sign == num_bigint::Sign::Minus {
        warn!("Chainlink provided a non supported negative BigInt value");
        return Err(ScopeError::PriceNotValid);
    }
    if bytes.len() > 24 {
        warn!("Chainlink provided a BigInt value not fitting in 192 bits");
        return Err(ScopeError::PriceNotValid);
    }
    let scaled_value = U192::from_little_endian(&bytes);
    Ok(Decimal(scaled_value))
}

pub(super) mod cfg_data {
    use anchor_lang::prelude::*;

    use super::MarketStatusBehavior;

    #[derive(Clone, Copy, Debug, AnchorSerialize, AnchorDeserialize)]
    pub struct V3 {
        pub confidence_factor: u32,
    }

    impl V3 {
        pub fn from_generic_data(data: &[u8; 20]) -> Result<V3> {
            let mut buff: &[u8] = &data[..];
            AnchorDeserialize::deserialize(&mut buff).map_err(Into::into)
        }
    }

    #[derive(Clone, Copy, Debug, AnchorSerialize, AnchorDeserialize)]
    pub struct V8V10 {
        pub market_status_behavior: MarketStatusBehavior,
    }

    impl V8V10 {
        pub fn from_generic_data(data: &[u8; 20]) -> Result<V8V10> {
            let mut buff: &[u8] = &data[..];
            let market_status_behavior: MarketStatusBehavior =
                AnchorDeserialize::deserialize(&mut buff)?;
            Ok(V8V10 {
                market_status_behavior,
            })
        }
    }
}

pub mod chainlink_streams_itf {
    use anchor_lang::{
        prelude::*,
        solana_program::{
            instruction::{AccountMeta, Instruction},
            pubkey::Pubkey,
        },
    };
    use solana_program::pubkey;

    #[cfg(not(feature = "devnet"))]
    pub const ACCESS_CONTROLLER_PUBKEY: Pubkey =
        pubkey!("7mSn5MoBjyRLKoJShgkep8J17ueGG8rYioVAiSg5YWMF");

    #[cfg(feature = "devnet")]
    pub const ACCESS_CONTROLLER_PUBKEY: Pubkey =
        pubkey!("2k3DsgwBoqrnvXKVvd7jX7aptNxdcRBdcd5HkYsGgbrb");

    pub const VERIFIER_CONFIG_PUBKEY: Pubkey =
        pubkey!("HJR45sRiFdGncL69HVzRK4HLS2SXcVW3KeTPkp2aFmWC");

    pub const VERIFIER_PROGRAM_ID: Pubkey = pubkey!("Gt9S41PtjR58CbG9JhJ3J6vxesqrNAswbWYbLNTMZA3c");

    pub const VERIFY_DISCRIMINATOR: [u8; 8] = [133, 161, 141, 48, 120, 198, 88, 150];

    #[derive(AnchorDeserialize, AnchorSerialize)]
    struct VerifyParams {
        signed_report: Vec<u8>,
    }

    /// Creates a verify instruction.
    ///
    /// # Parameters:
    ///
    /// * `program_id` - The public key of the verifier program.
    /// * `verifier_account` - The public key of the verifier account. The function [`Self::get_verifier_config_pda`] can be used to calculate this.
    /// * `access_controller_account` - The public key of the access controller account.
    /// * `user` - The public key of the user - this account must be a signer
    /// * `report_config_account` - The public key of the report configuration account. The function [`Self::get_config_pda`] can be used to calculate this.
    /// * `signed_report` - The signed report data as a vector of bytes. Returned from data streams API/WS
    ///
    /// # Returns
    ///
    /// Returns an `Instruction` object that can be sent to the Solana runtime.
    pub fn verify(
        program_id: &Pubkey,
        verifier_account: &Pubkey,
        access_controller_account: &Pubkey,
        user: &Pubkey,
        report_config_account: &Pubkey,
        signed_report: Vec<u8>,
    ) -> Instruction {
        let accounts = vec![
            AccountMeta::new_readonly(*verifier_account, false),
            AccountMeta::new_readonly(*access_controller_account, false),
            AccountMeta::new_readonly(*user, true),
            AccountMeta::new_readonly(*report_config_account, false),
        ];

        // 8 bytes for discriminator
        // 4 bytes size of the length prefix for the signed_report vector
        let mut instruction_data = Vec::with_capacity(8 + 4 + signed_report.len());
        instruction_data.extend_from_slice(&VERIFY_DISCRIMINATOR);

        let params = VerifyParams { signed_report };
        let param_data = params.try_to_vec().unwrap();
        instruction_data.extend_from_slice(&param_data);

        Instruction {
            program_id: *program_id,
            accounts,
            data: instruction_data,
        }
    }

    /// Helper to compute the report config PDA account. This uses the first 32 bytes of the
    /// uncompressed report as the seed. This is validated within the verifier program
    pub fn get_config_pda(report: &[u8]) -> Pubkey {
        Pubkey::find_program_address(&[&report[..32]], &VERIFIER_PROGRAM_ID).0
    }
}
