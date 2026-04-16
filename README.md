<p align="center">
  <img src="./s0nar.svg" alt="s0nar logo" width="400" />
</p>

<h1 align="center">s0nar-program</h1>

<p align="center">
Anchor program for the s0nar decentralized network telemetry oracle on Solana.
</p>
<p align="center">
  <a href="https://github.com/s0nar-nw/s0nar-program/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License" /></a>
  <img src="https://img.shields.io/badge/Solana-devnet-9945FF?logo=solana&logoColor=white" alt="Solana devnet" />
  <img src="https://img.shields.io/badge/Anchor-latest-blue" alt="Anchor" /> </br>
<a href="https://s0nar.online">Website</a>
    
</p>

## Overview

<p align="">
Lightweight observer nodes measure real TPU reachability and slot propagation timing from multiple geographic vantage points and submit signed attestations on-chain. The program aggregates these into a single health score that any Solana program can read.
</p>

---

## What The Program Does

Observers across supported regions submit measurements every ~10 seconds. The program stores the latest measurement per observer, computes a health score per region, and aggregates them into a global `NetworkHealthAccount` that serves as the oracle.

Health score formula:

```
observer_score   = (reachability_pct × 0.70) + (latency_score × 0.30)
latency_score    = max(0, (400 - slot_latency_ms) / 400 × 100)
global_score     = average of all non-stale region scores
```

A region is considered stale if no attestation has been received in the last 150 slots (~60 seconds). Stale regions are excluded from the global score automatically.

---

## Program Structure

```
programs/s0nar-program/src/
├── lib.rs                      # entrypoint + instruction dispatch
├── error.rs                    # custom error codes
├── events.rs                   # program events
├── utils.rs                    # score computation helpers (shared across instructions)
├── state/
│   ├── mod.rs                  # all account structs + embedded types + seeds
├── constants.rs                # STALE_SLOTS
└── instructions/
    ├── mod.rs
    ├── initialize.rs
    ├── register_observer.rs
    ├── submit_attestation.rs
    ├── crank_aggregation.rs
    ├── deregister_observer.rs
    ├── slash_observer.rs
    ├── update_config.rs
    └── transfer_authority.rs
```

---

## Instructions

| Instruction           | Caller                      | What It Does                                                |
| --------------------- | --------------------------- | ----------------------------------------------------------- |
| `initialize`          | Deployer, once              | Creates `RegistryAccount` + `NetworkHealthAccount`          |
| `register_observer`   | Observer daemon             | Creates `ObserverAccount` PDA, escrows stake                |
| `submit_attestation`  | Observer daemon, every ~10s | Writes latest measurement, immediately updates oracle       |
| `crank_aggregation`   | Anyone, permissionless      | Full recomputation across all observers, decays stale nodes |
| `deregister_observer` | Observer or authority       | Returns escrowed stake, marks node inactive                 |
| `slash_observer`      | Authority                   | Sends part of an observer's escrowed stake to a treasury    |
| `update_config`       | Authority                   | Updates min stake, observer cap, and paused state           |
| `propose_authority`   | Authority                   | Proposes a new authority for the registry                   |
| `accept_authority`    | Pending Authority           | Accepts the authority handoff                               |

### `initialize`

Parameters: `min_stake_lamports: u64`, `max_observers: u16`. Both must be non-zero. Creates `RegistryAccount` and `NetworkHealthAccount` PDAs. Can only be called once (accounts are `init`).

### `register_observer`

Parameters: `region: Region` (`Asia | US | EU | SouthAmerica | Africa | Oceania | Other`). Transfers `min_stake_lamports` from the observer wallet into the `ObserverAccount` PDA as escrow. Fails if the registry is paused or the observer cap is reached.

### `submit_attestation`

Parameters: `tpu_reachable: u16`, `tpu_probed: u16`, `avg_rtt_us: u32`, `p95_rtt_us: u32`, `slot_latency_ms: u32`. Writes the measurement to the observer's account and immediately recomputes the global `NetworkHealthAccount`. Requires an active, non-paused registry. Rejects attestations for the same slot twice.

### `crank_aggregation`

No parameters. Permissionless full recomputation from scratch. **Requires all `ObserverAccount` PDAs to be passed as `remaining_accounts`** — the instruction iterates over them, skips inactive or stale ones, and rebuilds the global score. Fails if no active observers are found after filtering.

### `deregister_observer`

No parameters. The caller must be either the observer themselves or the registry authority. Transfers escrowed stake back to the observer wallet, sets `is_active = false`, and decrements `registry.active_count`.

### `slash_observer`

Parameters: `slash_bps: u16`. Authority-only instruction that transfers a percentage of an observer's escrowed stake from the observer PDA to a treasury account and reduces `observer_account.stake_lamports` by the same amount.

### `update_config`

Parameters: `min_stake_lamports: Option<u64>`, `max_observers: Option<u16>`, `paused: Option<bool>`. Authority-only instruction for updating registry configuration, including pause/unpause.

### `propose_authority`

Parameters: `new_authority: Pubkey`. Authority-only instruction that sets the `pending_authority` field in the registry to prepare for a two-step authority handoff.

### `accept_authority`

No parameters. The caller must be the `pending_authority`. Completes the handoff by updating the registry's `authority` and clearing `pending_authority`.

---

## Accounts

| Account                | Seeds                   | Size | Description                                            |
| ---------------------- | ----------------------- | ---- | ------------------------------------------------------ |
| `RegistryAccount`      | `[b"registry"]`         | 65B  | Global config — authority, stake params, observer cap  |
| `ObserverAccount`      | `[b"observer", pubkey]` | 128B | Per-observer state — region, stake, latest attestation |
| `NetworkHealthAccount` | `[b"network_health"]`   | 205B | Oracle — health score and per-region breakdown         |

**Embedded structs (packed inside accounts above, not standalone):**

| Struct        | Size | Lives In                                |
| ------------- | ---- | --------------------------------------- |
| `Attestation` | 32B  | `ObserverAccount.latest_attestation`    |
| `RegionScore` | 19B  | `NetworkHealthAccount.region_scores[7]` |

Total rent depends on observer count and current account sizes.

### `RegistryAccount` fields

| Field                | Type     | Description                                        |
| -------------------- | -------- | -------------------------------------------------- |
| `authority`          | `Pubkey` | Admin key — can deregister any observer, pause     |
| `pending_authority`  | `Option<Pubkey>`| Pending authority for two-step handoff      |
| `min_stake_lamports` | `u64`    | Minimum stake required to register                 |
| `observer_count`     | `u16`    | Total observers ever registered (never decrements) |
| `active_count`       | `u16`    | Currently active observer count                    |
| `max_observers`      | `u16`    | Hard cap on concurrent observers                   |
| `paused`             | `bool`   | When true, blocks register/submit/crank            |
| `version`            | `u8`     | Schema version (currently 1)                       |
| `bump`               | `u8`     | PDA bump seed                                      |

### `NetworkHealthAccount` fields

| Field                   | Type               | Description                                           |
| ----------------------- | ------------------ | ----------------------------------------------------- |
| `health_score`          | `u8`               | Global health score 0–100                             |
| `tpu_reachability_pct`  | `u8`               | Average TPU reachability % across active regions      |
| `avg_slot_latency_ms`   | `u32`              | Average slot latency across active regions (ms)       |
| `active_observer_count` | `u16`              | Number of active observers that contributed to this score |
| `active_region_count`   | `u16`              | Number of non-stale regions contributing to the score |
| `last_updated_slot`     | `u64`              | Slot of the last update — check for staleness         |
| `last_updated_ts`       | `i64`              | Unix timestamp of the last update                     |
| `min_health_ever`       | `u8`               | Lowest score ever recorded                            |
| `max_health_ever`       | `u8`               | Highest score ever recorded                           |
| `total_attestations`    | `u64`              | Cumulative attestation count across all observers     |
| `region_scores`         | `[RegionScore; 7]` | Per-region breakdown across all supported regions     |
| `bump`                  | `u8`               | PDA bump seed                                         |

### `RegionScore` fields

| Field               | Type  | Description                         |
| ------------------- | ----- | ----------------------------------- |
| `region`            | `u8`  | Region enum (`Asia`, `US`, `EU`, `SouthAmerica`, `Africa`, `Oceania`, `Other`) |
| `health_score`      | `u8`  | Score for this region 0–100         |
| `reachability_pct`  | `u8`  | TPU reachability % from this region |
| `avg_rtt_us`        | `u32` | Average RTT in microseconds         |
| `slot_latency_ms`   | `u32` | Slot propagation latency (ms)       |
| `last_updated_slot` | `u64` | Slot when this region last reported |

---

## Admin Controls

The `authority` set during `initialize` has these admin capabilities:

- **Deregister any observer** — call `deregister_observer` with `caller = authority` to forcibly remove a misbehaving node and return its stake.
- **Pause or unpause the registry** — call `update_config(..., paused = Some(true | false))` to halt or resume `register_observer`, `submit_attestation`, and `crank_aggregation`.
- **Slash an observer** — call `slash_observer` to move a percentage of escrowed stake to a treasury account.
- **Transfer authority** — perform a two-step handoff using `propose_authority` (called by current authority) and `accept_authority` (called by the new authority).

---

## Events

The program emits the following events for indexing and off-chain monitoring:

- `ObserverRegistered`: Emitted when an observer successfully registers and escrows stake.
- `ObserverDeregistered`: Emitted when an observer leaves or is forcibly removed.
- `ObserverSlashed`: Emitted when an observer's stake is slashed by the authority.
- `AttestationSubmitted`: Emitted during `submit_attestation` with the observer's measurement, health score, and slot.
- `ConfigUpdated`: Emitted when the registry configuration is updated by the authority.

---

## Development

```bash
# Build
anchor build

# Get program ID after first build
anchor keys list

# Run tests
anchor test
```
