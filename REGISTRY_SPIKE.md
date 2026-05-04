# Registry Idempotency Spike

Status: partial local reference model complete; real SPEL/LEZ account pattern still unproven.

What is proven in this workspace:

- `RegistryState::anchor_one` creates the first entry for a CID hash.
- A duplicate `anchor_one` for the same CID returns success and leaves the original entry unchanged.
- `anchor_batch` with 10 new CIDs succeeds in one call.
- `anchor_batch` with mixed existing/new CIDs succeeds, skips existing entries, and only creates missing entries.

Tests:

```bash
cargo test -p whistleblower-core --test registry_idempotency_spike
```

Why this is not the final Task 1.0B gate:

The local execution environment does not currently have `lgs` or `spel`, so this
commit proves the desired registry semantics in the shared Rust model, not the
actual SPEL account creation behavior. The critical unknown remains whether SPEL
can implement one-entry-account-per-CID without `#[account(init)]` rejecting
already-initialized duplicate accounts before handler logic can no-op.

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
