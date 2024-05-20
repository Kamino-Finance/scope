pub mod consts;
pub mod macros;
pub mod math;
pub mod pdas;
pub mod price_impl;
pub mod scope_chain;

use std::cell::{Ref, RefMut};

use anchor_lang::{
    __private::bytemuck,
    prelude::{msg, AccountDeserialize, AccountInfo},
    Discriminator, Key,
};

use crate::{ScopeError, ScopeResult};

pub const SECONDS_PER_HOUR: i64 = 60 * 60;

pub fn account_deserialize<T: AccountDeserialize + Discriminator>(
    account: &AccountInfo<'_>,
) -> ScopeResult<T> {
    let data = account.clone().data.borrow().to_owned();
    let discriminator = data.get(..8).ok_or_else(|| {
        msg!(
            "Account {:?} does not have enough bytes to be deserialized",
            account.key()
        );
        ScopeError::UnableToDeserializeAccount
    })?;
    if discriminator != T::discriminator() {
        msg!(
            "Expected discriminator for account {:?} ({:?}) is different from received {:?}",
            account.key(),
            T::discriminator(),
            discriminator
        );
        return Err(ScopeError::InvalidAccountDiscriminator);
    }

    let mut data: &[u8] = &data;
    let user: T = T::try_deserialize(&mut data).map_err(|_| {
        msg!("Account {:?} deserialization failed", account.key());
        ScopeError::UnableToDeserializeAccount
    })?;

    Ok(user)
}

pub fn zero_copy_deserialize<'info, T: bytemuck::AnyBitPattern + Discriminator>(
    account: &'info AccountInfo,
) -> ScopeResult<Ref<'info, T>> {
    let data = account.data.try_borrow().unwrap();

    let disc_bytes = data.get(..8).ok_or_else(|| {
        msg!(
            "Account {:?} does not have enough bytes to be deserialized",
            account.key()
        );
        ScopeError::UnableToDeserializeAccount
    })?;
    if disc_bytes != T::discriminator() {
        msg!(
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
        msg!(
            "Account {:?} does not have enough bytes to be deserialized",
            account.key()
        );
        ScopeError::UnableToDeserializeAccount
    })?;
    if disc_bytes != T::discriminator() {
        msg!(
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
