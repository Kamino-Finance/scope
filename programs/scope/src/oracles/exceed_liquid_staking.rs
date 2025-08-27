use anchor_lang::prelude::{AccountInfo, Clock};
pub use exceed_liquid_staking_itf::state::Pair;
use exceed_liquid_staking_itf::ID;

use crate::{utils::account_deserialize, DatedPrice, Price, ScopeError};

/// Get the current exchange rate of any Exceed LST pair
pub fn get_price(account_info: &AccountInfo, clock: &Clock) -> Result<DatedPrice, ScopeError> {
    let pair: Pair = account_deserialize(account_info)?;
    let exchange_rate = pair
        .calculate_exchange_rate(clock.unix_timestamp)
        .ok_or(ScopeError::MathOverflow)?;

    Ok(DatedPrice {
        price: Price {
            value: exchange_rate,
            exp: 12, // PRECISION is 1e12
        },
        last_updated_slot: clock.slot,
        unix_timestamp: clock.unix_timestamp as u64,
        generic_data: [0; 24],
    })
}

pub fn validate_account(account: &AccountInfo) -> Result<(), ScopeError> {
    if account.owner != &ID {
        return Err(ScopeError::WrongAccountOwner);
    }

    let _: Pair = account_deserialize(account)?;

    Ok(())
}
