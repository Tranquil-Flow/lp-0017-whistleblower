# `lgs create` template host-side runners reference removed wallet API fields

Target repo: `logos-co/logos-scaffold`
Suggested labels: `bug`, `templates`, `good-first-issue`
Severity: low (first-run confusion; misleads new builders into copying broken patterns)

---

## Summary

A fresh `lgs create <name> && lgs build` workspace fails to compile in the
generated `src/bin/run_hello_world*.rs` files because they reference wallet
response fields that no longer exist in the current LEZ client API.

## Symptom

```
error[E0609]: no field `tx_hash` on type `(...)`
   --> src/bin/run_hello_world.rs:38:46
```

## Root cause

The template's example runners reference `response.status` and
`response.tx_hash`, but `wallet::WalletCore::sequencer_client.send_transaction`
now returns a `HashType` (tuple-struct — only `.0` is available).

## Workaround

Delete the template's `run_hello_world*.rs` files and write host-side code
against the current `wallet`/`nssa` API.

## Suggested fix

Regenerate the template's runner examples against the current pinned LEZ commit,
or have `lgs create` emit a deprecation banner on the runner files.
