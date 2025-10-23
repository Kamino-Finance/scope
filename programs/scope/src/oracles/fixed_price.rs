use anchor_lang::prelude::*;

use crate::{warn, Price, ScopeError};

pub fn parse_generic_data(generic_data: &[u8; 20]) -> Result<Price> {
    let mut price_data: &[u8] = generic_data;
    let price: Price = AnchorDeserialize::deserialize(&mut price_data)
        .map_err(|_| error!(ScopeError::FixedPriceInvalid))?;
    Ok(price)
}

pub fn validate_mapping(
    price_account: Option<&AccountInfo>,
    generic_data: &[u8; 20],
) -> Result<()> {
    if price_account.is_some() {
        warn!("No account is expected with a fixed price oracle");
        return err!(ScopeError::PriceNotValid);
    }
    let mut price_data: &[u8] = generic_data;
    let _price: Price = AnchorDeserialize::deserialize(&mut price_data)
        .map_err(|_| error!(ScopeError::FixedPriceInvalid))?;
    Ok(())
}
