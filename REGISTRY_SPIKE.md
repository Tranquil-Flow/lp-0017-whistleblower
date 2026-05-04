# Registry Idempotency Spike — PASSED

Status: **PASSED** end-to-end against a real LEZ sequencer (2026-05-04).

## What's proven

The deployed `whistleblower-registry` LEZ program correctly implements the
duplicate-safe registry semantics LP-0017 requires:

1. ✅ **First `anchor_one(cid_a)` creates the entry.**
   Registry PDA is claimed by the program via `Claim::Pda(seed)`, state is
   populated with one `AnchorEntry`.

2. ✅ **Second `anchor_one(cid_a)` is a no-op success.**
   No InvalidProgramBehavior, state unchanged, original `anchor_timestamp`
   preserved (re-submission with newer timestamp does NOT overwrite).

3. ✅ **`anchor_batch([cid_a, cid_b])` with mixed existing/new is partial-success.**
   `cid_a` is skipped (already present), `cid_b` is added — single tx, no error.

4. ✅ **`anchor_batch(10 fresh CIDs)` lands in one transaction.**
   Spec line 41 ("≥10 CIDs per batch transaction") satisfied. Entry count
   jumps by exactly 10.

## How to run

```bash
cd ~/Projects/logos-basecamp/lp-0017-whistleblower/whistleblower
lgs localnet start
lgs build
lgs deploy --program-path target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin
cargo build -p anchor-spike --release
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet ./target/release/anchor_spike
```

The spike is idempotent across runs — it measures deltas from the current
registry state, so re-running it just adds more CIDs to the same PDA.

## Architecture decisions confirmed

- **Registry storage:** Single registry-root PDA holding `Vec<AnchorEntry>`.
  PDA seed = `REGISTRY_PDA_SEED_BYTES` (constant in `whistleblower-core`).
  Bucketing is a follow-up only if entry count grows large.

- **Claim semantics:** `AccountPostState::new_claimed_if_default(post, Claim::Pda(seed))`.
  - First call: PDA in default state → claim emitted, runtime assigns
    program ownership.
  - Subsequent calls: PDA already owned → no claim emitted (re-claiming a
    non-default account would fail nssa validation).
  - Critically NOT `Claim::Authorized` — that requires signer authorization,
    which we don't have for the PDA. Lost ~30 minutes here; an upstream docs
    PR would help next builder.

- **Wire format:** Host borsh-encodes `RegistryInstruction` and sends it
  as `Vec<u8>`; guest reads via `read_nssa_inputs::<Vec<u8>>` then
  borsh-decodes. Round-trip clean.

- **State format:** `RegistryStateOnChain { entries: Vec<AnchorEntry> }`,
  borsh-encoded into the PDA's `account.data` field.

## What's NOT yet proven (deferred to Task 1.1 proper)

- Per-CID PDA-per-account variant (vs single root PDA) — current shape works
  for spike, may need bucketing later if entry count grows large.
- Compute Unit benchmarks for single vs 50-CID anchor (Phase 1.1.8).
- IDL generation via `spel generate-idl` against our raw-nssa guest. SPEL
  macros aren't used in the guest because spel-framework's `host` feature on
  `nssa_core` pulls bonsai-sdk transitively into the riscv32im build, which
  fails to cross-compile (ring/Apple Metal). `spel generate-idl` only scans
  `#[lez_program]` annotations, so we'll need to either add SPEL wrapper
  macros (and figure out the bonsai issue), hand-write the IDL JSON, or
  derive it from `RegistryInstruction` programmatically.

## Reproduced artifacts

- `methods/guest/src/bin/whistleblower_registry.rs` — the SPEL-free LEZ guest
- `core/src/lib.rs` — shared `RegistryInstruction` + `REGISTRY_PDA_SEED_BYTES`
- `anchor_spike/src/main.rs` — host runner with tx-confirmation polling
- This file — spike documentation
