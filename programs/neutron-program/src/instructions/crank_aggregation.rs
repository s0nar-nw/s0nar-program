use anchor_lang::prelude::*;

use crate::{
    error::NeutronErrors,
    utils::{
        compute_avg_reach_latency, compute_health_score, count_active_regions,
        recompute_global_score,
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
        constraint = !registry_account.paused @ NeutronErrors::RegistryPaused,
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
        });
    }

    // write snapshots to network_health
    let network_health = &mut ctx.accounts.network_health;

    for snap in snapshots.iter() {
        for rs in network_health.region_scores.iter_mut() {
            if rs.region == snap.region {
                rs.health_score = snap.score;
                rs.reachability_pct = snap.reachability_pct;
                rs.avg_rtt_us = snap.avg_rtt_us;
                rs.slot_latency_ms = snap.slot_latency_ms;
                rs.last_updated_slot = snap.attestation_slot;
                break;
            }
        }
    }

    // Recompute global aggregates
    let global_score = recompute_global_score(network_health, current_slot);
    let active_count = count_active_regions(network_health, current_slot);

    require!(active_count > 0, NeutronErrors::NoActiveObservers);

    let (avg_reach, avg_latency) = compute_avg_reach_latency(network_health, current_slot);

    network_health.health_score = global_score;
    network_health.tpu_reachability_pct = avg_reach;
    network_health.avg_slot_latency_ms = avg_latency;
    network_health.active_observer_count = active_count;
    network_health.last_updated_slot = current_slot;
    network_health.last_updated_ts = clock.unix_timestamp;

    if global_score < network_health.min_health_ever {
        network_health.min_health_ever = global_score;
    }
    if global_score > network_health.max_health_ever {
        network_health.max_health_ever = global_score;
    }

    msg!(
        "Crank: score={} reach={}% latency={}ms active={}/3 slot={}",
        global_score,
        avg_reach,
        avg_latency,
        active_count,
        current_slot,
    );

    Ok(())
}
