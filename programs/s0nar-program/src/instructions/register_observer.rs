use crate::{
    error::CustomErrors, Attestation, ObserverAccount, Region, RegistryAccount, OBSERVER_SEED,
    REGISTRY_SEED,
};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

#[derive(Accounts)]
pub struct RegisterObserver<'info> {
    // Observer initializing the Observer account
    #[account(mut)]
    pub observer: Signer<'info>,

    // Per-observer state
    #[account(
        init,
        payer = observer,
        space = ObserverAccount::LEN,
        seeds = [OBSERVER_SEED, observer.key().as_ref()],
        bump
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

/// Registers a new observer node in the s0nar network.
///
/// This instruction:
/// - Guards against a paused registry and a full observer set
/// - Transfers `min_stake_lamports` from the observer wallet into the PDA as escrow (if > 0)
/// - Initializes all `ObserverAccount` fields
/// - Increments `registry.observer_count` and `registry.active_count`
pub fn register(ctx: Context<RegisterObserver>, region: Region) -> Result<()> {
    // Ensure registry is active and observer limit is not exceeded
    require!(!ctx.accounts.registry.paused, CustomErrors::RegistryPaused);
    require!(
        ctx.accounts.registry.active_count < ctx.accounts.registry.max_observers,
        CustomErrors::MaxObserversReached
    );

    let min_stake_lamports = ctx.accounts.registry.min_stake_lamports;

    // Ensure observer has enough lamports
    require!(
        ctx.accounts.observer.lamports() >= min_stake_lamports,
        CustomErrors::InsufficientLamports
    );

    // Transfer min_stake_lamports into the observer_account PDA as escrow
    if min_stake_lamports > 0 {
        let cpi_ctx = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.observer.to_account_info(),
                to: ctx.accounts.observer_account.to_account_info(),
            },
        );
        transfer(cpi_ctx, min_stake_lamports)?;
    }

    // Initialize all ObserverAccount fields
    let observer = &mut ctx.accounts.observer_account;
    observer.set_inner(ObserverAccount {
        authority: ctx.accounts.observer.key(),
        region,
        stake_lamports: min_stake_lamports,
        registered_at: Clock::get()?.unix_timestamp,
        last_attestation_slot: 0,
        attestation_count: 0,
        latest_attestation: Attestation::default(),
        is_active: true,
        bump: ctx.bumps.observer_account,
    });

    let registry = &mut ctx.accounts.registry;

    // Increment registry counts
    registry.observer_count = registry.observer_count.saturating_add(1);
    registry.active_count = registry.active_count.saturating_add(1);

    emit!(crate::events::ObserverRegistered {
        observer: ctx.accounts.observer.key(),
        region,
        stake_lamports: min_stake_lamports,
    });

    Ok(())
}
