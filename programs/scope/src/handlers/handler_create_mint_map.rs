use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

use crate::{
    states::mints_to_scope_chains::{MintToScopeChain, MintsToScopeChains},
    utils::pdas::seeds,
};

#[derive(Accounts)]
#[instruction(
    seed_pk: Pubkey,
    seed_id: u64,
    bump: u8,
    scope_chains: Vec<[u16; 4]>,
)]
pub struct CreateMintMap<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(has_one = admin)]
    pub configuration: AccountLoader<'info, crate::states::Configuration>,
    #[account(
        init,
        seeds = [seeds::MINTS_TO_SCOPE_CHAINS, configuration.load()?.oracle_prices.as_ref(), seed_pk.as_ref(), &seed_id.to_le_bytes()],
        bump,
        space = 8 + MintsToScopeChains::size_from_len(scope_chains.len()),
        payer = admin,
    )]
    pub mappings: Account<'info, MintsToScopeChains>,

    pub system_program: Program<'info, System>,
    // Mints are passed as extra accounts
}

pub fn process(
    ctx: Context<CreateMintMap>,
    seed_pk: Pubkey,
    seed_id: u64,
    bump: u8,
    scope_chains: Vec<[u16; 4]>,
) -> Result<()> {
    require_eq!(ctx.remaining_accounts.len(), scope_chains.len());

    ctx.accounts.mappings.set_inner(MintsToScopeChains {
        seed_pk,
        seed_id,
        bump,
        oracle_prices: ctx.accounts.configuration.load()?.oracle_prices,
        mapping: scope_chains
            .iter()
            .zip(ctx.remaining_accounts.iter())
            .map(|(chain, mint)| {
                let mint_data = mint.data.borrow();
                let _: Mint = Mint::try_deserialize_unchecked(&mut mint_data.as_ref())?;
                Ok(MintToScopeChain {
                    mint: *mint.key,
                    scope_chain: *chain,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    });

    Ok(())
}
