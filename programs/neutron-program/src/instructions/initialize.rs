use crate::{
    error::NeutronErrors, NetworkHealthAccount, Region, RegionScore, RegistryAccount,
    NETWORK_HEALTH_SEED, REGISTRY_SEED,
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = RegistryAccount::LEN,
        seeds = [REGISTRY_SEED],
        bump
    )]
    pub registry: Account<'info, RegistryAccount>,
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

pub fn init(ctx: Context<Initialize>, min_stake_lamports: u64, max_observers: u16) -> Result<()> {
    require!(min_stake_lamports > 0, NeutronErrors::ValueCannotBeZero);
    require!(max_observers > 0, NeutronErrors::ValueCannotBeZero);

    let registry = &mut ctx.accounts.registry;

    registry.authority = ctx.accounts.authority.key();
    registry.min_stake_lamports = min_stake_lamports;
    registry.max_observers = max_observers;
    registry.version = 1;
    registry.bump = ctx.bumps.registry;

    let network_health = &mut ctx.accounts.network_health;

    network_health.min_health_ever = u8::MAX;
    network_health.region_scores = [
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
    ];
    network_health.bump = ctx.bumps.network_health;

    Ok(())
}