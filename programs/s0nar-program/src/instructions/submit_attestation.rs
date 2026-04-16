use anchor_lang::prelude::*;

use crate::{
    error::CustomErrors,
    utils::{
        clear_region_aggregate, compute_avg_reach_latency, compute_health_score,
        count_active_regions, recompute_global_score, set_region_averages,
    },
    Attestation, NetworkHealthAccount, ObserverAccount, RegistryAccount, MAX_RTT_US,
    MAX_SLOT_LATENCY_MS, MIN_PROBE_COUNT, NETWORK_HEALTH_SEED, OBSERVER_SEED, REGISTRY_SEED,
    STALE_SLOTS,
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
        has_one = authority @ CustomErrors::UnauthorizedObserver,
        constraint = observer_account.is_active @ CustomErrors::ObserverNotActive,
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
        constraint = !registry.paused @ CustomErrors::RegistryPaused,
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

    require!(
        tpu_probed >= MIN_PROBE_COUNT,
        CustomErrors::InsufficientValidatorsProbed
    );
    require!(
        tpu_reachable <= tpu_probed,
        CustomErrors::InvalidReachabilityCount
    );
    require!(
        clock.slot > ctx.accounts.observer_account.last_attestation_slot,
        CustomErrors::StaleAttestation
    );
    require!(avg_rtt_us <= MAX_RTT_US, CustomErrors::InvalidLatencyValue);
    require!(p95_rtt_us <= MAX_RTT_US, CustomErrors::InvalidLatencyValue);
    require!(
        slot_latency_ms <= MAX_SLOT_LATENCY_MS,
        CustomErrors::InvalidLatencyValue
    );

    let observer_account = &mut ctx.accounts.observer_account;
    let region = observer_account.region;
    let previous_attestation = observer_account.latest_attestation;
    let previous_attestation_slot = observer_account.last_attestation_slot;
    let had_previous_fresh_attestation = observer_account.attestation_count > 0
        && clock.slot.saturating_sub(previous_attestation_slot) <= STALE_SLOTS;

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
    observer_account.latest_attestation = attestation;
    observer_account.last_attestation_slot = clock.slot;
    observer_account.attestation_count = observer_account.attestation_count.saturating_add(1);

    // Compute this observer's score by using the helper functions
    let reachability_pct = (tpu_reachable as u64 * 100 / tpu_probed as u64) as u8;
    let observer_score = compute_health_score(reachability_pct, slot_latency_ms);
    let network_health = &mut ctx.accounts.network_health;

    let previous_reachability_pct = if previous_attestation.tpu_probed == 0 {
        0
    } else {
        (previous_attestation.tpu_reachable as u64 * 100 / previous_attestation.tpu_probed as u64)
            as u8
    };
    let previous_observer_score = compute_health_score(
        previous_reachability_pct,
        previous_attestation.slot_latency_ms,
    );

    // Find this observer's region entry and update the region aggregate
    for rs in network_health.region_scores.iter_mut() {
        if rs.region == region {
            if clock.slot.saturating_sub(rs.last_updated_slot) > STALE_SLOTS {
                clear_region_aggregate(rs);
            }

            if had_previous_fresh_attestation && rs.observer_count > 0 {
                rs.total_health_score = rs
                    .total_health_score
                    .saturating_sub(previous_observer_score as u32);
                rs.total_reachability_pct = rs
                    .total_reachability_pct
                    .saturating_sub(previous_reachability_pct as u32);
                rs.total_avg_rtt_us = rs
                    .total_avg_rtt_us
                    .saturating_sub(previous_attestation.avg_rtt_us as u64);
                rs.total_slot_latency_ms = rs
                    .total_slot_latency_ms
                    .saturating_sub(previous_attestation.slot_latency_ms as u64);
            } else {
                rs.observer_count = rs.observer_count.saturating_add(1);
            }

            rs.total_health_score = rs.total_health_score.saturating_add(observer_score as u32);
            rs.total_reachability_pct = rs
                .total_reachability_pct
                .saturating_add(reachability_pct as u32);
            rs.total_avg_rtt_us = rs.total_avg_rtt_us.saturating_add(avg_rtt_us as u64);
            rs.total_slot_latency_ms = rs
                .total_slot_latency_ms
                .saturating_add(slot_latency_ms as u64);
            rs.last_updated_slot = clock.slot;
            set_region_averages(rs);
            break;
        }
    }

    // Update the global health score
    let global_score = recompute_global_score(network_health, clock.slot);
    network_health.health_score = global_score;
    network_health.last_updated_slot = clock.slot;
    network_health.last_updated_ts = clock.unix_timestamp;
    network_health.total_attestations = network_health.total_attestations.saturating_add(1);
    network_health.active_region_count = count_active_regions(network_health, clock.slot);
    network_health.active_observer_count = network_health
        .region_scores
        .iter()
        .filter(|rs| clock.slot.saturating_sub(rs.last_updated_slot) <= STALE_SLOTS)
        .map(|rs| rs.observer_count as u32)
        .sum::<u32>() as u16; // submit path doesn't have access to all observers, so active_observer_count can't be computed accurately here. Sum observer_count across all fresh regions as a proxy

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

    emit!(crate::events::AttestationSubmitted {
        observer: ctx.accounts.authority.key(),
        region,
        score: observer_score,
        reachability_pct,
        slot_latency_ms,
        slot: clock.slot,
    });

    Ok(())
}
