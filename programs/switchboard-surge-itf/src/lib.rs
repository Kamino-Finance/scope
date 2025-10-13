pub mod smallvec;
pub mod ed25519_sysvar;
pub mod feed_info;
pub mod switchboard_quote;

use anchor_lang::prelude::*;

// Re-export commonly used types
pub use anchor_lang::solana_program::pubkey::Pubkey;
pub use anchor_lang::solana_program::borsh;

declare_id!("orac1eFjzWL5R3RbbdMV68K9H6TaCVVcL6LjvQQWAbz");

/// Prelude module containing commonly used constants and re-exports
pub mod prelude {
    pub use super::Pubkey;
    pub use super::borsh;

    /// Precision for fixed-point decimal representation (10^18)
    pub const PRECISION: u32 = 18;
}
