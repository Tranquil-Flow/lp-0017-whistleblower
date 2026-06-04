# Public LEZ testnet does not expose or persist per-transaction compute units

Target repo: `logos-blockchain/logos-execution-zone`
Suggested labels: `enhancement`, `rpc`, `observability`
Severity: moderate (multiple λPrize specs require testnet CU benchmarks the testnet cannot provide)

---

## Summary

Several λPrize specs (LP-0017 line 62, LP-0002 line 49, LP-0005 line 55) require
documenting per-operation compute-unit (CU) cost "on LEZ devnet/testnet". On the
public testnet (commit `cf3639d8`) there is no way to read a transaction's CU,
and the data is not merely hidden — it is never persisted.

## Detail

- `getTransactionReceipt` / `getReceipt` → `-32601 Method not found` (live).
- None of the sequencer's 11 RPC methods (`sequencer/service/rpc/src/lib.rs`)
  returns execution cost.
- The RISC0 executor computes cycle counts in `SessionInfo`, but
  `nssa/src/program.rs:80` consumes only `session_info.journal` and discards the
  cycle data.
- `ProgramOutput`, `StateDiff`, `NSSATransaction`, and `Block`/`BlockHeader`
  carry no cost field, so the indexer + explorer have nothing to show. Fees are
  explicitly TODO (`nssa/src/program.rs:19`; the `GasConfig` in
  `wallet/src/config.rs:168` is dead/unreferenced).

## Impact

Builders can only report CU as the deterministic executor-cycle cost of the
deployed ELF (the zkVM is deterministic, so executing the deployed program for a
given input equals the cycles consumed on the testnet) — there is no on-chain /
RPC source of truth to cite.

## Suggested fix

Capture `session_info` cycle counts in `nssa/src/program.rs`, thread them through
`ProgramOutput` → tx/block metadata, and add a `getTransactionReceipt` RPC
returning `{ tx_hash, compute_units, … }`. This unblocks every prize that asks
for testnet CU benchmarks.
