# `whistleblower-logos-adapter` — design note (deferred)

This crate is a **deliberate placeholder** for a future headless Rust adapter
that talks to the Logos Storage / Delivery modules from outside the Basecamp
UI process. It is intentionally not in the workspace `members` list — nothing
in the project depends on it — and exists only to document the design space
for whoever picks up the headless-live-Delivery integration later.

## What already exists (so this is scoped correctly)

- **Real-Storage / real-Delivery** ships in the [`ui/`](../../ui/) Basecamp
  plugin via Logos Core's in-process `LogosAPIClient` over QtRemoteObjects —
  the spec's "Basecamp app" deliverable, end-to-end.
- **A real, headless `DeliveryClient` for the batch CLI** already exists:
  `document_indexing::FileDeliveryClient`, driven by
  `whistleblower-batch --envelopes-from <file>`. It replays the exact
  `MetadataEnvelopeV1` records the Delivery topic carries through the real
  dedupe + batch + on-chain anchor pipeline — no mock. (`--mock-delivery`
  remains as a dev-only option.)

So the **only** thing this crate would add is a *headless live subscription*
to Logos Delivery — i.e. the batch CLI subscribing to Waku itself instead of
being fed envelopes from a file or the UI. That is the hard part, for the
reasons below.

## Why headless live Delivery is hard

Logos Delivery is **Waku + RLN** (the `delivery_module`'s dlopen pulls
`librln.dylib` / Zerokit — see `BUGS_FILED.md` #5). Publishing to / subscribing
from the live network is therefore not a plain socket call: RLN-protected
relays require a valid rate-limiting-nullifier membership, which the
`delivery_module` manages. Two consequences:

1. You cannot just point an off-the-shelf nwaku REST client at the network and
   publish — without RLN membership the relays drop the message.
2. The `delivery_module`'s IPC is **structured QtRemoteObjects calls**, not a
   CLI or REST surface, so "subprocess + parse stderr" does not work.

## Runtime architecture

`logos-co/logos-liblogos` builds two binaries:

```
logos_host        # per-module runtime host (loads ONE .dylib in a process)
logos_host_qt     # Qt-wrapped variant
```

They take a single module's `.dylib` and keep it alive. The IPC mechanism
for invoking the module's `Q_INVOKABLE` methods is **QtRemoteObjects** — the
build links against `qtremoteobjects-6.9.2`. There is no `logoscore` umbrella
binary in the current build outputs, despite what the storage module README
shows; that path was the older monolithic CLI shape.

The earlier scaffold here was designed around "subprocess + parse stderr,"
which doesn't work against `logos_host` — its IPC is structured QtRO calls,
not a CLI.

## Why this is hard, and the three options

### Option A — QtRemoteObjects client in Rust

Build a QRO client in Rust to talk to the per-module `logos_host` process.
Either `qmetaobject` / `cxx-qt` Rust crates plus matching Qt 6 dev libs, or
hand-written QRO wire-format encoder.

- **Pros:** pure Rust, headless-CLI-friendly, reuses the rest of the indexing
  crate unchanged.
- **Cons:** significant Qt build complexity, fragile across Qt minor versions,
  the wire format is undocumented and tracks Qt internals.

### Option B — Restructure batch CLI as a Basecamp plugin component

Wrap the batch loop as a second Qt plugin that loads inside Basecamp and
reuses the same `LogosAPI` handle the UI plugin gets. The CLI becomes a
"start a background batch session" toggle in the UI rather than a separate
binary.

- **Pros:** zero new IPC surface, reuses the working UI-plugin integration
  verbatim, ships in the same `.lgx`.
- **Cons:** loses the "permissionless, run anywhere" property the spec
  envisions for the batch tool — operators must run Basecamp.

### Option C — Wait for `logoscore-cli`

Memory references a `logos-logoscore-tui` project as a TUI frontend for a
hypothetical `logoscore-cli`. If that CLI ships separately, the original
"subprocess" pattern becomes viable again.

- **Pros:** matches the documented integration pattern in the storage module
  README.
- **Cons:** depends on upstream work that may or may not land.

### Option D — Direct Waku + RLN membership in Rust

Skip the Logos module entirely and speak to the Waku network directly from
Rust (nwaku bindings / REST), managing an RLN membership so relays accept the
traffic.

- **Pros:** truly headless, no Qt.
- **Cons:** you reimplement the RLN-membership management the `delivery_module`
  already does; brittle against Waku/RLN protocol changes; the most work.

## Decision

Defer the *headless live subscription*. The deliverables it would serve are
already met another way:

- **"Basecamp app"** → the UI plugin (real Storage + Delivery, in-process).
- **"Permissionless batch anchor tool"** → `whistleblower-batch` anchors on
  chain for real, fed by `document_indexing::FileDeliveryClient`
  (`--envelopes-from`) — the broadcast envelopes replayed through the real
  pipeline, no mock.
- **"Reusable indexing module"** → all three traits + mock + LEZ + file-replay
  impls live in [`indexing/`](../../indexing/).

A headless live Waku subscription (Option A or D) is genuinely multi-day and
**untestable without the full Logos + Waku + RLN stack running**, so shipping
it now would mean unverified code. It is left as documented future work rather
than a half-built, untested integration.

## If you do pick this up

The crate compiles as an empty stub today. Add it to the workspace `members`
list in [`../../Cargo.toml`](../../Cargo.toml), then implement
`StorageClient` and `DeliveryClient` from `document-indexing::traits` against
your chosen IPC mechanism. The contract tests in
[`indexing/tests/adapter_contract.rs`](../../indexing/tests/adapter_contract.rs)
will exercise the implementation against the same expectations the mock and
file-replay adapters meet — no special test infra needed.

The natural model is `document_indexing::FileDeliveryClient`: a `DeliveryClient`
whose `subscribe` yields `ReceivedEnvelope`s. Swap its finite file stream for a
live QtRO/Waku stream and the rest of `run_batch_loop` is unchanged.
