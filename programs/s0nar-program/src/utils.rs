use crate::{
    state::{NetworkHealthAccount, RegionScore},
    STALE_SLOTS,
};

/// Computes a single observer's health score (0-100).
/// Reachability carries more weight because a slow network is still
/// functional - but an unreachable TPU means transactions can't land at all.
/// So those 2 components weighted as 70/30:
///   - Reachability (70%): what % of validators accepted our QUIC probe
///   - Latency (30%):      how fast slot propagation is vs 400ms ceiling
pub fn compute_health_score(reachability_pct: u8, slot_latency_ms: u32) -> u8 {
    let reach_component = reachability_pct as u32 * 70 / 100;

    let latency_score = if slot_latency_ms >= 400 {
        0u32
    } else {
        (400 - slot_latency_ms) * 100 / 400
    };

    let latency_component = latency_score * 30 / 100;

    ((reach_component + latency_component).min(100)) as u8
}

/// Recomputes the global health score by averaging all non-stale region scores.
/// Stale = no update received in the last 150 slots (~60 seconds at 400ms/slot).
/// Stale regions are skipped entirely — their old score would misrepresent
/// current network conditions if that observer has gone offline.
/// Returns 0 if all regions are stale (no active observers).
pub fn recompute_global_score(health: &NetworkHealthAccount, current_slot: u64) -> u8 {
    let mut score_sum: u32 = 0;
    let mut count = 0u32;

    for rs in health.region_scores.iter() {
        if current_slot.saturating_sub(rs.last_updated_slot) <= STALE_SLOTS {
            score_sum += rs.health_score as u32;
            count += 1;
        }
    }

    if count == 0 {
        return 0;
    }

    (score_sum / count) as u8
}

/// Counts how many regions have submitted a fresh attestation recently.
/// Used to populate network_health.active_observer_count.
pub fn count_active_regions(health: &NetworkHealthAccount, current_slot: u64) -> u16 {
    health
        .region_scores
        .iter()
        .filter(|rs| {
            rs.observer_count > 0
                && current_slot.saturating_sub(rs.last_updated_slot) <= STALE_SLOTS
        })
        .count() as u16
}

/// Computes average TPU reachability % and slot latency across all active regions.
/// Same staleness filter as recompute_global_score — stale regions are excluded.
/// Returns a tuple: (reachability_pct: u8, avg_slot_latency_ms: u32)
pub fn compute_avg_reach_latency(health: &NetworkHealthAccount, current_slot: u64) -> (u8, u32) {
    let mut count = 0u32;
    let mut latency_sum = 0u32;
    let mut reach_sum = 0u32;

    for rs in health.region_scores.iter() {
        if current_slot.saturating_sub(rs.last_updated_slot) <= STALE_SLOTS {
            count += 1;
            latency_sum += rs.slot_latency_ms;
            reach_sum += rs.reachability_pct as u32;
        }
    }

    if count == 0 {
        (0, 0)
    } else {
        (
            reach_sum.checked_div(count).unwrap_or(0) as u8,
            latency_sum.checked_div(count).unwrap_or(0),
        )
    }
}

pub fn set_region_averages(region_score: &mut RegionScore) {
    if region_score.observer_count == 0 {
        region_score.health_score = 0;
        region_score.reachability_pct = 0;
        region_score.avg_rtt_us = 0;
        region_score.slot_latency_ms = 0;
        return;
    }

    let count = region_score.observer_count as u32;

    region_score.health_score = region_score
        .total_health_score
        .checked_div(count)
        .unwrap_or(0) as u8;
    region_score.reachability_pct = region_score
        .total_reachability_pct
        .checked_div(count)
        .unwrap_or(0) as u8;
    region_score.avg_rtt_us = region_score
        .total_avg_rtt_us
        .checked_div(count as u64)
        .unwrap_or(0) as u32;
    region_score.slot_latency_ms = region_score
        .total_slot_latency_ms
        .checked_div(count as u64)
        .unwrap_or(0) as u32;
}

pub fn clear_region_aggregate(region_score: &mut RegionScore) {
    region_score.observer_count = 0;
    region_score.total_health_score = 0;
    region_score.total_reachability_pct = 0;
    region_score.total_avg_rtt_us = 0;
    region_score.total_slot_latency_ms = 0;
    set_region_averages(region_score);
}
