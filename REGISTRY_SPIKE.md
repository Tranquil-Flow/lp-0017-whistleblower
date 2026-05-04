# Registry Idempotency Spike — PASSED (PDA-per-CID design)

Status: **PASSED** end-to-end against a real LEZ sequencer (2026-05-04).

## Architecture: PDA-per-CID

Each anchored CID lives in its own LEZ account, deterministically derived as
`AccountId::from((&program_id, &PdaSeed::new(cid_hash)))`. This gives:

- **O(1) per-anchor cost** — each tx touches only its own entries' accounts
- **Unbounded registry capacity** — no shared blob, no per-account size cliff
- **Built-in idempotency** — re-anchoring an existing PDA finds it already
  program-owned (`pre.account.data` non-empty), no-ops without re-claiming
- **Trivial off-chain query** — derive PDA from `cid_hash`, fetch account,
  decode `AnchorEntry` directly. **No transaction required for reads.**

## What's proven

The deployed `whistleblower-registry` LEZ program correctly implements all
four behaviours LP-0017 requires plus the bonus query path:

1. ✅ **First `anchor_one(cid_a)` creates the entry.**
   PDA claimed via `Claim::Pda(seed)`, `AnchorEntry` written to account data.

2. ✅ **Second `anchor_one(cid_a)` is a no-op success.**
   Guest sees non-empty pre-state data, returns `AccountPostState::new(pre)`
   without re-claiming. Original `anchor_timestamp` preserved (re-anchoring
   with newer timestamp does NOT overwrite).

3. ✅ **`anchor_batch([cid_a, cid_b])` with mixed existing/new is partial-success.**
   Single tx with 2 accounts in `account_ids`. `cid_a` no-ops, `cid_b` is
   freshly claimed. Original `cid_a` timestamp preserved.

4. ✅ **`anchor_batch(10 fresh CIDs)` lands in one transaction.**
   Spec line 41 ("≥10 CIDs per batch transaction") satisfied. 10 distinct
   PDAs in one `account_ids` vec; guest re-derives + verifies each match.

5. ✅ **`query_by_cid_hash(cid_a)` reads without a transaction.**
   `LezRegistryClient::query_by_cid_hash` derives the PDA, fetches the
   account, decodes the entry. Confirms the off-chain query path works.

## How to run

```bash
cd ~/Projects/logos-basecamp/lp-0017-whistleblower/whistleblower
lgs localnet start
lgs build
lgs deploy --program-path target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin
cargo build -p anchor-spike --release
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet ./target/release/anchor_spike
```

Or via the full live integration suite (`#[ignore]`'d so it only runs on demand):

```bash
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet \
  cargo test -p whistleblower-lez-adapter --release -- --ignored --nocapture
```

The spike picks a fresh per-run CID suffix so each run anchors brand-new PDAs
(no cross-run contamination).

## Architecture decisions confirmed

- **Registry storage:** PDA-per-CID. Seed = `cid_hash` (32 bytes — fits
  `PdaSeed::new([u8; 32])` exactly). The single-root-PDA v1 design
  (`REGISTRY_PDA_SEED_BYTES`) is deprecated and removed from active code paths.

- **Claim semantics:** `AccountPostState::new_claimed_if_default(post, Claim::Pda(seed))`.
  - First call: PDA in default state → claim emitted with `Claim::Pda`,
    runtime confirms PDA derivation matches and assigns ownership.
  - Subsequent calls: PDA already program-owned → no claim emitted (re-claim
    on a non-default account would fail nssa validation).
  - `Claim::Authorized` would NOT work — it requires signer auth on the PDA,
    which we don't have.

- **Wire format:** Host borsh-encodes `RegistryInstruction` into `Vec<u8>`,
  sends via nssa serde. Guest reads via `read_nssa_inputs::<Vec<u8>>` then
  borsh-decodes. The host is also responsible for pre-deriving all entry
  PDAs and listing them in `Message.account_ids` in the same order as the
  instruction's entries — guest verifies the match.

- **State format:** `AnchorEntry { cid, cid_hash, metadata_hash, anchor_timestamp }`,
  borsh-encoded directly into the PDA's `account.data`. No wrapper struct —
  one entry per account.

## What's NOT yet proven (future work)

- **CU benchmarks for single-CID and 50-CID batch** (`BENCHMARKS.md` empty).
- **`anchor_timestamp` from LEZ clock account** — currently host-supplied.
  See `program_methods/guest/src/bin/clock.rs` in LEZ for the canonical
  pattern (clock_01_program_account_id).
- **Real Storage / Delivery integration** — the `LezRegistryClient` proves
  the on-chain registry path; `MockStorageClient` / `MockDeliveryClient`
  cover the upload/broadcast path in unit tests but real Logos Core module
  bindings (Qt-based, nix-built) are deferred to Phase 1.7.
- **IDL JSON via `spel generate-idl`** — our guest is raw `nssa_core` (not
  SPEL macros) because spel-framework's `host` feature on `nssa_core` pulls
  bonsai-sdk into the riscv32im build (cross-compile failure). Hand-writing
  the IDL JSON is the next concrete step.

## Reproduced artifacts

- `methods/guest/src/bin/whistleblower_registry.rs` — PDA-per-CID guest
- `core/src/lib.rs` — `RegistryInstruction`, `AnchorEntry`, `cid_hash()`
- `adapters/lez/src/lib.rs` — `LezRegistryClient` + `entry_pda_for(...)`
- `anchor_spike/src/main.rs` — host runner exercising adapter + queries
- `adapters/lez/tests/live_registry.rs` — live integration tests
- `indexing/src/publisher.rs` — Publisher orchestrator (mock + real adapters)
- This file — spike documentation
