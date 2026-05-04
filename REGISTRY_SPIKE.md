# Registry Idempotency Spike

Status: partial local reference model complete; real SPEL/LEZ account pattern still unproven.

What is proven in this workspace:

- `RegistryState::anchor_one` creates the first entry for a CID hash.
- A duplicate `anchor_one` for the same CID returns success and leaves the original entry unchanged.
- `anchor_batch` with 10 new CIDs succeeds in one call.
- `anchor_batch` with mixed existing/new CIDs succeeds, skips existing entries, and only creates missing entries.
- `RegistryInstruction::{AnchorOne, AnchorBatch}` Borsh-serializes cleanly for the future guest/host boundary.
- `RegistryState::apply` executes those instruction envelopes while preserving duplicate-safe semantics.

Tests:

```bash
cargo test -p whistleblower-core --test registry_idempotency_spike
cargo test -p whistleblower-core --test registry_instruction
```

Tooling status:

- `lgs` can be installed in this Linux container by setting `TMPDIR` and `CARGO_TARGET_DIR` outside `/tmp`.
- `spel` installation was attempted with rustc 1.85 and then rustc 1.95 via rustup.
- The rustc 1.85 attempt failed because current transitive dependencies require newer Rust.
- The rustc 1.95 attempt progressed into compilation but was killed with exit code 137 while compiling the large Logos dependency graph, likely memory pressure in this 16GB container.

Why this is not the final Task 1.0B gate:

This commit proves the desired registry semantics and shared guest-input encoding in the Rust model, not the actual SPEL account creation behavior. The critical unknown remains whether SPEL can implement one-entry-account-per-CID without `#[account(init)]` rejecting already-initialized duplicate accounts before handler logic can no-op.

Next steps for the real gate:

1. Run in the Logos toolchain environment where `lgs`, `spel`, RISC Zero, and the
   local LEZ sequencer are available.
2. Port `RegistryState` semantics into `methods/guest/src/bin/whistleblower_registry.rs`.
3. Prove duplicate-safe behavior against local LEZ accounts:
   - first `anchor_one` creates entry
   - duplicate `anchor_one` is success/no-op
   - 10-CID `anchor_batch` succeeds
   - mixed existing/new `anchor_batch` succeeds
4. If SPEL rejects duplicate initialized accounts before handler logic, switch to
   manually validated mutable accounts or fixed bucket accounts and update
   `ARCHITECTURE.md` / `TASKS.md` before building CLI or UI flows.
