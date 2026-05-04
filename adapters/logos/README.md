# `whistleblower-logos-adapter` — design note (deferred)

This crate is a **deliberate placeholder** for a future headless Rust adapter
that talks to the Logos Storage / Delivery modules from outside the Basecamp
UI process. It is intentionally not in the workspace `members` list — nothing
in the project depends on it — and exists only to document the design space
for whoever picks up the headless-CLI integration later.

The canonical real-Storage / real-Delivery integration ships in the
[`ui/`](../../ui/) Basecamp plugin, which uses Logos Core's in-process
`LogosAPIClient` to call the modules over QtRemoteObjects. That covers the
spec's "Basecamp app" deliverable end-to-end. This scaffold only matters if
you want a headless equivalent of `whistleblower-batch` that does its own
upload + broadcast (today the batch CLI runs `--mock-delivery` and relies on
the UI plugin to populate the topic).

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

## Decision (2026-05-04)

Defer. The UI plugin satisfies the spec's "Basecamp app" requirement and the
batch CLI satisfies the spec's "permissionless batch anchor tool" requirement
on the on-chain side. Headless real-Delivery for the CLI is not in the LP-0017
deliverable list — the spec only requires that the indexing module be
"reusable" (it is — all three adapter traits + their mock + lez impls are in
[`indexing/`](../../indexing/)). A future PR can add option A or B once we
have a clearer picture of what the upstream stack settles on.

## If you do pick this up

The crate compiles as an empty stub today. Add it to the workspace `members`
list in [`../../Cargo.toml`](../../Cargo.toml), then implement
`StorageClient` and `DeliveryClient` from `document-indexing::traits` against
your chosen IPC mechanism. The contract tests in
[`indexing/tests/adapter_contract.rs`](../../indexing/tests/adapter_contract.rs)
will exercise the implementation against the same expectations the mock
adapters meet — no special test infra needed.

The `whistleblower-batch` binary already has a feature gate (`--mock-delivery`)
that's the natural insertion point.
