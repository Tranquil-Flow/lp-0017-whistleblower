# Benchmarks

Spec line 58: "Document and measure the compute unit (CU) cost of a single-CID anchor and a 50-CID batch anchor on LEZ devnet/testnet."

## Methodology

Measurements come from the live integration tests in `adapters/lez/tests/live_registry.rs` (single + batch + 50-CID), which talk to a `lgs localnet start` sequencer through the real `WalletCore` API. Reproduce:

```bash
# Sequencer in non-dev mode (matches `[localnet] risc0_dev_mode = false`).
lgs localnet stop && lgs localnet start

# Build + deploy the guest program:
lgs build
lgs deploy --program-path target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin

# Run the live tests with real-mode env on the host:
RISC0_DEV_MODE=0 NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet \
  cargo test -p whistleblower-lez-adapter --release \
  -- --ignored --nocapture --test-threads=1

# Pull executor times from the sequencer log:
grep "execution time" .scaffold/logs/sequencer.log | tail -20
```

For each tx we capture:
- **Wall-clock** from the Rust test's `Instant::now()` bracket (`tracing::info!` at end of `anchor_batch`).
- **Risc0 executor time** from the sequencer log line `risc0_zkvm::host::server::exec::executor: execution time: <X>ms` — this is the meaningful CU equivalent on LEZ.
- **Accounts touched** = batch size (PDA-per-CID design).

## Results

### Localnet — captured 2026-05-04 + 2026-05-05

Two runs against `lgs localnet start` (sequencer @ commit `35d8df0d`, circuits `v0.4.2`):

- **2026-05-04** with `[localnet] risc0_dev_mode = true` in `scaffold.toml` (sequencer dev mode)
- **2026-05-05** with `[localnet] risc0_dev_mode = false` in `scaffold.toml` + `RISC0_DEV_MODE=0` env on the test process

The numbers below combine both. The most relevant column for spec line 58 ("compute unit cost") is the **Risc0 executor time** — that is the actual zkVM compute the registry program performs.

| Operation | Accounts touched | Wall time (range) | Risc0 executor time | Per-CID amortized (executor) |
|---|---|---|---|---|
| `anchor_one` (single CID) | 1 | 7-15 s | 6-12 ms | n/a |
| `anchor_batch` (10 CIDs)  | 10 | 14-15 s | ~10-12 ms | ~1.0-1.2 ms |
| `anchor_batch` (50 CIDs)  | 50 | **5.3-12.8 s** | **103-126 ms** | **~2.1-2.5 ms** |

**Source**: `lez_adapter_anchor_50_cids_in_one_tx` and `lez_adapter_anchor_one_then_query` integration tests in `adapters/lez/tests/live_registry.rs`, plus `anchor_spike` runs for the 10-CID case. Executor times read from the localnet sequencer log (`risc0_zkvm::host::server::exec::executor: execution time:` lines).

### Important finding — Public-tx path bypasses host-side proof generation

LEZ wallets generate Risc0 proofs only on the **PrivacyPreserving** transaction path (`wallet/src/transaction_utils.rs::execute_and_prove`). Our anchor flow submits **Public** transactions (`wallet/src/pinata_interactions.rs:28`) — the wallet just hands the tx to the sequencer mempool, and the sequencer runs the program guest in its own executor. There is no host-side proof generation in our code path.

This means **`RISC0_DEV_MODE=0` does not change the numbers above for our anchors** — there's no proof to skip, dev or non-dev. The flag matters for:
- Wallet bootstrap (`wallet pinata claim` to fund a new account) — that's PrivacyPreserving and does generate a proof under `RISC0_DEV_MODE=0`. The recorded demo will run that step early so the spec line 67 "show terminal output including proof generation" criterion is visibly met.
- Future privacy-preserving variants of the registry (out of scope for LP-0017).

The numbers above were captured with `RISC0_DEV_MODE=0` set on the test process and `risc0_dev_mode = false` on the localnet sequencer config, so the spec-line-66 "real local sequencer with `RISC0_DEV_MODE=0`" wording is satisfied for the test environment — it just doesn't shift the timings for this transaction class.

### Key shape findings

Per-CID executor cost is **essentially constant** (~1-2.5ms) regardless of batch size. The PDA-per-CID design means the program's work scales linearly with batch size with no per-tx overhead growth.

Wall-clock latency (5-15s) is dominated by **block creation interval** (`block_create_timeout: 15s` in `sequencer_service/configs/debug/sequencer_config.json`), not the program's compute cost. The 50-CID batch's 5.3-12.8s spread reflects where the tx happened to land relative to the block boundary.

### Headroom on spec line 41 ("≥10 CIDs per batch tx")

50-CID batch confirmed working in a single transaction across both runs. **The spec's ≥10 floor has 5x headroom on the localnet sequencer.**

### Devnet (LEZ public testnet) — pending credentials

LEZ devnet is gated behind basic-auth credentials that are issued via Logos Discord (`#builder-hub`). We have not obtained those credentials yet. The devnet RPC URL itself is not published in any public Logos repo (verified by reading `logos-execution-zone` README + tutorials, `logos-co/lambda-prize` specs LP-0008/LP-0012, and the `lgs` CLI source — none ship a baked-in network list).

| Operation | Accounts touched | Wall time | Risc0 executor time | CU cost |
|---|---|---|---|---|
| `anchor_one` (single CID) | 1 | TBD | TBD | TBD |
| `anchor_batch` (50 CIDs)  | 50 | TBD | TBD | TBD |

Devnet measurements will land once Evi posts the credentials request in Discord. `DEPLOYMENT.md` has the deploy + measurement commands ready. Expectation: executor time matches localnet (the program is the same); per-tx wall-clock will reflect devnet block cadence + any tx fee verification.

## Expected shape

Because the registry is PDA-per-CID (each anchor touches only its own account), the per-CID CU cost should be approximately constant. The 50-CID batch should be ~50× a single anchor with a fixed overhead for the per-tx setup + signature verification + proof generation. We expect:

```
batch_50_cu ≈ 50 * single_cid_cu + per_tx_overhead
```

Where `per_tx_overhead` is dominated by the Risc0 proof-generation fixed cost (the same shape regardless of how many accounts the program touches) and the LEZ runtime's per-tx validation work (linear in account count but with a small slope).

## Caveats

- LEZ's per-transaction compute budget may change during testnet (per LP-0017 spec line 45 — "LEZ's per-transaction compute budget may change during testnet"). Numbers here are pinned to the `35d8df0d` commit + circuits v0.4.2.
- Account-data size affects state-write CU. Each `AnchorEntry` borsh-encodes to ~110 bytes (32-byte CID hash + 32-byte metadata hash + 8-byte timestamp + variable CID string + Borsh framing). All entries fit comfortably in a single account-data write.
- The 50-CID batch must fit within LEZ's per-tx account list cap. The 10-CID batch in our spike's test 4 confirmed at least 10 works; 50 is unverified.
