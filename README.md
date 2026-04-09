# s0nar-program

Anchor program for the s0nar decentralized network telemetry oracle on Solana.

Lightweight observer nodes measure real TPU reachability and slot propagation timing from multiple geographic vantage points and submit signed attestations on-chain. The program aggregates these into a single stake-weighted health score that any Solana program can read.

---

## What The Program Does

Three observer nodes (Asia, US, EU) each submit a measurement every ~10 seconds. The program stores the latest measurement per observer, computes a health score per region, and aggregates them into a global `NetworkHealthAccount` that serves as the oracle.

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
    └── deregister_observer.rs
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

### `initialize`

Parameters: `min_stake_lamports: u64`, `max_observers: u16`. Both must be non-zero. Creates `RegistryAccount` and `NetworkHealthAccount` PDAs. Can only be called once (accounts are `init`).

### `register_observer`

Parameters: `region: Region` (Asia | US | EU). Transfers `min_stake_lamports` from the observer wallet into the `ObserverAccount` PDA as escrow. Fails if the registry is paused or the observer cap is reached.

### `submit_attestation`

Parameters: `tpu_reachable: u16`, `tpu_probed: u16`, `avg_rtt_us: u32`, `p95_rtt_us: u32`, `slot_latency_ms: u32`. Writes the measurement to the observer's account and immediately recomputes the global `NetworkHealthAccount`. Requires an active, non-paused registry. Rejects attestations for the same slot twice.

### `crank_aggregation`

No parameters. Permissionless full recomputation from scratch. **Requires all `ObserverAccount` PDAs to be passed as `remaining_accounts`** — the instruction iterates over them, skips inactive or stale ones, and rebuilds the global score. Fails if no active observers are found after filtering.

### `deregister_observer`

No parameters. The caller must be either the observer themselves or the registry authority. Transfers escrowed stake back to the observer wallet, sets `is_active = false`, and decrements `registry.active_count`.

---

## Accounts

| Account                | Seeds                   | Size | Description                                            |
| ---------------------- | ----------------------- | ---- | ------------------------------------------------------ |
| `RegistryAccount`      | `[b"registry"]`         | 65B  | Global config — authority, stake params, observer cap  |
| `ObserverAccount`      | `[b"observer", pubkey]` | 128B | Per-observer state — region, stake, latest attestation |
| `NetworkHealthAccount` | `[b"network_health"]`   | 129B | Oracle — health score and per-region breakdown         |

**Embedded structs (packed inside accounts above, not standalone):**

| Struct        | Size | Lives In                                |
| ------------- | ---- | --------------------------------------- |
| `Attestation` | 32B  | `ObserverAccount.latest_attestation`    |
| `RegionScore` | 19B  | `NetworkHealthAccount.region_scores[3]` |

Total rent: ~0.005 SOL for a full 3-observer deployment on devnet.

### `RegistryAccount` fields

| Field                | Type     | Description                                        |
| -------------------- | -------- | -------------------------------------------------- |
| `authority`          | `Pubkey` | Admin key — can deregister any observer, pause     |
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
| `active_observer_count` | `u16`              | Number of non-stale regions contributing to the score |
| `last_updated_slot`     | `u64`              | Slot of the last update — check for staleness         |
| `last_updated_ts`       | `i64`              | Unix timestamp of the last update                     |
| `min_health_ever`       | `u8`               | Lowest score ever recorded                            |
| `max_health_ever`       | `u8`               | Highest score ever recorded                           |
| `total_attestations`    | `u64`              | Cumulative attestation count across all observers     |
| `region_scores`         | `[RegionScore; 3]` | Per-region breakdown (Asia, US, EU)                   |
| `bump`                  | `u8`               | PDA bump seed                                         |

### `RegionScore` fields

| Field               | Type  | Description                         |
| ------------------- | ----- | ----------------------------------- |
| `region`            | `u8`  | Region enum (Asia=0, US=1, EU=2)    |
| `health_score`      | `u8`  | Score for this region 0–100         |
| `reachability_pct`  | `u8`  | TPU reachability % from this region |
| `avg_rtt_us`        | `u32` | Average RTT in microseconds         |
| `slot_latency_ms`   | `u32` | Slot propagation latency (ms)       |
| `last_updated_slot` | `u64` | Slot when this region last reported |

---

## Admin Controls

The `authority` set during `initialize` has two admin capabilities:

- **Deregister any observer** — call `deregister_observer` with `caller = authority` to forcibly remove a misbehaving node and return its stake.
- **Pause the registry** — set `registry.paused = true` to halt `register_observer`, `submit_attestation`, and `crank_aggregation`. There is no on-chain instruction to flip the paused flag; it must be done via a direct account update with upgrade authority.

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
