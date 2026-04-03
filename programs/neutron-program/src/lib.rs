pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;
pub mod tests;
pub mod utils;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("Au4AWwhGvJFpxgJh3Qe83V8Z4emdd3CoE7EVoSiR5P5L");

#[program]
pub mod neutron_program {
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
    ) -> Result<()> {
        submit_attestation::submit(
            ctx,
            tpu_reachable,
            tpu_probed,
            avg_rtt_us,
            p95_rtt_us,
            slot_latency_ms,
        )
    }

    pub fn crank_aggregation<'a>(ctx: Context<'_, '_, '_, 'a, CrankAggregation<'a>>) -> Result<()> {
        crank_aggregation::crank(ctx)
    }

    pub fn deregister_observer(ctx: Context<DeregisterObserver>) -> Result<()> {
        deregister_observer::deregister(ctx)
    }

    pub fn update_config(
        ctx: Context<UpdateConfig>,
        min_stake_lamports: Option<u64>,
        max_observers: Option<u16>,
        paused: Option<bool>,
    ) -> Result<()> {
        update_config::update(ctx, min_stake_lamports, max_observers, paused)
    }
}
