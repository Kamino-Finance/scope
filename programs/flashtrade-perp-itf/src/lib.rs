pub mod states;

pub use states::*;

use anchor_lang::prelude::*;

pub const USD_DECIMALS: u8 = 6;

declare_id!("FLASH6Lo6h3iasJKWDs2F8TkW2UKf3s15C8PMGuVfgBn");

#[program]
pub mod flashtrade {}