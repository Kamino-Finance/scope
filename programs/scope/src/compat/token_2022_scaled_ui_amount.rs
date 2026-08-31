//! Parser for Token-2022's `ScaledUiAmount` mint extension.
//!
//! The official `spl-token-2022-interface` crate cannot coexist with this workspace's
//! solana 1.16 dependency graph (its mandatory confidential-transfer proof dependency requires
//! `solana-curve25519` with Agave unstable APIs), and the in-tree `spl-token-2022` both
//! predates the extension and hard-errors on mints carrying unknown extension types in its TLV
//! walker — so this is parsed manually. Everything transcribed here can be checked against the
//! `spl-token-2022` v7.0.0 sources: the account layout and TLV framing are defined in its
//! `extension/mod.rs`, and the `ScaledUiAmountConfig` field order in its
//! `extension/scaled_ui_amount/mod.rs`.
//!
//! Extended mint account layout:
//!
//! | offset   | content                                                  |
//! |----------|----------------------------------------------------------|
//! | 0..82    | base `Mint` (unchanged legacy wire format)               |
//! | 82..165  | zero padding to the legacy `Account` length              |
//! | 165      | `AccountType` byte (`1` = mint)                          |
//! | 166..    | TLV entries: type (u16 LE), length (u16 LE), then value  |

use anchor_spl::token_2022::spl_token_2022::{
    extension::AccountType,
    state::{Account as TokenAccount, Mint},
};
use solana_program::program_pack::Pack;

use crate::{warn, ScopeError, ScopeResult};

/// `ExtensionType::ScaledUiAmount` in upstream spl-token-2022 v7+ — absent from this tree's
/// older enum, which is the reason this module exists.
pub const SCALED_UI_AMOUNT_EXTENSION_TYPE: u16 = 25;
/// `ExtensionType::Uninitialized`: terminates the TLV list.
const EXTENSION_TYPE_UNINITIALIZED: u16 = 0;
/// The account-type byte follows the zero padding to the legacy `Account` length.
const ACCOUNT_TYPE_OFFSET: usize = TokenAccount::LEN;
/// The first TLV entry starts right after the account-type byte.
const TLV_START_OFFSET: usize = ACCOUNT_TYPE_OFFSET + 1;
/// Bytes of a TLV entry's type field (u16).
const TLV_TYPE_LEN: usize = 2;
/// Bytes of a full TLV entry header: type (u16) + value length (u16).
const TLV_HEADER_LEN: usize = 4;

// `ScaledUiAmountConfig`'s wire layout, field order pinned to upstream v7.
const AUTHORITY_LEN: usize = 32;
const MULTIPLIER_OFFSET: usize = AUTHORITY_LEN;
const NEW_MULTIPLIER_EFFECTIVE_TIMESTAMP_OFFSET: usize = MULTIPLIER_OFFSET + 8;
const NEW_MULTIPLIER_OFFSET: usize = NEW_MULTIPLIER_EFFECTIVE_TIMESTAMP_OFFSET + 8;
pub const CONFIG_LEN: usize = NEW_MULTIPLIER_OFFSET + 8;

/// The `ScaledUiAmountConfig` fields relevant to reading the multiplier (the update authority
/// is deliberately not exposed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaledUiAmountMultipliers {
    pub multiplier: f64,
    pub new_multiplier: f64,
    pub new_multiplier_effective_timestamp: i64,
}

impl ScaledUiAmountMultipliers {
    /// The multiplier in effect at `unix_timestamp`. This selection is upstream ABI semantics
    /// (`ScaledUiAmountConfig::total_multiplier`, without its division by `10^decimals` which
    /// converts raw amounts to UI amounts and is not part of the scaling factor itself).
    pub fn effective_multiplier(&self, unix_timestamp: i64) -> f64 {
        if unix_timestamp >= self.new_multiplier_effective_timestamp {
            self.new_multiplier
        } else {
            self.multiplier
        }
    }
}

/// Validates a Token-2022 mint's account envelope and TLV framing, returning the
/// `ScaledUiAmountConfig` value bytes.
///
/// The whole account is validated, not just the entry of interest: the base mint must unpack as
/// an initialized `Mint`, the padding to the legacy `Account` length must be zero, the
/// account-type byte must be `Mint`, and every TLV entry — including ones after the
/// `ScaledUiAmount` entry — must be well-framed. Unknown extension types are
/// skipped by their declared length without interpreting their discriminant, a zero type
/// terminates the list, and a duplicate `ScaledUiAmount` entry is rejected rather than silently
/// picking one.
fn find_scaled_ui_amount_config(data: &[u8]) -> ScopeResult<&[u8]> {
    // The base mint keeps the legacy wire format; `unpack` also enforces `is_initialized`.
    let base_mint = data.get(..Mint::LEN).ok_or_else(|| {
        warn!("Scaled ui amount: account data too short for a mint");
        ScopeError::UnexpectedAccount
    })?;
    Mint::unpack(base_mint).map_err(|_| {
        warn!("Scaled ui amount: invalid or uninitialized base mint");
        ScopeError::UnexpectedAccount
    })?;

    // Upstream's `type_and_tlv_indices` requires this padding to be entirely zero, so a non-zero
    // byte here means the account is not a structurally valid extended mint.
    let Some(padding) = data.get(Mint::LEN..ACCOUNT_TYPE_OFFSET) else {
        warn!("Scaled ui amount: mint carries no extension data");
        return Err(ScopeError::UnexpectedAccount);
    };
    if padding.iter().any(|&byte| byte != 0) {
        warn!("Scaled ui amount: non-zero padding between the base mint and the account type");
        return Err(ScopeError::UnexpectedAccount);
    }

    if data.get(ACCOUNT_TYPE_OFFSET) != Some(&u8::from(AccountType::Mint)) {
        warn!("Scaled ui amount: mint carries no extension data");
        return Err(ScopeError::UnexpectedAccount);
    }

    let mut config: Option<&[u8]> = None;
    let mut cursor = TLV_START_OFFSET;
    while cursor < data.len() {
        // The type is read alone first: a zero type terminates the list without requiring
        // length bytes after it, and fewer than two remaining bytes cannot hold another type at
        // all — upstream treats that as leftover realloc tail space and stops successfully.
        let Some(type_bytes) = data.get(cursor..cursor + TLV_TYPE_LEN) else {
            break;
        };
        let extension_type = u16::from_le_bytes([type_bytes[0], type_bytes[1]]);
        if extension_type == EXTENSION_TYPE_UNINITIALIZED {
            break;
        }
        let length_bytes = data
            .get(cursor + TLV_TYPE_LEN..cursor + TLV_HEADER_LEN)
            .ok_or_else(|| {
                warn!("Scaled ui amount: truncated TLV entry length");
                ScopeError::UnexpectedAccount
            })?;
        let value_len = usize::from(u16::from_le_bytes([length_bytes[0], length_bytes[1]]));
        let value_start = cursor + TLV_HEADER_LEN;
        let value_end = value_start.checked_add(value_len).ok_or_else(|| {
            warn!("Scaled ui amount: TLV entry length overflows");
            ScopeError::UnexpectedAccount
        })?;
        let value = data.get(value_start..value_end).ok_or_else(|| {
            warn!("Scaled ui amount: truncated TLV entry value");
            ScopeError::UnexpectedAccount
        })?;
        if extension_type == SCALED_UI_AMOUNT_EXTENSION_TYPE {
            if value_len != CONFIG_LEN {
                warn!(
                    "Scaled ui amount: config has length {} (expected {})",
                    value_len, CONFIG_LEN
                );
                return Err(ScopeError::UnexpectedAccount);
            }
            if config.is_some() {
                warn!("Scaled ui amount: duplicate ScaledUiAmount extension");
                return Err(ScopeError::UnexpectedAccount);
            }
            config = Some(value);
        }
        cursor = value_end;
    }
    config.ok_or_else(|| {
        warn!("Scaled ui amount: mint has no ScaledUiAmount extension");
        ScopeError::UnexpectedAccount
    })
}

/// Parses a Token-2022 mint's account data (see [`find_scaled_ui_amount_config`] for what is
/// validated) and decodes its `ScaledUiAmount` multipliers.
pub fn parse_scaled_ui_amount_multipliers(data: &[u8]) -> ScopeResult<ScaledUiAmountMultipliers> {
    let config = find_scaled_ui_amount_config(data)?;
    let le_bytes = |offset: usize| -> [u8; 8] {
        config[offset..offset + 8]
            .try_into()
            .expect("config length is validated by find_scaled_ui_amount_config")
    };
    Ok(ScaledUiAmountMultipliers {
        multiplier: f64::from_le_bytes(le_bytes(MULTIPLIER_OFFSET)),
        new_multiplier: f64::from_le_bytes(le_bytes(NEW_MULTIPLIER_OFFSET)),
        new_multiplier_effective_timestamp: i64::from_le_bytes(le_bytes(
            NEW_MULTIPLIER_EFFECTIVE_TIMESTAMP_OFFSET,
        )),
    })
}
