use crate::{error::NeutronErrors, ObserverAccount, RegistryAccount, OBSERVER_SEED, REGISTRY_SEED};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

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
        bump = observer_account.bump
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

/// De-Registers an existing observer node in the Neutron network.
///
/// This instruction:
/// - Guards against unauthorized caller and already inactive observer
/// - Transfers `stake_lamports` to the observer wallet from the PDA
/// - Updates the is_active field in observer_account to be FALSE
/// - Decrements `registry.active_count`
pub fn deregister(ctx: Context<DeregisterObserver>) -> Result<()> {
    let is_observer = ctx.accounts.caller.key() == ctx.accounts.observer_wallet.key();
    let is_authority = ctx.accounts.caller.key() == ctx.accounts.registry.authority;

    require!(
        is_observer || is_authority,
        NeutronErrors::UnAuthorizedCaller
    );

    // Ensure observer_account is active
    require!(
        ctx.accounts.observer_account.is_active,
        NeutronErrors::ObserverAlreadyInActive
    );

    let stake_lamports = ctx.accounts.observer_account.stake_lamports;
    // Transfer stake_lamports from the observer_account PDA to observer wallet
    if stake_lamports > 0 {
        let key_binding = ctx.accounts.observer_wallet.key();
        let seeds = &[
            OBSERVER_SEED,
            key_binding.as_ref(),
            &[ctx.accounts.observer_account.bump],
        ];

        let signer_seeds = &[&seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.observer_account.to_account_info(),
                to: ctx.accounts.observer_wallet.to_account_info(),
            },
            signer_seeds,
        );
        transfer(cpi_ctx, stake_lamports)?;
    }

    // Updating observer to be inactive
    let observer = &mut ctx.accounts.observer_account;
    observer.is_active = false;

    let registry = &mut ctx.accounts.registry;

    // Decrement registry counts
    registry.active_count = registry.active_count.saturating_sub(1);

    Ok(())
}
