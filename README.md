# Whistleblower

LP-0017 reference implementation: a censorship-resistant document upload + indexing system on the Logos stack. Published under MIT or Apache-2.0 at the user's option.

Anyone can upload a document → its CID is broadcast over Logos Delivery → any altruistic third party (or the publisher themselves) can later batch-anchor accumulated CIDs to a LEZ program. The on-chain registry stores `(CID, metadata_hash, anchor_timestamp)` per document and is queryable by CID hash without a transaction.

> **Status:** the registry program is **deployed and exercised on the public LEZ testnet** (`testnet.lez.logos.co`, real consensus, `RISC0_DEV_MODE=0`, 2026-06-03) — program `54c7f793…aa91`; deploy + `anchor_one` + idempotent re-anchor + `anchor_batch` all confirm on chain (`wallet chain-info` → `Some(ProgramDeployment)`/`Some(Public)`), and both entry PDAs decode to the expected `AnchorEntry`. Full hashes, decodes, and reproduction are in [`TESTNET_PROOF.md`](TESTNET_PROOF.md); re-verify live with `bash scripts/verify-testnet.sh`. The repository also includes the reusable indexing module, LEZ adapter, permissionless batch-anchor CLI, SPEL IDL, and a Qt6/QML Basecamp UI plugin packaged into a portable `.lgx` via nix and smoke-tested in real Basecamp on 2026-05-09. Earlier local-sequencer runs (`REGISTRY_SPIKE.md`, `BENCHMARKS.md`) provide compute and development traces; public testnet verification is the primary deployment evidence. Repo: `https://github.com/Tranquil-Flow/lp-0017-whistleblower`.

## Repository layout

```
whistleblower/
├── methods/                  # Risc0 zkVM build wrapper
│   ├── build.rs              # risc0_build::embed_methods()
│   ├── src/lib.rs            # re-exports the compiled WHISTLEBLOWER_REGISTRY_ELF
│   └── guest/
│       └── src/bin/
│           └── whistleblower_registry.rs    # the LEZ program (PDA-per-CID)
├── core/                     # Shared types: CanonicalCid, CidHash, MetadataHash,
│                             # AnchorEntry, RegistryInstruction, MetadataEnvelopeV1
├── indexing/                 # Qt-free reusable orchestration
│   ├── src/traits.rs         # StorageClient / DeliveryClient / RegistryClient
│   ├── src/publisher.rs      # upload → broadcast → anchor pipeline
│   ├── src/batch.rs          # permissionless batch-anchor engine
│   ├── src/retry.rs          # exponential backoff for adapter calls
│   ├── src/orchestration.rs  # DurableDedupeStore (subscriber-side)
│   └── tests/                # adapter contract + publisher e2e + batch loop tests
├── adapters/
│   ├── mock/                 # in-memory adapters for unit tests
│   └── lez/                  # real LEZ-backed RegistryClient (the on-chain side)
├── batch/                    # `whistleblower-batch` CLI binary (permissionless batch anchor tool)
├── ui/                       # Qt6/QML Basecamp plugin (manifest + qml + src + ffi cdylib)
├── anchor_spike/             # standalone runner that proves Task 1.0B end-to-end
├── flake.nix                 # workspace-root nix flake (.#ffi / .#plugin / .#lgx / .#install)
├── dist/                     # built .lgx package (after `nix build .#lgx`)
├── whistleblower-registry.idl.json   # SPEL-generated IDL (spel generate-idl; regen via scripts/regen-idl.sh)
├── idl/whistleblower_registry.rs      # parse-only #[lez_program] mirror spel generate-idl reads (never compiled)
├── TESTNET_PROOF.md          # PRIMARY EVIDENCE — public-testnet deploy + lifecycle, hashes, decodes
├── anchor_spike/src/bin/testnet_lifecycle.rs  # typed deploy + anchor-lifecycle driver (captures hashes)
├── scripts/verify-testnet.sh # reviewer re-verification (read-only; decodes the entry PDAs via wallet)
├── scripts/ci-verify-testnet.sh # on-push CI check (raw JSON-RPC; no wallet/secrets)
├── REGISTRY_SPIKE.md         # local-sequencer spike (historical corroboration)
├── BENCHMARKS.md             # CU profile (localnet-sourced; testnet hides executor logs)
├── DEPLOYMENT.md             # deployment commands (local + public testnet)
├── DEMO.md                   # End-to-end demo script for the submission video
└── BUGS_FILED.md             # Upstream Logos issues filed during the build
```

## Quick start (public testnet — clone and run)

The fastest way to confirm the deployed registry is real. Re-verifies the full deploy → anchor → idempotent re-anchor → batch lifecycle on the public LEZ testnet, read-only — no build, no faucet, no localnet, no mock:

```bash
# Curl-only (no toolchain): confirms every deployed tx is live on the public sequencer.
bash scripts/demo.sh           # or: bash scripts/ci-verify-testnet.sh

# Richer check (decodes the entry PDAs) — needs the `wallet` binary (LEZ tag v0.1.2):
bash scripts/verify-testnet.sh
```

Program `54c7f793…aa91` on `https://testnet.lez.logos.co/` — full hashes + decodes in [`TESTNET_PROOF.md`](TESTNET_PROOF.md). To run a fresh deploy + lifecycle or the real batch tool against the testnet, see `scripts/demo.sh --full` / `--batch` (needs a funded testnet wallet).

## Quick start (local sequencer)

You need: macOS arm64 with `cargo`, `docker`, and the Logos toolchain. Install order matters — `logos-blockchain-circuits` is a build-time dependency of `spel`, so install it first. Verified versions: `lgs 0.1.1`, `spel 0.2.0`, `cargo-risczero 3.0.5`, `r0vm 3.0.5`, `rzup 0.5.1`, `logos-blockchain-circuits v0.4.2`. (For the public testnet, build the `wallet` from LEZ tag `v0.1.2` — see [`TESTNET_PROOF.md`](TESTNET_PROOF.md).)

```bash
# 1. Bring up a local LEZ sequencer (~3 min on warm cache).
lgs localnet start

# 2. Build the guest registry program (~6-15 min first time, ~1 min incremental).
lgs build

# 3. Deploy it to the running sequencer.
lgs deploy --program-path \
  target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin

# 4. Inspect the resulting program ID.
spel inspect \
  target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin

# 5. Run the spike to prove the registry is up + idempotent.
cargo build -p anchor-spike --release
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet ./target/release/anchor_spike

# 6. Stop the sequencer when you're done.
lgs localnet stop
```

The spike anchors fresh CIDs on every run — see `REGISTRY_SPIKE.md` for the four behaviours it proves.

## Run the test suite

```bash
# Fast unit + contract + e2e tests (excludes guest build + live integration).
cargo test --workspace \
  --exclude whistleblower-methods --exclude whistleblower-programs --exclude anchor-spike \
  --release

# Live LEZ integration tests (require sequencer + deployed program + wallet env).
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet \
  cargo test -p whistleblower-lez-adapter --release -- --ignored --nocapture
```

Expected: 18 fast tests pass (5 adapter contract + 3 batch loop + 2 publisher e2e + 5 retry lib + 3 core), plus 2 live integration tests pass (~30s). The Rust FFI cdylib in `ui/ffi/` has 4 additional unit tests — run via `(cd ui/ffi && cargo test --release)`.

## Build the Basecamp UI plugin

The plugin is a Qt6/QML widget that loads inside Basecamp, exposes the file picker + metadata form + 4-stage publish progress UI, bridges to required Logos Storage via `LogosAPIClient`, can best-effort broadcast through Logos Delivery when `WHISTLEBLOWER_ENABLE_DELIVERY=1`, and anchors through the Rust FFI cdylib. Build via the workspace-root nix flake on a machine with the Logos toolchain installed:

```bash
nix build .#ffi      # Rust cdylib (~3-4 min)
nix build .#plugin   # Qt6 plugin + standalone preview app
nix build .#lgx      # portable .lgx package — the spec deliverable
nix run  .#install   # copies plugin into Basecamp dev plugin dir
lgs basecamp install # evaluator path: installs storage_module + whistleblower; delivery_module may also be present as an optional module
scripts/fix_delivery_rln.sh # optional workaround when enabling delivery_module on affected macOS installs
```

The `.lgx` file is the spec deliverable. Verified end-to-end on m4pro (aarch64-darwin) on 2026-05-04: `dist/whistleblower-plugin.lgx` (2.4MB). See [`ui/README.md`](ui/README.md) for the manual `cmake -B build` development workflow.

## Use the batch anchor CLI

The CLI is the spec's "permissionless batch anchor tool" (line 33). It owns the on-chain side end-to-end: dedupe ledger, batching window, retry, idempotent anchor against the deployed program. The CID source is the `DeliveryClient` trait, so the same binary works against several transports:

- **`--envelopes-from <file>` (real, headless).** Replays the exact `MetadataEnvelopeV1` records the Delivery topic carries (newline-delimited JSON) through the real dedupe + batch + on-chain anchor pipeline — no mock, no Qt/Waku dependency. This is what the reproducible demo and CI use, and a legitimate operating mode for anyone who already holds a list of broadcast envelopes. Point `--program-bin` at the **deployed** program `.bin` so PDAs match the on-chain program id (a docker `cargo risczero build` and the in-process `embed_methods` build can produce different ImageIDs).
- **Live Logos Delivery (Waku) subscription** — the production transport. Real Delivery is Waku + RLN behind the Logos Core `delivery_module`, reachable as a Qt `logos_host` process over QtRemoteObjects. The Basecamp plugin keeps this path opt-in (`WHISTLEBLOWER_ENABLE_DELIVERY=1`) so upload and on-chain anchoring remain stable even if Delivery startup is unavailable. A headless Rust equivalent (QtRemoteObjects client, or RLN-membership management against Waku directly) is a separate integration — options + tradeoffs in [`adapters/logos/README.md`](adapters/logos/README.md).
- **`--mock-delivery`** — in-memory dev client only.

```bash
# Real, headless anchor against the deployed public-testnet program:
NSSA_WALLET_HOME_DIR=$WALLET_HOME \
  ./target/release/whistleblower-batch \
    --topic /lp0017-whistleblower/1/cids/json \
    --batch-size 10 \
    --batch-interval-secs 30 \
    --dedupe-store-path queue.db \
    --program-bin target/riscv32im-risc0-zkvm-elf/docker/whistleblower_registry.bin \
    --envelopes-from demo/sample-envelopes.jsonl
```

Flags accept env vars too: `WL_TOPIC`, `WL_BATCH_SIZE`, `WL_BATCH_INTERVAL_SECS`, `WL_DEDUPE_PATH`, `WL_ENVELOPES_FROM`, `WL_PROGRAM_BIN`, `WL_MOCK_DELIVERY`. SIGINT triggers a graceful flush.

## Inspect on-chain registry entries

Each anchored CID lives in its own PDA derived from `(program_id, sha256("lp0017:cid:v1\0" || cid))`. Read it with `spel inspect`:

```bash
# Derive the PDA from a CID, then inspect it with the SPEL-generated IDL.
python3 - <<'PY'
import base58, hashlib, sys
from hashlib import sha256
program_id = bytes.fromhex("54c7f793caa540408ce2ca4c22051d78c466cd5ed8db607feedd19dcb749aa91")
cid = (sys.argv[1] if len(sys.argv) > 1 else "bafybeigdyrzt3qgq2gqexamplelp0017cid00000000000000000000000000").encode()
seed = sha256(b"lp0017:cid:v1\0" + cid).digest()
print(base58.b58encode(hashlib.sha256(program_id + seed).digest()).decode())
PY
spel inspect <pda-base58> --idl whistleblower-registry.idl.json --type AnchorEntry
```

Returns the JSON-decoded `AnchorEntry { cid, cid_hash, metadata_hash, anchor_timestamp }`.

## Architecture & key decisions

The locked design decisions (`REGISTRY_SPIKE.md` has the on-chain spike detail):

- **PDA-per-CID** registry storage (not single-root-PDA) — O(1) anchor cost, unbounded capacity, idempotency-by-default-state-check.
- **Raw `nssa_core` guest** (not SPEL macros) — the deployed guest ELF (`54c7f793…aa91`) is hand-rolled `nssa_core` for a lean cross-compile. The IDL is still **machine-generated via `spel generate-idl`** from a parse-only `#[lez_program]` mirror at `idl/whistleblower_registry.rs` that is never compiled (regen: `bash scripts/regen-idl.sh`). `spel generate-idl` only AST-parses its input, so this pulls nothing into the guest build — the earlier "spel-framework forces bonsai-sdk into the riscv32im build" claim was wrong (it's a `k256` host feature, off by default, with no bonsai dep anywhere in LEZ). One documented gap: SPEL's seed model (`const|account|arg`) can't express our `sha256(domain‖cid)` PDA seed — filed upstream (see [`BUGS_FILED.md`](BUGS_FILED.md)).
- **Adapter-based reusable indexing module** — Qt-free Rust core, `Arc<dyn StorageClient + DeliveryClient + RegistryClient>` boundary. Real LEZ registry adapter + a real headless file-replay `DeliveryClient` (`--envelopes-from`) in tree; the UI plugin supplies real Storage and an opt-in Delivery send path via in-process `LogosAPIClient`. A headless live Waku Delivery client (Waku + RLN over QtRemoteObjects) is a documented separate integration — see [`adapters/logos/README.md`](adapters/logos/README.md).
- **Wallet-free upload + envelope creation** — only on-chain anchoring needs a wallet, satisfying spec line 17 ("without identifying the uploader"). Delivery broadcast is available as an opt-in best-effort step when the host Delivery module is enabled.
- **Topic** = `/lp0017-whistleblower/1/cids/json` (LIP-23 shape). Constant in `whistleblower-core`.

## Spec compliance map

| Spec § | Requirement | Where |
|---|---|---|
| Functionality 1 | Upload to Logos Storage | `ui/src/WhistleblowerBackend.cpp::uploadToStorage` via `LogosAPIClient::invokeRemoteMethodAsync("storage_module", "uploadUrl", ...)` |
| Functionality 2 | Broadcast envelope to Logos Delivery topic | `ui/src/WhistleblowerBackend.cpp::broadcastEnvelope` via `LogosAPIClient::invokeRemoteMethodAsync("delivery_module", "send", ...)` when `WHISTLEBLOWER_ENABLE_DELIVERY=1`; topic = `core::DEFAULT_CONTENT_TOPIC`. Publish/anchor remains usable when Delivery is unavailable. |
| Functionality 3 | Optional anchor on-chain | `Publisher::anchor_published` (Rust) + `whistleblower_anchor_one` FFI exposed to QML |
| Functionality 4 | Batch anchor CLI tool | `whistleblower-batch` binary (`batch/`) — subscribes to topic, batches, anchors |
| Functionality 4 idempotency | Re-submitting registered CID succeeds no-op | Built into `process_entry` in the guest; `LezRegistryClient` exercises it |
| Functionality 5 | On-chain registry stores (CID, metadata_hash, anchor_timestamp) | `AnchorEntry` in `core/src/lib.rs`; one PDA per CID |
| Functionality 6 | Document-indexing module reusable | `document-indexing` crate, no Qt dep, public `Publisher` API |
| Usability | LEZ program IDL via SPEL framework | `whistleblower-registry.idl.json` — **generated via `spel generate-idl`** from `idl/whistleblower_registry.rs` (regen: `bash scripts/regen-idl.sh`); `spel inspect … --type AnchorEntry` decodes on-chain entries. instructions/accounts emitted verbatim by spel; PDA-seed caveat documented in the IDL `provenance` + [`BUGS_FILED.md`](BUGS_FILED.md) |
| Usability | Basecamp app GUI | `ui/` Qt6/QML plugin → `dist/whistleblower-plugin.lgx` (2.4MB darwin-arm64) |
| Reliability | Upload retries with backoff | `Publisher` wraps every adapter call in `with_retry` (5 retries, exponential) |
| Reliability | Delivery dedup | `DurableDedupeStore` in `batch::run_batch_loop` (sled-backed) |
| Reliability | Batch tool resumes from last successfully anchored | Persistent dedupe ledger; registry idempotency means safe re-runs |
| Performance | CU benchmarks single + 50-CID batch | `BENCHMARKS.md` — per-tx CU is not exposed by the testnet RPC (no receipt method; filed upstream, [`BUGS_FILED.md`](BUGS_FILED.md) #7), so CU = executor cycles of the **deployed rc3 ELF** (`54c7f793…`), which the deterministic zkVM makes exactly equal to on-chain CU. Authoritative measured values: `anchor_one` = 100,185 user cycles; `anchor_batch(50)` = 4,506,872 user cycles (~90 K/CID). Testnet latency + payload/proof size captured live. |
| Supportability | Deployed on LEZ testnet | **complete** — public testnet `testnet.lez.logos.co` (2026-06-03), program `54c7f793…aa91`, deploy + anchor lifecycle confirmed on chain; see [`TESTNET_PROOF.md`](TESTNET_PROOF.md), re-verify via `bash scripts/verify-testnet.sh` |
| Supportability | E2E integration tests in CI with `RISC0_DEV_MODE=0` | `.github/workflows/ci.yml` `verify-testnet` job runs on **every push** — re-queries the deployed program's transactions from the public sequencer (read-only, no secrets) and fails if any are missing. Replaces the old `workflow_dispatch`+`exit 1` stub that never ran. |

## License

Dual-licensed under MIT (`LICENSE-MIT`) or Apache 2.0 (`LICENSE-APACHE`) at the recipient's option.
