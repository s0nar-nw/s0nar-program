use anchor_lang::prelude::*;

use crate::error::CustomErrors;
use crate::state::RegistryAccount;

#[derive(Accounts)]
pub struct ProposeAuthority<'info> {
    // Global registry storing protocol configuration
    #[account(
        mut,
        has_one = authority @ CustomErrors::UnAuthorizedCaller,
    )]
    pub registry: Account<'info, RegistryAccount>,

    // Current authority of the protocol
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct AcceptAuthority<'info> {
    // Global registry storing protocol configuration
    #[account(mut)]
    pub registry: Account<'info, RegistryAccount>,

    // New proposed authority
    pub new_authority: Signer<'info>,
}

/// Proposes a new authority for the registry.
///
/// This instruction:
/// - Sets the `pending_authority` field in the registry to the provided public key.
/// - Requires the caller to be the current authority.
pub fn propose(ctx: Context<ProposeAuthority>, new_authority: Pubkey) -> Result<()> {
    let registry = &mut ctx.accounts.registry;
    registry.pending_authority = Some(new_authority);
    Ok(())
}

/// Accepts the pending authority transfer.
///
/// This instruction:
/// - Verifies the caller matches the `pending_authority`.
/// - Updates the registry's `authority` to the new authority's public key.
/// - Clears the `pending_authority` field.
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
