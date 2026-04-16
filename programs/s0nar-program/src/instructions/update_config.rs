use crate::{error::CustomErrors, RegistryAccount, REGISTRY_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    // Authority of the protocol
    pub authority: Signer<'info>,

    // Global registry storing protocol configuration
    #[account(
        mut,
        seeds = [REGISTRY_SEED],
        bump = registry.bump,
        has_one = authority @ CustomErrors::UnAuthorizedCaller
    )]
    pub registry: Account<'info, RegistryAccount>,
}

/// Updates the sonar registry
///
/// This instruction:
/// - Optionally update min_stake_lamports field in registry
/// - Optionally update max_observers field in registry
/// - Optionally update paused status of the registry
pub fn update(
    ctx: Context<UpdateConfig>,
    min_stake_lamports: Option<u64>,
    max_observers: Option<u16>,
    paused: Option<bool>,
) -> Result<()> {
    let registry = &mut ctx.accounts.registry;

    if let Some(min_stake) = min_stake_lamports {
        require!(min_stake > 0, CustomErrors::ValueCannotBeZero);
        registry.min_stake_lamports = min_stake;
    }

    if let Some(max_obs) = max_observers {
        require!(
            max_obs >= registry.active_count,
            CustomErrors::MaxObserversCannotBeLessThanActiveObservers
        );
        require!(max_obs > 0, CustomErrors::ValueCannotBeZero);

        registry.max_observers = max_obs;
    }

    if let Some(is_paused) = paused {
        registry.paused = is_paused;
    }

    Ok(())
}
