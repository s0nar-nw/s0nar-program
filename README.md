# neutron-program

Anchor program for the Neutron decentralized network telemetry oracle on Solana.

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

## Accounts

| Account                | Seeds                   | Size | Description                                            |
| ---------------------- | ----------------------- | ---- | ------------------------------------------------------ |
| `RegistryAccount`      | `[b"registry"]`         | 64B  | Global config — authority, stake params, observer cap  |
| `ObserverAccount`      | `[b"observer", pubkey]` | 128B | Per-observer state — region, stake, latest attestation |
| `NetworkHealthAccount` | `[b"network_health"]`   | 128B | Oracle — health score and per-region breakdown         |

**Embedded structs (packed inside accounts above, not standalone):**

| Struct        | Size | Lives In                                |
| ------------- | ---- | --------------------------------------- |
| `Attestation` | 32B  | `ObserverAccount.latest_attestation`    |
| `RegionScore` | 19B  | `NetworkHealthAccount.region_scores[3]` |

Total rent: ~0.005 SOL for a full 3-observer deployment on devnet.

---

## Instructions

| Instruction           | Caller                      | What It Does                                                |
| --------------------- | --------------------------- | ----------------------------------------------------------- |
| `initialize`          | Deployer, once              | Creates `RegistryAccount` + `NetworkHealthAccount`          |
| `register_observer`   | Observer daemon             | Creates `ObserverAccount` PDA, escrows stake                |
| `submit_attestation`  | Observer daemon, every ~10s | Writes latest measurement, immediately updates oracle       |
| `crank_aggregation`   | Anyone, permissionless      | Full recomputation across all observers, decays stale nodes |
| `deregister_observer` | Observer or authority       | Returns escrowed stake, marks node inactive                 |

---

## Program Structure

```
programs/neutron-program/src/
├── lib.rs                      # entrypoint + instruction dispatch
├── error.rs                    # custom error codes
├── utils.rs                    # score computation helpers (shared across instructions)
├── state/
│   ├── mod.rs                  # all account structs + embedded types + seeds
├──constants.rs                 # STALE_SLOTS
└── instructions/
    ├── mod.rs
    ├── initialize.rs
    ├── register_observer.rs
    ├── submit_attestation.rs
    ├── crank_aggregation.rs
    └── deregister_observer.rs
```

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
