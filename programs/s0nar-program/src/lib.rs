pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod state;
pub mod tests;
pub mod utils;

use anchor_lang::prelude::*;

pub use constants::*;
pub use events::*;
pub use instructions::*;
pub use state::*;

declare_id!("9eqgnuLZP5vMnxU27vZVcrhoSkf3PhhVECRKbb8P8fNQ");

#[program]
pub mod s0nar_program {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        min_stake_lamports: u64,
        max_observers: u16,
    ) -> Result<()> {
        initialize::init(ctx, min_stake_lamports, max_observers)
    }

    pub fn register_observer(ctx: Context<RegisterObserver>, region: Region) -> Result<()> {
        register_observer::register(ctx, region)
    }

    pub fn submit_attestation(
        ctx: Context<SubmitAttestation>,
        tpu_reachable: u16,
        tpu_probed: u16,
        avg_rtt_us: u32,
        p95_rtt_us: u32,
        slot_latency_ms: u32,
        agave_count: u16,
        firedancer_count: u16,
        jito_count: u16,
        solana_labs_count: u16,
        other_count: u16,
        reachable_stake_pct: u8,
    ) -> Result<()> {
        submit_attestation::submit(
            ctx,
            tpu_reachable,
            tpu_probed,
            avg_rtt_us,
            p95_rtt_us,
            slot_latency_ms,
            agave_count,
            firedancer_count,
            jito_count,
            solana_labs_count,
            other_count,
            reachable_stake_pct,
        )
    }

    pub fn crank_aggregation<'a>(ctx: Context<'_, '_, '_, 'a, CrankAggregation<'a>>) -> Result<()> {
        crank_aggregation::crank(ctx)
    }

    pub fn deregister_observer(ctx: Context<DeregisterObserver>) -> Result<()> {
        deregister_observer::deregister(ctx)
    }

    pub fn slash_observer(ctx: Context<SlashObserver>, slash_bps: u16) -> Result<()> {
        slash_observer::slash(ctx, slash_bps)
    }

    pub fn update_config(
        ctx: Context<UpdateConfig>,
        min_stake_lamports: Option<u64>,
        max_observers: Option<u16>,
        paused: Option<bool>,
    ) -> Result<()> {
        update_config::update(ctx, min_stake_lamports, max_observers, paused)
    }

    pub fn propose_authority(ctx: Context<ProposeAuthority>, new_authority: Pubkey) -> Result<()> {
        transfer_authority::propose(ctx, new_authority)
    }

    pub fn accept_authority(ctx: Context<AcceptAuthority>) -> Result<()> {
        transfer_authority::accept(ctx)
    }
}

pub use crate::s0nar_program::*;
