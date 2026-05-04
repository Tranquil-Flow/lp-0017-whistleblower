# Benchmarks

Spec line 58: "Document and measure the compute unit (CU) cost of a single-CID anchor and a 50-CID batch anchor on LEZ devnet/testnet."

## Methodology

Measure both flows on the same sequencer build (RC1, commit `35d8df0d`), with `RISC0_DEV_MODE=0` (real proof generation), against a freshly-bootstrapped scaffold wallet.

For each measurement, capture:
- Wall-clock latency from `wallet send_transaction` return → tx confirmed in a block (via `get_transaction(hash)`).
- The sequencer's reported CU consumption (extract from the executor log line `risc0_zkvm::host::server::exec::executor: execution time: <X>ms` and from any explicit CU field in `wallet account get` post-tx, if exposed).
- Number of accounts touched per tx.

Scaffold for capturing these:

```bash
# Spike already prints per-tx hashes; add CU extraction:
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet ./target/release/anchor_spike 2>&1 | tee bench.log
grep "execution time" .scaffold/logs/sequencer.log | tail -20
```

For the 50-CID case, use anchor_spike's existing `make_cid` helper to produce 50 fresh CIDs and call `anchor_batch` once. (The 4th test currently does 10 — easy to bump to 50 for the benchmark recording.)

## Results

### Localnet (RISC0_DEV_MODE=true) — captured 2026-05-04

These numbers come from the local sequencer (`lgs localnet start`) which skips real proving. They isolate the registry program's per-tx compute cost from the proof-generation fixed cost. **Devnet numbers with `RISC0_DEV_MODE=0` are still TBD** and will be substantially higher (proof generation is the dominant term in production).

| Operation | Accounts touched | Wall time | Risc0 executor time | Per-CID amortized |
|---|---|---|---|---|
| `anchor_one` (single CID) | 1 | ~7-15 s | 6-7 ms | n/a |
| `anchor_batch` (10 CIDs)  | 10 | ~14-15 s | ~10-12 ms | ~1.0-1.2 ms |
| `anchor_batch` (50 CIDs)  | 50 | **11.39 s** | **52.9 ms** | **~1.06 ms** |

**Source**: `lez_adapter_anchor_50_cids_in_one_tx` integration test in `adapters/lez/tests/live_registry.rs` (the 50-CID measurement) plus `anchor_spike` runs (the smaller cases). All measurements against sequencer @ commit `35d8df0d` + circuits v0.4.2.

### Key finding

Per-CID compute cost is **essentially constant** at ~1ms regardless of batch size (1.06ms/CID at N=50, ~1.0-1.2ms at N=10). This validates the PDA-per-CID design choice — the program's work scales linearly with batch size, no per-tx overhead grows.

The wall-clock latency (~11-15s) is dominated by **block creation interval** (~15s on localnet config), not by the program's compute cost. Real production throughput will be limited by block cadence, not by registry program efficiency.

### Headroom on spec line 41 ("≥10 CIDs per batch tx")

50-CID batch confirmed working in a single transaction. **The spec's ≥10 floor has 5x headroom on the localnet sequencer.** Whether devnet enforces a tighter cap is TBD — needs a real-proof run.

### Devnet (RISC0_DEV_MODE=0) — TBD

| Operation | Accounts touched | Wall time | Risc0 executor time | CU cost |
|---|---|---|---|---|
| `anchor_one` (single CID) | 1 | TBD | TBD | TBD |
| `anchor_batch` (50 CIDs)  | 50 | TBD | TBD | TBD |

Devnet measurements pending the devnet RPC URL and a deployed program ID there. The proof-generation fixed cost dominates here — expect wall times in the minutes for real proofs.

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
