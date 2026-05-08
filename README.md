# Whistleblower

LP-0017 reference implementation: a censorship-resistant document upload + indexing system on the Logos stack. Published under MIT or Apache-2.0 at the user's option.

Anyone can upload a document → its CID is broadcast over Logos Delivery → any altruistic third party (or the publisher themselves) can later batch-anchor accumulated CIDs to a LEZ program. The on-chain registry stores `(CID, metadata_hash, anchor_timestamp)` per document and is queryable by CID hash without a transaction.

> **Status:** built end-to-end — registry program + indexing module + LEZ adapter exercised against a live local sequencer in non-dev mode (`risc0_dev_mode = false`, `RISC0_DEV_MODE=0`); 50-CID batch validated at ~120ms zkVM executor time (~2.5ms/CID amortized — see `BENCHMARKS.md`); Qt6/QML Basecamp UI plugin built into a portable `.lgx` package via nix and smoke-tested in real Basecamp on 2026-05-09 (`lgs basecamp install` + `lgs basecamp launch alice`; storage upload returned a manifest CID and delivery broadcast sent the CID JSON envelope). **Remaining for submission:** get devnet RPC credentials from Logos Discord, record the narrated demo video, migrate to public repo, and open the PR after Evi sign-off. See [`ui/README.md`](ui/README.md) "What's left" for the detailed punch list.

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
├── whistleblower-registry-idl.json   # hand-written SPEL IDL for the registry
├── ARCHITECTURE.md           # design with locked decisions + risk table
├── REGISTRY_SPIKE.md         # Task 1.0B spike result + rerun instructions
├── BENCHMARKS.md             # CU benchmarks — localnet captured, devnet TBD
├── DEPLOYMENT.md             # Local + devnet deployment commands
├── DEMO.md                   # End-to-end demo script for the submission video
└── BUGS_FILED.md             # Upstream Logos issues filed during the build
```

## Quick start (local sequencer)

You need: macOS arm64 with `cargo`, `docker`, and the Logos toolchain installed via the order documented in [`reference_logos_repos.md`](../../../.claude/projects/-Users-evinova-Projects/memory/reference_logos_repos.md). Verified: `lgs 0.1.1`, `spel 0.2.0`, `cargo-risczero 3.0.5`, `r0vm 3.0.5`, `rzup 0.5.1`, `logos-blockchain-circuits v0.4.2`.

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

The plugin is a Qt6/QML widget that loads inside Basecamp, exposes the file picker + metadata form + 4-stage publish progress UI, and bridges to Logos Storage / Delivery via `LogosAPIClient` and to the on-chain registry via the Rust FFI cdylib. Build via the workspace-root nix flake on a machine with the Logos toolchain installed:

```bash
nix build .#ffi      # Rust cdylib (~3-4 min)
nix build .#plugin   # Qt6 plugin + standalone preview app
nix build .#lgx      # portable .lgx package — the spec deliverable
nix run  .#install   # copies plugin into Basecamp dev plugin dir
lgs basecamp install # evaluator path: installs storage_module + delivery_module + whistleblower
scripts/fix_delivery_rln.sh # workaround for upstream delivery_module librln install-name bug
```

The `.lgx` file is the spec deliverable. Verified end-to-end on m4pro (aarch64-darwin) on 2026-05-04: `dist/whistleblower-plugin.lgx` (2.4MB). See [`ui/README.md`](ui/README.md) for the manual `cmake -B build` development workflow.

## Use the batch anchor CLI

The CLI is the spec's "permissionless batch anchor tool" (line 33). It owns the on-chain side end-to-end: dedupe ledger, batching window, retry, idempotent anchor against the deployed program.

The Storage / Delivery integration the CLI subscribes to is currently mocked behind `--mock-delivery`. The real Storage + Delivery integration ships in the Basecamp UI plugin (`ui/`, see [`ui/README.md`](ui/README.md)) — that plugin uses Logos Core's in-process `LogosAPIClient` to call the storage / delivery modules directly. A headless CLI equivalent would need either a Rust QtRemoteObjects client against the per-module `logos_host` process, or a Basecamp-plugin variant of this binary that reuses the same `LogosAPI` handle. Both options + tradeoffs are documented in [`adapters/logos/README.md`](adapters/logos/README.md). For now, the CLI runs in mock-delivery mode for development and CI:

```bash
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet \
  ./target/release/whistleblower-batch \
    --topic /lp0017-whistleblower/1/cids/json \
    --batch-size 10 \
    --batch-interval-secs 30 \
    --dedupe-store-path queue.db \
    --mock-delivery
```

Flags accept env vars too: `WL_TOPIC`, `WL_BATCH_SIZE`, `WL_BATCH_INTERVAL_SECS`, `WL_DEDUPE_PATH`, `WL_MOCK_DELIVERY`. SIGINT triggers a graceful flush.

## Inspect on-chain registry entries

Each anchored CID lives in its own PDA derived from `(program_id, sha256("lp0017:cid:v1\0" || cid))`. Read it with `spel inspect`:

```bash
# Compute the PDA for a known CID first (script helper TBD; for now use anchor_spike's printout).
spel inspect <pda-base58> --idl whistleblower-registry-idl.json --type AnchorEntry
```

Returns the JSON-decoded `AnchorEntry { cid, cid_hash, metadata_hash, anchor_timestamp }`.

## Architecture & key decisions

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the design + the locked decisions:

- **PDA-per-CID** registry storage (not single-root-PDA) — O(1) anchor cost, unbounded capacity, idempotency-by-default-state-check.
- **Raw `nssa_core` guest** (not SPEL macros) — spel-framework forces `bonsai-sdk` into the riscv32im build via the `host` feature on `nssa_core`, which fails to cross-compile (ring/Apple Metal). IDL is hand-written.
- **Adapter-based reusable indexing module** — Qt-free Rust core, `Arc<dyn StorageClient + DeliveryClient + RegistryClient>` boundary, mock + real LEZ adapters in tree, real Logos Core module adapters deferred to Phase 1.7.
- **Wallet-free upload + broadcast** — only on-chain anchoring needs a wallet, satisfying spec line 17 ("without identifying the uploader").
- **Topic** = `/lp0017-whistleblower/1/cids/json` (LIP-23 shape). Constant in `whistleblower-core`.

## Spec compliance map

| Spec § | Requirement | Where |
|---|---|---|
| Functionality 1 | Upload to Logos Storage | `ui/src/WhistleblowerBackend.cpp::uploadToStorage` via `LogosAPIClient::invokeRemoteMethodAsync("storage_module", "uploadUrl", ...)` |
| Functionality 2 | Broadcast envelope to Logos Delivery topic | `ui/src/WhistleblowerBackend.cpp::broadcastEnvelope` via `LogosAPIClient::invokeRemoteMethodAsync("delivery_module", "send", ...)`; topic = `core::DEFAULT_CONTENT_TOPIC` |
| Functionality 3 | Optional anchor on-chain | `Publisher::anchor_published` (Rust) + `whistleblower_anchor_one` FFI exposed to QML |
| Functionality 4 | Batch anchor CLI tool | `whistleblower-batch` binary (`batch/`) — subscribes to topic, batches, anchors |
| Functionality 4 idempotency | Re-submitting registered CID succeeds no-op | Built into `process_entry` in the guest; `LezRegistryClient` exercises it |
| Functionality 5 | On-chain registry stores (CID, metadata_hash, anchor_timestamp) | `AnchorEntry` in `core/src/lib.rs`; one PDA per CID |
| Functionality 6 | Document-indexing module reusable | `document-indexing` crate, no Qt dep, public `Publisher` API |
| Usability | LEZ program IDL via SPEL framework | `whistleblower-registry-idl.json` (hand-written; `spel inspect` reads it) |
| Usability | Basecamp app GUI | `ui/` Qt6/QML plugin → `dist/whistleblower-plugin.lgx` (2.4MB darwin-arm64) |
| Reliability | Upload retries with backoff | `Publisher` wraps every adapter call in `with_retry` (5 retries, exponential) |
| Reliability | Delivery dedup | `DurableDedupeStore` in `batch::run_batch_loop` (sled-backed) |
| Reliability | Batch tool resumes from last successfully anchored | Persistent dedupe ledger; registry idempotency means safe re-runs |
| Performance | CU benchmarks single + 50-CID batch | `BENCHMARKS.md` — localnet captured (~120ms total zkVM executor for 50-CID batch, ~2.5ms/CID amortized); **devnet TBD** (RPC URL gated behind Discord-issued basic-auth credentials — see `BUGS_FILED.md` #7) |
| Supportability | Deployed on LEZ devnet/testnet | **TBD — awaiting devnet RPC URL from Logos team**; `DEPLOYMENT.md` has commands ready |
| Supportability | E2E integration tests in CI with `RISC0_DEV_MODE=0` | `.github/workflows/ci.yml` — workspace tests + ignored live-LEZ tests |

## License

Dual-licensed under MIT (`LICENSE-MIT`) or Apache 2.0 (`LICENSE-APACHE`) at the recipient's option.
