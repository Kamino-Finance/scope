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

    #[msg("Invalid token metadata update mode")]
    InvalidTokenUpdateMode,

    #[msg("Unable to derive PDA address")]
    UnableToDerivePDA,

    #[msg("Invalid timestamp")]
    BadTimestamp,

    #[msg("Invalid slot")]
    BadSlot,

    #[msg("TWAP price account is different than Scope ID")]
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
