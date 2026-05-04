# `whistleblower-logos-adapter` — bring-up plan

This crate is a **scaffold** for the real Logos Storage + Delivery adapters.
It is not yet wired into the workspace `members` list (so the rest of the
project compiles cleanly without it). The next session that takes this
forward should follow the steps below.

## Status — what's been validated (2026-05-04)

- ✅ Nix install works on the m4pro Tailscale host (`100.84.252.4`).
- ✅ `nix build github:logos-co/logos-liblogos#portable` produces:
  `bin/logos_host` and `bin/logos_host_qt` (the runtime hosts).
  - The `default` and `logos-liblogos` outputs fail with a known
    `gtest_discover_tests` 5-second-timeout issue (already filed upstream
    as logos-basecamp#77 per memory). The `portable` output skips that
    step and links cleanly.
- 🟡 `nix build` of `logos-storage-module` and `logos-delivery-module`
  in progress on m4pro at session end. Track via:
  ```
  ssh m4pro 'tail ~/logos-build/storage.log ~/logos-build/delivery.log'
  ```

## The runtime architecture (rediscovered)

The storage module's `README.md` shows this pattern:

```bash
./logos/bin/logoscore -m ./modules --load-modules storage_module \
  -c "storage_module.init(@config.json)" -c "storage_module.start()"
```

**That `logoscore` binary doesn't exist in the current `logos-liblogos`
build outputs.** What we get is `logos_host` and `logos_host_qt`, with
this CLI:

```
logos_host_qt --name <module> --path <module.dylib> [--instance-persistence-path <dir>]
```

`logos_host` is **per-module** — it loads ONE module's `.dylib` in a
dedicated process and keeps it alive. The IPC mechanism for invoking
the module's `Q_INVOKABLE` methods is **QtRemoteObjects** (the build
links against `qtremoteobjects-6.9.2`).

This is significantly more complex than the "subprocess + parse stderr"
pattern this scaffold was originally designed around. Realistic
integration options:

### Option A: QRO client in Rust

Build a Qt-Remote-Objects client in Rust to talk to the per-module
`logos_host` process. Requires either `qmetaobject`/`cxx-qt` Rust crates
plus matching Qt 6 dev libs OR hand-written QRO wire-format encoder.

- Pros: pure Rust, headless-CLI-friendly
- Cons: significant Qt build complexity, fragile across Qt versions

### Option B: Find or build a CLI client

Check `logos-co/logos-logoscore-tui` (mentioned in memory as the TUI
frontend for `logoscore-cli`). If `logoscore-cli` exists separately,
shell out to it the way the storage README originally implied.

- Pros: matches the documented integration pattern
- Cons: another binary to build + ship; depends on what state that
  CLI is in

### Option C: Build the Basecamp UI plugin (Task 1.7 proper)

The whisper-wall reference shows the canonical path: a Qt/QML plugin
loads the modules via `LogosAPIClient` (the in-process Qt API), exposes
a `.lgx` package for users to install in Basecamp. Rust gets called via
a cdylib FFI for the indexing logic.

- Pros: matches the spec exactly, what reviewers will look for
- Cons: full UI work, biggest single deliverable in the LP-0017 scope

### Option D: Skip headless integration, use option C only

Drop this `whistleblower-logos-adapter` crate entirely. Real adapter
lives inside the Basecamp UI plugin (option C). The batch CLI either:
- Stays mock-delivery-only with a clear "use Basecamp app to source the
  CIDs" workflow note
- Embeds a small QRO client (gets us back to option A complexity)
- Calls a separate `logoscore-cli` if/when that lands (option B)

## Recommended next-session path

**Option C** (build the Basecamp UI plugin). The reasoning:

1. The spec REQUIRES a Basecamp app GUI (LP-0017 §Usability). That work
   has to happen anyway.
2. The UI plugin's Rust FFI cdylib already integrates with our
   document-indexing crate via the `Publisher` API — minimal new code.
3. `whisper-wall/ui/` is a working reference that does exactly this:
   `nix build ./ui#install` builds + installs the .lgx, `nix run ./ui#install`
   runs it through a launch script.
4. Defer the headless QRO client until we've shipped the UI and have
   real users asking for CLI workflows.

If the next session takes option C:

```bash
# On m4pro (where Qt 6 is already in the nix store):
ssh m4pro
mkdir -p ~/wb-ui && cd ~/wb-ui
# Copy the whisper-wall ui/ pattern, swap whisper_wall guest for our
# whistleblower-registry guest, rewrite the QML to a file picker +
# upload progress bar + "anchor on chain" button.
# Connect the QML actions to our document-indexing::Publisher via the
# Rust cdylib FFI (whisper-wall's ui/ffi/ has the layout).

nix build ./ui#install  # produces .lgx + installs to ~/.local/share/Logos/...
```

Then test in a real Logos Basecamp instance.

## Build prerequisites (validated 2026-05-04)

On the build host (m4pro):
- Nix installed (Determinate, see install command in BUGS_FILED.md)
- Apple SDK 11.3 (provided automatically by nix)
- 30+ GB free disk for the dep tree
- ~30-60 min for first cold build

## Cleanup

The crate is self-contained and not in the workspace members list, so
`rm -rf adapters/logos/` is safe at any time. No other crate depends
on it.

## Build artifacts location (m4pro)

- `~/logos-build/result/bin/logos_host` — per-module runtime host
- `~/logos-build/result/bin/logos_host_qt` — Qt-wrapped variant
- `~/logos-build/storage-result/lib/*.dylib` — storage module (when build completes)
- `~/logos-build/delivery-result/lib/*.dylib` — delivery module (when build completes)
- `~/logos-build/*.log` — build logs for diagnostics
