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

**Status: TBD**. Numbers below are placeholders to be replaced once we run on devnet with `RISC0_DEV_MODE=0`. The local sequencer in dev mode skips proving so its execution-time numbers are not representative.

| Operation | Sequencer | Mode | Accounts touched | Wall time | Risc0 execution time | CU cost |
|---|---|---|---|---|---|---|
| `anchor_one` (single CID) | localnet | DEV | 1 | TBD | TBD | TBD |
| `anchor_batch` (10 CIDs) | localnet | DEV | 10 | TBD | TBD | TBD |
| `anchor_batch` (50 CIDs) | localnet | DEV | 50 | TBD | TBD | TBD |
| `anchor_one` (single CID) | devnet | PROD | 1 | TBD | TBD | TBD |
| `anchor_batch` (50 CIDs) | devnet | PROD | 50 | TBD | TBD | TBD |

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
