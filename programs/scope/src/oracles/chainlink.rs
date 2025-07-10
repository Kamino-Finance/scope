use anchor_lang::prelude::*;
use chainlink_streams_report::{
    feed_id::ID as FeedID,
    report::{v3::ReportDataV3, v8::ReportDataV8},
};
use decimal_wad::decimal::{Decimal, U192};
use num_bigint::BigInt;
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    borsh::BorshSerialize,
    errors::ScopeError,
    info,
    utils::{
        consts::NANOSECONDS_PER_SECOND,
        math::{check_confidence_interval_decimal, estimate_slot_update_from_ts},
    },
    warn, DatedPrice, Price, ScopeResult,
};

const PRICE_STALENESS_S: u64 = 60;

#[derive(IntoPrimitive, TryFromPrimitive, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
enum ReportDataV8MarketStatus {
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
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum MarketStatusBehavior {
    AllUpdates = 0,
    Open,
    OpenAndPrePost,
}

impl MarketStatusBehavior {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize MarketStatusBehavior");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut cursor = &mut buff[..];
        BorshSerialize::serialize(&self, &mut cursor)
            .expect("Failed to serialize MarketStatusBehavior");
        buff
    }
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
    dated_price: &DatedPrice,
    clock: &Clock,
) -> ScopeResult<(u64, u64, [u8; 24])> {
    let current_onchain_ts: u64 = clock
        .unix_timestamp
        .try_into()
        .expect("Invalid clock timestamp");
    let last_observations_ts =
        u64::from_le_bytes(dated_price.generic_data[0..8].try_into().unwrap());

    if observations_ts <= last_observations_ts {
        warn!("An outdated report was provided");
        return Err(ScopeError::BadTimestamp);
    }

    let price_ts = u64::min(observations_ts, current_onchain_ts);

    let last_updated_slot = estimate_slot_update_from_ts(clock, price_ts);
    let mut generic_data = [0u8; 24];
    generic_data[..8].copy_from_slice(&observations_ts.to_le_bytes());

    Ok((price_ts, last_updated_slot, generic_data))
}

pub fn update_price_v3(
    dated_price: &mut DatedPrice,
    mapping: Pubkey,
    mapping_generic_data: &[u8],
    clock: &Clock,
    chainlink_report: &ReportDataV3,
) -> ScopeResult<()> {
    validate_report_feed_id(&chainlink_report.feed_id, &mapping)?;
    let (unix_timestamp, last_updated_slot, generic_data) = validate_observations_timestamp(
        chainlink_report.observations_timestamp.into(),
        dated_price,
        clock,
    )?;

    let price_dec = chainlink_price_parse(&chainlink_report.benchmark_price)?;

    let bid_dec = chainlink_price_parse(&chainlink_report.bid)?;
    let ask_dec = chainlink_price_parse(&chainlink_report.ask)?;

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

    *dated_price = DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        generic_data,
    };

    Ok(())
}

pub fn update_price_v4(
    dated_price: &mut DatedPrice,
    mapping: Pubkey,
    mapping_generic_data: &[u8],
    clock: &Clock,
    chainlink_report: &ReportDataV8,
) -> ScopeResult<()> {
    validate_report_feed_id(&chainlink_report.feed_id, &mapping)?;
    let (unix_timestamp, last_updated_slot, generic_data) = validate_observations_timestamp(
        chainlink_report.observations_timestamp.into(),
        dated_price,
        clock,
    )?;

    // `last_update_timestamp` is in nanoseconds
    let now_timestamp_u64 =
        u64::try_from(clock.unix_timestamp).map_err(|_| ScopeError::ConversionFailure)?;
    let price_is_stale = chainlink_report.last_update_timestamp
        < (now_timestamp_u64.saturating_sub(PRICE_STALENESS_S)) * NANOSECONDS_PER_SECOND;

    let market_status_behavior = MarketStatusBehavior::from_generic_data(mapping_generic_data)
        .map_err(|_| ScopeError::ConversionFailure)?;
    let market_status = ReportDataV8MarketStatus::try_from(chainlink_report.market_status)
        .map_err(|_| ScopeError::ConversionFailure)?;

    match market_status_behavior {
        MarketStatusBehavior::Open => {
            // Reject all prices that are not during market open, or are during market open but price is stale
            // (which means unexpected market pause)
            if market_status != ReportDataV8MarketStatus::Open {
                warn!("ChainlinkRWA type DuringOpen: price received outside of market hours, rejecting update");
                return Err(ScopeError::PriceNotValid);
            }
            if price_is_stale {
                warn!("ChainlinkRWA type DuringOpen: price is stale (unexpected market pause), rejecting update");
                return Err(ScopeError::PriceNotValid);
            }
        }
        MarketStatusBehavior::OpenAndPrePost => {
            // Accept all prices that are not stale, which means that the update is either during market open,
            // or during pre and post market hours
            if market_status == ReportDataV8MarketStatus::Unknown {
                warn!("ChainlinkRWA type DuringOpenAndPrePost: market status is unknown, rejecting update");
                return Err(ScopeError::PriceNotValid);
            }
            if price_is_stale {
                warn!("ChainlinkRWA type DuringOpenAndPrePost: price is stale, rejecting update");
                return Err(ScopeError::PriceNotValid);
            }
        }
        MarketStatusBehavior::AllUpdates => (),
    }

    let price_dec = chainlink_price_parse(&chainlink_report.mid_price)?;
    let price: Price = price_dec.into();

    *dated_price = DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        generic_data,
    };

    Ok(())
}

pub fn validate_mapping_v3(
    price_account: &Option<AccountInfo>,
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

pub fn validate_mapping_v4(
    price_account: &Option<AccountInfo>,
    generic_data: &[u8],
) -> ScopeResult<()> {
    let Some(account) = price_account else {
        warn!("ChainlinkRWA requires a price id as account");
        return Err(ScopeError::UnexpectedAccount);
    };

    if MarketStatusBehavior::from_generic_data(generic_data).is_err() {
        warn!("Invalid market status behavior passed in");
        return Err(ScopeError::InvalidGenericData);
    }

    let feed_id = FeedID(account.key.to_bytes());
    info!(
        "Validating mapping for ChainlinkRWA price with feed id: {}",
        feed_id.to_hex_string()
    );

    Ok(())
}

fn chainlink_price_parse(price: &BigInt) -> ScopeResult<Decimal> {
    // Chainlink price have 18 decimals like `Decimal`
    let (sign, bytes) = price.to_bytes_le();
    if sign == num_bigint::Sign::Minus {
        warn!("Chainlink provided a non supported negative price");
        return Err(ScopeError::PriceNotValid);
    }
    if bytes.len() > 24 {
        warn!("Chainlink provided a price not fitting in 192 bits");
        return Err(ScopeError::PriceNotValid);
    }
    let scaled_value = U192::from_little_endian(&bytes);
    Ok(Decimal(scaled_value))
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
