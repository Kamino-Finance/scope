use anchor_lang::prelude::Pubkey;
use solana_program::pubkey;

#[cfg(all(feature = "mainnet", feature = "localnet"))]
compile_error!("'mainnet' and 'localnet' features are mutually exclusive");

#[cfg(all(feature = "mainnet", feature = "devnet"))]
compile_error!("'mainnet' and 'devnet' features are mutually exclusive");

#[cfg(all(feature = "localnet", feature = "devnet"))]
compile_error!("'localnet' and 'devnet' features are mutually exclusive");

#[cfg(all(feature = "mainnet", feature = "skip_price_validation"))]
compile_error!("'mainnet' and 'skip_price_validation' features are mutually exclusive");

#[cfg(all(feature = "mainnet", feature = "staging"))]
compile_error!("'mainnet' and 'staging' features are mutually exclusive");

#[cfg(all(feature = "localnet", feature = "staging"))]
compile_error!("'localnet' and 'staging' features are mutually exclusive");

#[cfg(all(feature = "devnet", feature = "staging"))]
compile_error!("'devnet' and 'staging' features are mutually exclusive");

#[cfg(all(feature = "skip_price_validation", feature = "staging"))]
compile_error!("'skip_price_validation' and 'staging' features are mutually exclusive");

cfg_if::cfg_if! {
    if #[cfg(feature = "mainnet")] {
        pub const PROGRAM_ID:Pubkey = pubkey!("HFn8GnPADiny6XqUoWE8uRPPxb29ikn4yTuPa9MF2fWJ");
    }
    else if #[cfg(feature = "staging")] {
        pub const PROGRAM_ID:Pubkey = pubkey!("scpStzYvKzE7DHwsGMP5XLhcMTuLr3feoiC9mJ3yHr5");
    }
    else if #[cfg(feature = "localnet")] {
        pub const PROGRAM_ID:Pubkey = pubkey!("2fU6YqiA2aj9Ct1tDagA8Tng7otgxHM5KwgnsUWsMFxM");
    } else if #[cfg(feature = "devnet")] {
        pub const PROGRAM_ID:Pubkey = pubkey!("3Vw8Ngkh1MVJTPHthmUbmU2XKtFEkjYvJzMqrv2rh9yX");
    } else {
        compile_error!("At least one of 'mainnet', 'staging', 'localnet' or 'devnet' feature need to be set");
    }
}
