use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";

pub const STALE_SLOTS: u64 = 150;

pub const MIN_PROBE_COUNT: u16 = 10;

pub const MAX_RTT_US: u32 = 10_000_000;
pub const MAX_SLOT_LATENCY_MS: u32 = 10_000;
