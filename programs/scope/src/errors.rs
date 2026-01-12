use std::num::TryFromIntError;

use anchor_lang::prelude::*;
use decimal_wad::error::DecimalError;
use num_enum::{TryFromPrimitive, TryFromPrimitiveError};

#[error_code]
#[derive(PartialEq, Eq, TryFromPrimitive)]
pub enum ScopeError {
    #[msg("Integer overflow")]
    IntegerOverflow,

    #[msg("Conversion failure")]
    ConversionFailure,

    #[msg("Mathematical operation with overflow")]
    MathOverflow,

    #[msg("Out of range integral conversion attempted")]
    OutOfRangeIntegralConversion,

    #[msg("Unexpected account in instruction")]
    UnexpectedAccount,

    #[msg("Price is not valid")]
    PriceNotValid,

    #[msg("The number of tokens is different from the number of received accounts")]
    AccountsAndTokenMismatch,

    #[msg("The token index received is out of range")]
    BadTokenNb,

    #[msg("The token type received is invalid")]
    BadTokenType,

    #[msg("There was an error with the Switchboard V2 retrieval")]
    SwitchboardV2Error,

    #[msg("Invalid account discriminator")]
    InvalidAccountDiscriminator,

    #[msg("Unable to deserialize account")]
    UnableToDeserializeAccount,

    #[msg("Error while computing price with ScopeChain")]
    BadScopeChainOrPrices,

    #[msg("Refresh price instruction called in a CPI")]
    RefreshInCPI,

    #[msg("Refresh price instruction preceded by unexpected ixs")]
    RefreshWithUnexpectedIxs,

    #[msg("Invalid update sequence or accounts")]
    InvalidUpdateSequenceOrAccounts,

    #[msg("Unable to derive PDA address")]
    UnableToDerivePDA,

    #[msg("Invalid timestamp")]
    BadTimestamp,

    #[msg("Invalid slot")]
    BadSlot,

    #[msg("Received a price account when none is expected")]
    PriceAccountNotExpected,

    #[msg("TWAP source index out of range")]
    TwapSourceIndexOutOfRange,

    #[msg("TWAP sample is too close to the previous one")]
    TwapSampleTooFrequent,

    #[msg("Unexpected JLP configuration")]
    UnexpectedJlpConfiguration,

    #[msg("Not enough price samples in period to compute TWAP")]
    TwapNotEnoughSamplesInPeriod,

    #[msg("The provided token list to refresh is empty")]
    EmptyTokenList,

    #[msg("The stake pool fee is higher than the maximum allowed")]
    StakeFeeTooHigh,

    #[msg("Cannot get a valid price for the tokens composing the Ktoken")]
    KTokenUnderlyingPriceNotValid,

    #[msg("Error while computing the Ktoken pool holdings")]
    KTokenHoldingsCalculationError,

    #[msg("Cannot resize the account we only allow it to grow in size")]
    CannotResizeAccount,

    #[msg("The provided fixed price is invalid")]
    FixedPriceInvalid,

    #[msg("Switchboard On Demand price derive error")]
    SwitchboardOnDemandError,

    #[msg("Confidence interval check failed")]
    ConfidenceIntervalCheckFailed,

    #[msg("Invalid generic data")]
    InvalidGenericData,

    #[msg("No valid Chainlink report data found")]
    NoChainlinkReportData,

    #[msg("Invalid Chainlink report data format")]
    InvalidChainlinkReportData,

    #[msg("MostRecentOf config must contain at least one valid source index")]
    MostRecentOfInvalidSourceIndices,

    #[msg("Invalid max divergence (bps) for MostRecentOf oracle")]
    MostRecentOfInvalidMaxDivergence,

    #[msg("Invalid max age (s) for MostRecentOf oracle")]
    MostRecentOfInvalidMaxAge,

    #[msg("Max age diff constraint violated for MostRecentOf oracle")]
    MostRecentOfMaxAgeViolated,

    #[msg("Max divergence bps constraint violated for MostRecentOf oracle")]
    MostRecentOfMaxDivergenceBpsViolated,

    #[msg("The invoked pyth lazer verify instruction failed")]
    PythLazerVerifyIxFailed,

    #[msg("Invalid feed id passed in to PythLazer oracle")]
    PythLazerInvalidFeedID,

    #[msg("Invalid exponent passed in to PythLazer oracle")]
    PythLazerInvalidExponent,

    #[msg("Invalid confidence factor passed in to PythLazer oracle")]
    PythLazerInvalidConfidenceFactor,

    #[msg("Received an invalid message payload in the PythLazer oracle when refreshing price")]
    PythLazerInvalidMessagePayload,

    #[msg("Received an invalid channel in the PythLazer payload when refreshing price")]
    PythLazerInvalidChannel,

    #[msg("Payload should have a single feed in the PythLazer payload when refreshing price")]
    PythLazerInvalidFeedsLength,

    #[msg("Invalid feed id in the PythLazer payload when refreshing price")]
    PythLazerInvalidFeedId,

    #[msg("Property fields in the feed of the PythLazer payload do not contain a price")]
    PythLazerPriceNotPresent,

    #[msg("Property fields in the feed of the PythLazer payload do not contain a best bid price")]
    PythLazerBestBidPriceNotPresent,

    #[msg("Property fields in the feed of the PythLazer payload do not contain a best ask price")]
    PythLazerBestAskPriceNotPresent,

    #[msg("Invalid ask/bid prices provided in the feed of the PythLazer payload")]
    PythLazerInvalidAskBidPrices,

    #[msg("Price account expected when updating mapping")]
    ExpectedPriceAccount,

    #[msg("Provided account has a different owner than expected")]
    WrongAccountOwner,

    #[msg("Provided source index is invalid")]
    CompositeOracleInvalidSourceIndex,

    #[msg("Can't set both cap and floor to None for CappedFloored oracle")]
    CappedFlooredBothCapAndFloorAreNone,

    #[msg("Missing price account for Oracle Mapping update")]
    MissingPriceAccount,

    #[msg("Cannot resume a ChainlinkX price that was not suspended")]
    ChainlinkXPriceNotSuspended,

    #[msg("Price update rejected as outside of market hours")]
    OutsideMarketHours,

    #[msg("Property fields in the feed of the PythLazer payload do not contain an exponent")]
    PythLazerExponentNotPresent,

    #[msg("The exponent provided in the feed of the PythLazer payload is not the expected one")]
    PythLazerUnexpectedExponent,

    #[msg("Cannot convert oracle type to EMA type")]
    InvalidConversionToEmaTypeForOracleType,

    #[msg("Invalid TWAP enabled bitmask value")]
    TwapEnabledBitmaskConversionFailure,
}

impl<T> From<TryFromPrimitiveError<T>> for ScopeError
where
    T: TryFromPrimitive,
{
    fn from(_: TryFromPrimitiveError<T>) -> Self {
        ScopeError::ConversionFailure
    }
}

impl From<TryFromIntError> for ScopeError {
    fn from(_: TryFromIntError) -> Self {
        ScopeError::OutOfRangeIntegralConversion
    }
}

pub type ScopeResult<T = ()> = std::result::Result<T, ScopeError>;

impl From<DecimalError> for ScopeError {
    fn from(err: DecimalError) -> ScopeError {
        match err {
            DecimalError::MathOverflow => ScopeError::IntegerOverflow,
        }
    }
}
