use crate::{error::CustomErrors, ObserverAccount, RegistryAccount, OBSERVER_SEED, REGISTRY_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct DeregisterObserver<'info> {
    // Observer themselves OR the registry authority (for slashing)
    #[account(mut)]
    pub caller: Signer<'info>,

    /// CHECK: Verified implicitly — observer_account PDA is derived from this key.
    /// Recipient of returned stake lamports on deregistration.
    #[account(mut)]
    pub observer_wallet: AccountInfo<'info>,

    // Per-observer state
    #[account(
        mut,
        seeds = [OBSERVER_SEED, observer_wallet.key().as_ref()],
        bump = observer_account.bump,
        close = observer_wallet
    )]
    pub observer_account: Account<'info, ObserverAccount>,

    #[account(
        mut,
        seeds = [REGISTRY_SEED],
        bump = registry.bump,
    )]
    pub registry: Account<'info, RegistryAccount>,

    pub system_program: Program<'info, System>,
}

/// De-Registers an existing observer node in the s0nar network.
///
/// This instruction:
/// - Guards against unauthorized caller and already inactive observer
/// - Transfers `stake_lamports` to the observer wallet from the PDA
/// - Decrements `registry.active_count`
pub fn deregister(ctx: Context<DeregisterObserver>) -> Result<()> {
    let is_observer = ctx.accounts.caller.key() == ctx.accounts.observer_wallet.key();
    let is_authority = ctx.accounts.caller.key() == ctx.accounts.registry.authority;

    require!(
        is_observer || is_authority,
        CustomErrors::UnAuthorizedCaller
    );

    // Ensure observer_account is active
    require!(
        ctx.accounts.observer_account.is_active,
        CustomErrors::ObserverAlreadyInActive
    );

    let stake_lamports = ctx.accounts.observer_account.stake_lamports;

    if stake_lamports > 0 {
        let pda_info = &mut ctx.accounts.observer_account.to_account_info();
        let wallet_info = &mut ctx.accounts.observer_wallet.to_account_info();

        **pda_info.try_borrow_mut_lamports()? -= stake_lamports;
        **wallet_info.try_borrow_mut_lamports()? += stake_lamports;
    }

    let registry = &mut ctx.accounts.registry;

    // Decrement registry counts
    registry.active_count = registry.active_count.saturating_sub(1);

    Ok(())
}
