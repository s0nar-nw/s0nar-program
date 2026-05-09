use anchor_lang::prelude::*;

use crate::{
    error::CustomErrors,
    utils::{
        clear_region_aggregate, compute_avg_client_diversity, compute_avg_reach_latency,
        compute_health_score, count_active_regions, recompute_global_score, set_region_averages,
    },
    NetworkHealthAccount, ObserverAccount, Region, RegistryAccount, NETWORK_HEALTH_SEED,
    REGISTRY_SEED, STALE_SLOTS,
};

#[derive(Accounts)]
pub struct CrankAggregation<'info> {
    // Anyone can call this instruction
    pub cranker: Signer<'info>,

    #[account(
        mut,
        seeds = [NETWORK_HEALTH_SEED],
        bump = network_health.bump,
    )]
    pub network_health: Account<'info, NetworkHealthAccount>,

    // Registry — checked for paused state, read only
    #[account(
        seeds = [REGISTRY_SEED],
        bump = registry_account.bump,
        constraint = !registry_account.paused @ CustomErrors::RegistryPaused,
    )]
    pub registry_account: Account<'info, RegistryAccount>,

    pub clock: Sysvar<'info, Clock>,
}

/// Permissionless full recomputation of NetworkHealthAccount.
/// Reads all ObserverAccounts from remaining_accounts,
/// skips stale ones, recomputes global score from scratch.
pub fn crank(ctx: Context<CrankAggregation>) -> Result<()> {
    let clock = &ctx.accounts.clock;
    let current_slot = clock.slot;

    // read all observer data into owned values
    struct ObserverSnapshot {
        region: Region,
        reachability_pct: u8,
        score: u8,
        avg_rtt_us: u32,
        slot_latency_ms: u32,
        attestation_slot: u64,
        /// Client distributions
        agave_count: u16,
        firedancer_count: u16,
        jito_count: u16,
        solana_labs_count: u16,
        other_count: u16,
        /// Stake-weighted reachability
        reachable_stake_pct: u8,
    }

    let mut snapshots: Vec<ObserverSnapshot> = Vec::new();

    for account_info in ctx.remaining_accounts.iter() {
        // Skip accounts not owned by this program
        if account_info.owner != ctx.program_id {
            continue;
        }

        let observer = {
            let data = account_info.try_borrow_data()?;
            match ObserverAccount::try_deserialize(&mut data.as_ref()) {
                Ok(o) => o,
                Err(_) => continue,
            }
        };

        // Skip inactive or stale observers
        if !observer.is_active {
            continue;
        }
        if current_slot.saturating_sub(observer.last_attestation_slot) > STALE_SLOTS {
            continue;
        }

        let att = &observer.latest_attestation;
        if att.tpu_probed == 0 {
            continue;
        }

        let reachability_pct = (att.tpu_reachable as u64 * 100 / att.tpu_probed as u64) as u8;

        snapshots.push(ObserverSnapshot {
            region: observer.region,
            reachability_pct,
            score: compute_health_score(reachability_pct, att.slot_latency_ms),
            avg_rtt_us: att.avg_rtt_us,
            slot_latency_ms: att.slot_latency_ms,
            attestation_slot: att.slot,
            agave_count: att.agave_count,
            firedancer_count: att.firedancer_count,
            jito_count: att.jito_count,
            solana_labs_count: att.solana_labs_count,
            other_count: att.other_count,
            reachable_stake_pct: att.reachable_stake_pct,
        });
    }

    // write snapshots to network_health
    let network_health = &mut ctx.accounts.network_health;

    for rs in network_health.region_scores.iter_mut() {
        let last_updated_slot = rs.last_updated_slot;
        clear_region_aggregate(rs);
        rs.last_updated_slot = last_updated_slot;
    }

    for snap in snapshots.iter() {
        for rs in network_health.region_scores.iter_mut() {
            if rs.region == snap.region {
                rs.observer_count = rs.observer_count.saturating_add(1);
                rs.total_health_score = rs.total_health_score.saturating_add(snap.score as u32);
                rs.total_reachability_pct = rs
                    .total_reachability_pct
                    .saturating_add(snap.reachability_pct as u32);
                rs.total_avg_rtt_us = rs.total_avg_rtt_us.saturating_add(snap.avg_rtt_us as u64);
                rs.total_slot_latency_ms = rs
                    .total_slot_latency_ms
                    .saturating_add(snap.slot_latency_ms as u64);
                rs.total_agave_count = rs.total_agave_count.saturating_add(snap.agave_count as u32);
                rs.total_firedancer_count = rs
                    .total_firedancer_count
                    .saturating_add(snap.firedancer_count as u32);
                rs.total_jito_count = rs.total_jito_count.saturating_add(snap.jito_count as u32);
                rs.total_solana_labs_count = rs
                    .total_solana_labs_count
                    .saturating_add(snap.solana_labs_count as u32);
                rs.total_other_count = rs.total_other_count.saturating_add(snap.other_count as u32);
                rs.total_reachable_stake_pct = rs
                    .total_reachable_stake_pct
                    .saturating_add(snap.reachable_stake_pct as u32);
                rs.last_updated_slot = rs.last_updated_slot.max(snap.attestation_slot);
                set_region_averages(rs);
                break;
            }
        }
    }

    // Recompute global aggregates
    let global_score = recompute_global_score(network_health, current_slot);
    let active_region_count = count_active_regions(network_health, current_slot);

    require!(active_region_count > 0, CustomErrors::NoActiveObservers);

    let (avg_reach, avg_latency) = compute_avg_reach_latency(network_health, current_slot);

    network_health.health_score = global_score;
    network_health.tpu_reachability_pct = avg_reach;
    network_health.avg_slot_latency_ms = avg_latency;
    network_health.active_region_count = active_region_count;
    network_health.active_observer_count = snapshots.len() as u16;
    network_health.last_updated_slot = current_slot;
    network_health.last_updated_ts = clock.unix_timestamp;

    // global client distribution
    let (agave_p, fd_p, jito_p, labs_p, other_p) =
        compute_avg_client_diversity(network_health, current_slot);
    network_health.agave_count = agave_p;
    network_health.firedancer_count = fd_p;
    network_health.jito_count = jito_p;
    network_health.solana_labs_count = labs_p;
    network_health.other_count = other_p;

    if global_score < network_health.min_health_ever {
        network_health.min_health_ever = global_score;
    }
    if global_score > network_health.max_health_ever {
        network_health.max_health_ever = global_score;
    }

    let stake_reach_avg = network_health
        .region_scores
        .iter()
        .filter(|rs| {
            rs.observer_count > 0
                && current_slot.saturating_sub(rs.last_updated_slot) <= STALE_SLOTS
        })
        .map(|rs| rs.reachable_stake_pct as u32)
        .sum::<u32>()
        .checked_div(active_region_count as u32)
        .unwrap_or(0) as u8;

    msg!(
        "Crank: score={} reach={}% latency={}ms stake_reach_avg={}% active_observers={} active_regions={} slot={}",
        global_score,
        avg_reach,
        avg_latency,
        stake_reach_avg,
        snapshots.len(),
        active_region_count,
        current_slot,
    );

    Ok(())
}
