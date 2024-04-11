use anchor_lang::prelude::Pubkey;

pub const MINT_SEED: &[u8] = b"lp_token_mint";

pub fn get_mint_pk(pool_pk: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[MINT_SEED, &pool_pk.to_bytes()], &crate::ID)
}

pub fn check_mint_pk(pool_pk: &Pubkey, expected_mint_pk: &Pubkey, bump: u8) -> Result<(), Errors> {
    let mint_pk =
        Pubkey::create_program_address(&[MINT_SEED, &pool_pk.to_bytes(), &[bump]], &crate::ID)
            .map_err(|_| Errors::UnableToDerivePDA)?;
    if mint_pk != *expected_mint_pk {
        Err(Errors::WrongMint)
    } else {
        Ok(())
    }
}

pub enum Errors {
    /// Unable to derive the pool mint PDA
    UnableToDerivePDA,
    /// The mint account is not the expected one
    WrongMint,
}
