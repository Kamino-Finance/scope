use anchor_lang::prelude::*;

pub mod seeds {
    pub const CONFIG: &[u8] = b"conf";
    pub const MINTS_TO_SCOPE_CHAINS: &[u8] = b"mints_to_scope_chains";
}

pub fn config_pubkey(price_feed: &str) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[seeds::CONFIG, price_feed.as_bytes()], &crate::id())
}

pub fn mints_to_scope_chains_pubkey(
    prices_pk: &Pubkey,
    seed_pk: &Pubkey,
    seed_id: u64,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            seeds::MINTS_TO_SCOPE_CHAINS,
            prices_pk.as_ref(),
            seed_pk.as_ref(),
            &seed_id.to_le_bytes(),
        ],
        program_id,
    )
}
