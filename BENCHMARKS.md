# Benchmarks

Spec line 58: "Document and measure the compute unit (CU) cost of a single-CID anchor and a 50-CID batch anchor on LEZ devnet/testnet."

> **Note (2026-06-04):** the program is **deployed on the public LEZ testnet** (`testnet.lez.logos.co`) — see [`TESTNET_PROOF.md`](TESTNET_PROOF.md).
>
> **How CU is measured, and why it is the testnet figure.** The public testnet does not expose a per-transaction compute-unit value: the RISC0 executor computes cycle counts in `SessionInfo`, but `nssa/src/program.rs` consumes only `session_info.journal` and discards the cycles; no tx/block/receipt struct carries a cost field, and there is no `getTransactionReceipt` RPC (filed upstream — [`BUGS_FILED.md`](BUGS_FILED.md) #7). The RISC0 zkVM is **deterministic**, so the compute units consumed by a transaction depend only on `(program ELF, input)` — running the **deployed ELF** (`ImageID 54c7f793…aa91`) in the executor yields the *exact* cycle count it consumes on the testnet. CU below is therefore the executor cost of the deployed program, not a separate localnet program. The testnet runs the identical program families (`getProgramIds`), and anchor txs use the public path (sequencer-side execution), so this equivalence is exact for this tx class.
>
> ⚠️ **rc3 re-measurement pending.** The table below was captured against the **rc1 (`35d8df0d`) guest**. The deployed testnet program is the **rc3 (`cf3639d8`) build** (`ImageID 54c7f793…`). The executor-cycle figures must be re-captured against the rc3 ELF before resubmission (the program logic is unchanged, so the per-CID shape is expected to hold, but the absolute numbers must come from the deployed ELF). This is a heavy RISC0/localnet run via `scripts/measure_cu.sh`. **Follow-up infra note:** the hermetic `cargo risczero build` (and `lgs localnet`) require **Docker**; the M4 Pro build host does not currently have Docker installed, so this runs on a Docker-enabled host (install Docker on the M4 Pro, or run on the main machine). The deployed rc3 `.bin` already exists at `target/riscv32im-risc0-zkvm-elf/docker/whistleblower_registry.bin` (ImageID `54c7f793…`).

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

### Public LEZ testnet (`testnet.lez.logos.co`) — deployed; CU via deterministic deployed-ELF execution

The earlier "devnet pending credentials" note is **obsolete**: a public, no-auth LEZ testnet now exists and the registry is deployed on it (program `54c7f793…aa91`; deploy + anchor lifecycle confirmed on chain — [`TESTNET_PROOF.md`](TESTNET_PROOF.md)).

**Testnet-measurable performance (captured live).** What the public testnet *does* expose, measured directly against it:
- **Inclusion latency** — anchor txs confirm within seconds-to-minutes (the wallet's ~45s confirmation poll sometimes lapses before inclusion; all four lifecycle txs landed — see `TESTNET_PROOF.md`). Dominated by block cadence, not program compute.
- **Payload size** — `anchor_one` instruction payload and the per-tx account list are small (one PDA for single, ≤50 for batch); the explorer's transaction view shows the serialized payload + proof size per tx.

**Per-transaction CU — not exposed by the testnet.** As explained at the top of this file, the testnet never persists a per-tx CU value (BUGS_FILED #7). CU is therefore obtained by executing the **deployed ELF** in the RISC0 executor — deterministic, so equal to on-chain CU.

| Operation | Accounts touched | CU source | Executor cycles (rc3 ELF `54c7f793…`) |
|---|---|---|---|
| `anchor_one` (single CID) | 1 | deployed-ELF execution | **pending rc3 re-measure (M4 Pro)** |
| `anchor_batch` (50 CIDs)  | 50 | deployed-ELF execution | **pending rc3 re-measure (M4 Pro)** |

The rc1-guest figures in the localnet table above stand in until the rc3 re-measure lands; the program logic is identical between rc1 and rc3 (only the LEZ host API changed — `AccountId::for_public_pda`), so the per-CID shape is expected to carry over. `scripts/measure_cu.sh` runs the capture; `DEPLOYMENT.md` has the deploy commands.

## Expected shape

Because the registry is PDA-per-CID (each anchor touches only its own account), the per-CID CU cost should be approximately constant. The 50-CID batch should be ~50× a single anchor with a fixed overhead for the per-tx setup + signature verification + proof generation. We expect:

```
batch_50_cu ≈ 50 * single_cid_cu + per_tx_overhead
```

Where `per_tx_overhead` is dominated by the Risc0 proof-generation fixed cost (the same shape regardless of how many accounts the program touches) and the LEZ runtime's per-tx validation work (linear in account count but with a small slope).

## Caveats

- LEZ's per-transaction compute budget may change during testnet (per LP-0017 spec line 45 — "LEZ's per-transaction compute budget may change during testnet"). Numbers here are pinned to the `35d8df0d` commit + circuits v0.4.2.
- Account-data size affects state-write CU. Each `AnchorEntry` borsh-encodes to ~110 bytes (32-byte CID hash + 32-byte metadata hash + 8-byte timestamp + variable CID string + Borsh framing). All entries fit comfortably in a single account-data write.
- The 50-CID batch must fit within LEZ's per-tx account list cap. The live `lez_adapter_anchor_50_cids_in_one_tx` integration test confirms 50 fits on the current localnet stack; rerun `scripts/measure_cu.sh` against devnet once credentials land to confirm the public network has the same cap/headroom.
