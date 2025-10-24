pub mod consts;
pub mod macros;
pub mod math;
pub mod pdas;
pub mod price_impl;
pub mod scope_chain;

use std::cell::{Ref, RefMut};

use anchor_lang::{
    __private::bytemuck,
    prelude::{AccountDeserialize, AccountInfo},
    Discriminator, Key,
};
pub use decimal_wad;

use crate::{warn, ScopeError, ScopeResult};

pub const SECONDS_PER_HOUR: i64 = 60 * 60;

pub fn account_deserialize<T: AccountDeserialize + Discriminator>(
    account: &AccountInfo<'_>,
) -> ScopeResult<T> {
    let data = account.clone().data.borrow().to_owned();
    let discriminator = data.get(..8).ok_or_else(|| {
        warn!(
            "Account {:?} does not have enough bytes to be deserialized",
            account.key()
        );
        ScopeError::UnableToDeserializeAccount
    })?;
    if discriminator != T::discriminator() {
        warn!(
            "Expected discriminator for account {:?} ({:?}) is different from received {:?}",
            account.key(),
            T::discriminator(),
            discriminator
        );
        return Err(ScopeError::InvalidAccountDiscriminator);
    }

    let mut data: &[u8] = &data;
    let user: T = T::try_deserialize(&mut data).map_err(|_| {
        warn!("Account {:?} deserialization failed", account.key());
        ScopeError::UnableToDeserializeAccount
    })?;

    Ok(user)
}

pub fn zero_copy_deserialize<'info, T: bytemuck::AnyBitPattern + Discriminator>(
    account: &'info AccountInfo,
) -> ScopeResult<Ref<'info, T>> {
    let data = account.data.try_borrow().unwrap();

    let disc_bytes = data.get(..8).ok_or_else(|| {
        warn!(
            "Account {:?} does not have enough bytes to be deserialized",
            account.key()
        );
        ScopeError::UnableToDeserializeAccount
    })?;
    if disc_bytes != T::discriminator() {
        warn!(
            "Expected discriminator for account {:?} ({:?}) is different from received {:?}",
            account.key(),
            T::discriminator(),
            disc_bytes
        );
        return Err(ScopeError::InvalidAccountDiscriminator);
    }
    let end = std::mem::size_of::<T>() + 8;
    Ok(Ref::map(data, |data| bytemuck::from_bytes(&data[8..end])))
}

pub fn zero_copy_deserialize_mut<'info, T: bytemuck::Pod + Discriminator>(
    account: &'info AccountInfo,
) -> ScopeResult<RefMut<'info, T>> {
    let data = account.data.try_borrow_mut().unwrap();

    let disc_bytes = data.get(..8).ok_or_else(|| {
        warn!(
            "Account {:?} does not have enough bytes to be deserialized",
            account.key()
        );
        ScopeError::UnableToDeserializeAccount
    })?;
    if disc_bytes != T::discriminator() {
        warn!(
            "Expected discriminator for account {:?} ({:?}) is different from received {:?}",
            account.key(),
            T::discriminator(),
            disc_bytes
        );
        return Err(ScopeError::InvalidAccountDiscriminator);
    }
    let end = std::mem::size_of::<T>() + 8;
    Ok(RefMut::map(data, |data| {
        bytemuck::from_bytes_mut(&mut data[8..end])
    }))
}

/// Changes an AccountInfo to an Option<AccountInfo>:
/// - Some(_) if the account is different from the program
/// - None if the account is the program
pub fn maybe_account<'a, 'b>(account: &'a AccountInfo<'b>) -> Option<&'a AccountInfo<'b>> {
    if account.key() == crate::ID {
        None
    } else {
        Some(account)
    }
}

/// Lists the bit positions (where LSB == 0) of all the set bits (i.e. `1`s) in the given number's
/// binary representation.
/// NOTE: This is a non-critical helper used only for logging of the update operation; should *not*
/// be needed by business logic. The implementation is a compressed version of a crate
/// https://docs.rs/bit-iter/1.2.0/src/bit_iter/lib.rs.html.
pub fn list_set_bit_positions(mut bits: u64) -> Vec<u8> {
    let mut positions = Vec::with_capacity(usize::try_from(bits.count_ones()).unwrap());
    while bits != 0 {
        let position = u8::try_from(bits.trailing_zeros()).unwrap();
        positions.push(position);
        bits &= bits.wrapping_sub(1);
    }
    positions
}
