use anchor_lang::prelude::*;

pub const REGISTRY_SEED: &[u8] = b"registry";
pub const OBSERVER_SEED: &[u8] = b"observer";
pub const NETWORK_HEALTH_SEED: &[u8] = b"network_health";

/// Global registry - tracks all observer nodes and program config
#[account]
pub struct RegistryAccount {
    /// Admin key
    pub authority: Pubkey,
    /// Pending authority for handoff
    pub pending_authority: Option<Pubkey>,
    /// Minimum stake required to observe
    pub min_stake_lamports: u64,
    /// Number of observers
    pub observer_count: u16,
    /// Currently active accounts
    pub active_count: u16,
    /// Maximum number of observers
    pub max_observers: u16,
    /// Paused flag
    pub paused: bool,
    /// Version of the registry
    pub version: u8,
    /// Bump seed for the PDA
    pub bump: u8,
}

impl RegistryAccount {
    pub const LEN: usize = 8   // discriminator
        + 32                    // authority
        + 33                    // pending_authority
        + 8                     // min_stake_lamports
        + 2                     // observer_count
        + 2                     // active_account
        + 2                     // max_observers
        + 1                     // paused
        + 1                     // version
        + 1                     // bump
        + 8; // padding
}

/// Single 10-second measurement from one observer node
#[derive(Default, AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attestation {
    /// Solana slot this measurement covers
    pub slot: u64,
    /// Timestamp of the measurement
    pub timestamp: i64,
    /// Average RTT of the QUIC probe
    pub avg_rtt_us: u32,
    /// P95 RTT of the QUIC probe
    pub p95_rtt_us: u32,
    /// Slot latency of the QUIC probe
    pub slot_latency_ms: u32,
    /// Validators reachable via QUIC probe
    pub tpu_reachable: u16,
    /// Total validators probed this round
    pub tpu_probed: u16,
    /// Client distributions
    pub agave_count: u16,
    pub firedancer_count: u16,
    pub jito_count: u16,
    pub solana_labs_count: u16,
    pub other_count: u16,
    /// % of total stake (by lamports) reachable via QUIC probe
    pub reachable_stake_pct: u8,
}

impl Attestation {
    pub const LEN: usize = 8    // slot
        + 8                     // timestamp
        + 4                     // avg_rtt_us
        + 4                     // p95_rtt_us
        + 4                     // slot_latency_ms
        + 2                     // tpu_reachable
        + 2                     // tpu_probed
        + 2                     // agave_count
        + 2                     // firedancer_count
        + 2                     // jito_count
        + 2                     // solana_labs_count
        + 2                     // other_count
        + 1; // reachable_stake_pct
}
/// Geographic region of an observer node - serializes as u8 on-chain
#[derive(Default, AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Region {
    #[default]
    Asia,
    US,
    EU,
    SouthAmerica,
    Africa,
    Oceania,
    Other,
}

/// Per-observer state - stores identity, region, stake and latest measurement
#[account]
pub struct ObserverAccount {
    /// The authority of the observer
    pub authority: Pubkey,
    /// The region of the observer
    pub region: Region,
    /// The stake of the observer
    pub stake_lamports: u64,
    /// The timestamp when the observer was registered
    pub registered_at: i64,
    /// Solana slot of the most recent attestation submitted
    /// Used for staleness check in crank_aggregation
    pub last_attestation_slot: u64,
    /// The number of attestations submitted by the observer
    pub attestation_count: u64,
    /// The latest attestation submitted by the observer
    pub latest_attestation: Attestation,
    /// Whether the observer is active
    pub is_active: bool,
    /// The bump seed for the PDA
    pub bump: u8,
}

impl ObserverAccount {
    pub const LEN: usize = 8           // discriminator
        + 32                            // authority
        + 1                             // region
        + 8                             // stake_lamports
        + 8                             // registered_at
        + 8                             // last_attestation_slot
        + 8                             // attestation_count
        + Attestation::LEN              // latest_attestation (43)
        + 1                             // is_active
        + 1                             // bump
        + 10; // padding
}

/// Health snapshot for one geographic region - embedded in NetworkHealthAccount
#[derive(Default, Debug, AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct RegionScore {
    /// Which region this entry represents
    pub region: Region,

    /// Number of observer contributions currently represented in this region aggregate
    pub observer_count: u16,

    /// Health score from this region
    pub health_score: u8,

    /// TPU reachability % from this region
    pub reachability_pct: u8,

    /// Average RTT from this region in microseconds
    pub avg_rtt_us: u32,

    /// Slot propagation latency from this region (ms)
    pub slot_latency_ms: u32,

    /// Slot when this region last reported
    pub last_updated_slot: u64,

    /// Running total of health scores for observers in this region
    pub total_health_score: u32,

    /// Running total of reachability percentages for observers in this region
    pub total_reachability_pct: u32,

    /// Running total of RTT values for observers in this region
    pub total_avg_rtt_us: u64,

    /// Running total of slot latency values for observers in this region
    pub total_slot_latency_ms: u64,

    /// Client distribution counts for this region
    pub agave_count: u16,
    pub firedancer_count: u16,
    pub jito_count: u16,
    pub solana_labs_count: u16,
    pub other_count: u16,

    /// Running totals
    pub total_agave_count: u32,
    pub total_firedancer_count: u32,
    pub total_jito_count: u32,
    pub total_solana_labs_count: u32,
    pub total_other_count: u32,

    /// Stake-weighted reachability for this region
    pub reachable_stake_pct: u8,
    pub total_reachable_stake_pct: u32,
}

impl RegionScore {
    pub const LEN: usize = 1     // region
        + 2                      // observer_count
        + 1                      // health_score
        + 1                      // reachability_pct
        + 4                      // avg_rtt_us
        + 4                      // slot_latency_ms
        + 8                      // last_updated_slot
        + 4                      // total_health_score
        + 4                      // total_reachability_pct
        + 8                      // total_avg_rtt_us
        + 8                      // total_slot_latency_ms
        + (2 * 5)                // agave/firedancer/jito/labs/other display (u16 × 5)
        + (4 * 5)                // totals (u32 × 5)
        + 1                      // reachable_stake_pct
        + 4; // total_reachable_stake_pct
}

/// Global oracle account - the single source of truth for dApps and UI reads
#[account]
pub struct NetworkHealthAccount {
    /// The health score of the network
    pub health_score: u8,
    /// TPU reachability % averaged across all regions
    pub tpu_reachability_pct: u8,
    /// Average slot latency in milliseconds
    pub avg_slot_latency_ms: u32,
    /// Number of active observers that contributed to this score
    pub active_observer_count: u16,
    /// Number of regions with fresh attestations
    pub active_region_count: u16,
    /// Slot of last aggregation — dApps check this for staleness
    pub last_updated_slot: u64,
    /// Unix timestamp of last update
    pub last_updated_ts: i64,

    /// Lowest health score ever recorded
    /// Note: Initialized to 255 (u8::MAX) which represents "no data yet".
    pub min_health_ever: u8,
    /// Highest health score ever recorded
    pub max_health_ever: u8,
    /// Total attestations ever submitted across all observers
    pub total_attestations: u64,

    /// One entry per region
    pub region_scores: [RegionScore; 7],

    // global client distribution across all active regions
    pub agave_count: u8,
    pub firedancer_count: u8,
    pub jito_count: u8,
    pub solana_labs_count: u8,
    pub other_count: u8,

    /// PDA bump seed
    pub bump: u8,
}

impl NetworkHealthAccount {
    pub const REGION_COUNT: usize = 7;

    pub const LEN: usize = 8            // discriminator
        + 1                             // health_score
        + 1                             // tpu_reachability_pct
        + 4                             // avg_slot_latency_ms
        + 2                             // active_observer_count
        + 2                             // active_region_count
        + 8                             // last_updated_slot
        + 8                             // last_updated_ts
        + 1                             // min_health_ever
        + 1                             // max_health_ever
        + 8                             // total_attestations
        + (Self::REGION_COUNT * RegionScore::LEN) // 7 × 75 = 525
        + 5                             // agave/firedancer/jito/labs/other pct (u8 × 5)
        + 1                             // bump
        + 22; // padding
}
