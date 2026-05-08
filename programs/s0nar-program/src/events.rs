use crate::state::Region;
use anchor_lang::prelude::*;

#[event]
pub struct AttestationSubmitted {
    pub observer: Pubkey,
    pub region: Region,
    pub score: u8,
    pub reachability_pct: u8,
    pub slot_latency_ms: u32,
    pub slot: u64,
    // v0.2.0
    pub agave_count: u16,
    pub firedancer_count: u16,
    pub jito_count: u16,
    pub solana_labs_count: u16,
    pub other_count: u16,
}

#[event]
pub struct ObserverRegistered {
    pub observer: Pubkey,
    pub region: Region,
    pub stake_lamports: u64,
}

#[event]
pub struct ObserverDeregistered {
    pub observer: Pubkey,
}

#[event]
pub struct ObserverSlashed {
    pub observer: Pubkey,
    pub slash_bps: u16,
    pub amount_slashed: u64,
}

#[event]
pub struct ConfigUpdated {
    pub min_stake_lamports: Option<u64>,
    pub max_observers: Option<u16>,
    pub paused: Option<bool>,
}
