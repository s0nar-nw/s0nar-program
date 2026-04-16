use anchor_lang::prelude::*;

use crate::error::CustomErrors;
use crate::state::RegistryAccount;

#[derive(Accounts)]
pub struct ProposeAuthority<'info> {
    #[account(
        mut,
        has_one = authority @ CustomErrors::UnAuthorizedCaller,
    )]
    pub registry: Account<'info, RegistryAccount>,

    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct AcceptAuthority<'info> {
    #[account(mut)]
    pub registry: Account<'info, RegistryAccount>,

    pub new_authority: Signer<'info>,
}

pub fn propose(ctx: Context<ProposeAuthority>, new_authority: Pubkey) -> Result<()> {
    let registry = &mut ctx.accounts.registry;
    registry.pending_authority = Some(new_authority);
    Ok(())
}

pub fn accept(ctx: Context<AcceptAuthority>) -> Result<()> {
    let registry = &mut ctx.accounts.registry;

    require!(
        registry.pending_authority == Some(ctx.accounts.new_authority.key()),
        CustomErrors::InvalidPendingAuthority
    );

    registry.authority = ctx.accounts.new_authority.key();
    registry.pending_authority = None;

    Ok(())
}
