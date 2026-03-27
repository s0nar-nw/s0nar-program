use anchor_lang::prelude::*;

use crate::{
    error::NeutronErrors,
    utils::{
        compute_avg_reach_latency, compute_health_score, count_active_regions,
        recompute_global_score,
    },
    Attestation, NetworkHealthAccount, ObserverAccount, RegistryAccount, NETWORK_HEALTH_SEED,
    OBSERVER_SEED, REGISTRY_SEED,
};

#[derive(Accounts)]
pub struct SubmitAttestation<'info> {
    // Observer signing the attestation
    #[account(mut)]
    pub authority: Signer<'info>,

    // Observer state - get's updated after the latest attestation is submitted
    #[account(
        mut,
        seeds = [OBSERVER_SEED, authority.key().as_ref()],
        bump = observer_account.bump,
        has_one = authority @ NeutronErrors::UnauthorizedObserver,
        constraint = observer_account.is_active @ NeutronErrors::ObserverNotActive,
    )]
    pub observer_account: Account<'info, ObserverAccount>,

    // Global oracle account - updates immediately after attestation submission
    #[account(
        mut,
        seeds = [NETWORK_HEALTH_SEED],
        bump = network_health.bump,
    )]
    pub network_health: Account<'info, NetworkHealthAccount>,

    // Registry account - read only account for paused state
    #[account(
        seeds = [REGISTRY_SEED],
        bump = registry.bump,
        constraint = !registry.paused @ NeutronErrors::RegistryPaused,
    )]
    pub registry: Account<'info, RegistryAccount>,

    pub clock: Sysvar<'info, Clock>,
}

/// Submits a 10-second measurement from an observer node.
/// Writes to observer_account and immediately updates network_health.
pub fn submit(
    ctx: Context<SubmitAttestation>,
    tpu_reachable: u16,
    tpu_probed: u16,
    avg_rtt_us: u32,
    p95_rtt_us: u32,
    slot_latency_ms: u32,
) -> Result<()> {
    let clock = &ctx.accounts.clock;

    require!(tpu_probed > 0, NeutronErrors::ZeroValidatorsProbed);
    require!(
        tpu_reachable <= tpu_probed,
        NeutronErrors::InvalidReachabilityCount
    );
    require!(
        clock.slot > ctx.accounts.observer_account.last_attestation_slot,
        NeutronErrors::StaleAttestation
    );

    // Build the attestation
    let attestation = Attestation {
        slot: clock.slot,
        timestamp: clock.unix_timestamp,
        avg_rtt_us,
        p95_rtt_us,
        slot_latency_ms,
        tpu_reachable,
        tpu_probed,
    };

    // Update the observer account
    let observer_account = &mut ctx.accounts.observer_account;
    observer_account.latest_attestation = attestation;
    observer_account.last_attestation_slot = clock.slot;
    observer_account.attestation_count += 1;

    // Compute this observer's score by using the helper functions
    let reachability_pct = (tpu_reachable as u64 / tpu_probed as u64) as u8;
    let observer_score = compute_health_score(reachability_pct, slot_latency_ms);
    let region = observer_account.region;

    let network_health = &mut ctx.accounts.network_health;

    // Find this observer's region entry and update it
    for rs in network_health.region_scores.iter_mut() {
        if rs.region == region {
            rs.health_score = observer_score;
            rs.avg_rtt_us = avg_rtt_us;
            rs.slot_latency_ms = slot_latency_ms;
            rs.reachability_pct = reachability_pct;
            rs.last_updated_slot = clock.slot;
            break;
        }
    }

    // Update the global health score
    let global_score = recompute_global_score(network_health, clock.slot);
    network_health.health_score = global_score;
    network_health.last_updated_slot = clock.slot;
    network_health.last_updated_ts = clock.unix_timestamp;
    network_health.total_attestations += 1;
    network_health.active_observer_count = count_active_regions(network_health, clock.slot);

    // These are all time records
    if network_health.min_health_ever > global_score {
        network_health.min_health_ever = global_score;
    }
    if network_health.max_health_ever < global_score {
        network_health.max_health_ever = global_score;
    }

    // Update reachability and latency averages from active regions
    let (avg_reach, avg_latency) = compute_avg_reach_latency(network_health, clock.slot);
    network_health.tpu_reachability_pct = avg_reach;
    network_health.avg_slot_latency_ms = avg_latency;

    msg!(
        "Attestation submitted: region={:?} score={} reachability={}% latency={}ms slot={}",
        region,
        observer_score,
        reachability_pct,
        slot_latency_ms,
        clock.slot,
    );

    Ok(())
}
