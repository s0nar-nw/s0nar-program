use crate::{
    error::CustomErrors, NetworkHealthAccount, Region, RegionScore, RegistryAccount,
    NETWORK_HEALTH_SEED, REGISTRY_SEED,
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize<'info> {
    // Authority initializing the protocol
    #[account(mut)]
    pub authority: Signer<'info>,

    // Global registry storing protocol configuration
    #[account(
        init,
        payer = authority,
        space = RegistryAccount::LEN,
        seeds = [REGISTRY_SEED],
        bump
    )]
    pub registry: Account<'info, RegistryAccount>,

    // Network-wide health metrics and statistics
    #[account(
        init,
        payer = authority,
        space = NetworkHealthAccount::LEN,
        seeds = [NETWORK_HEALTH_SEED],
        bump
    )]
    pub network_health: Account<'info, NetworkHealthAccount>,
    pub system_program: Program<'info, System>,
}

/// Initializes the Neutron registry and network health accounts.
///
/// This instruction:
/// - Creates RegistryAccount
/// - Creates NetworkHealthAccount
/// - Sets initial config parameters
pub fn init(ctx: Context<Initialize>, min_stake_lamports: u64, max_observers: u16) -> Result<()> {
    // Ensure protocol parameters are valid
    require!(min_stake_lamports > 0, CustomErrors::ValueCannotBeZero);
    require!(max_observers > 0, CustomErrors::ValueCannotBeZero);

    let registry = &mut ctx.accounts.registry;

    registry.set_inner(RegistryAccount {
        authority: ctx.accounts.authority.key(),
        min_stake_lamports,
        observer_count: 0,
        active_count: 0,
        max_observers,
        paused: false,
        version: 1,
        bump: ctx.bumps.registry,
    });

    let network_health = &mut ctx.accounts.network_health;

    network_health.set_inner(NetworkHealthAccount {
        health_score: 0,
        tpu_reachability_pct: 0,
        avg_slot_latency_ms: 0,
        active_observer_count: 0,
        last_updated_slot: 0,
        last_updated_ts: 0,
        // Initialize with worst possible value so first update always lowers it
        min_health_ever: u8::MAX,
        max_health_ever: 0,
        total_attestations: 0,
        region_scores: [
            RegionScore {
                region: Region::Asia,
                ..Default::default()
            },
            RegionScore {
                region: Region::US,
                ..Default::default()
            },
            RegionScore {
                region: Region::EU,
                ..Default::default()
            },
        ],
        bump: ctx.bumps.network_health,
    });

    Ok(())
}
