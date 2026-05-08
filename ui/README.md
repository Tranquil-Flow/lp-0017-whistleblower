# Whistleblower Basecamp UI plugin (LP-0017)

Qt6 + QML + Rust FFI plugin for the Logos Basecamp app. Lets a user pick a
document, upload it to Logos Storage, broadcast the `(CID, metadata)`
envelope over Logos Delivery, and anchor the CID on the LEZ registry.

## Status — built end-to-end

| Component | State |
|---|---|
| `manifest.json` + `metadata.json` | ✅ Configured for `whistleblower` plugin name + `storage_module`/`delivery_module` deps |
| `qml/Main.qml` | ✅ File picker + metadata form + 4-stage progress bar + Publish/Anchor buttons |
| `ffi/` (Rust C ABI) | ✅ 4 unit tests pass. Three exported FFI calls: `whistleblower_anchor_one`, `whistleblower_query_by_cid`, `whistleblower_compute_metadata_hash`. Builds via `cargo build --release -p whistleblower_ffi` AND via `nix build .#ffi` (workspace-root flake). |
| `src/WhistleblowerPlugin.{h,cpp}` | ✅ Standard Qt plugin entry |
| `src/WhistleblowerBackend.{h,cpp}` | ✅ **Real LogosAPI integration** — uses `m_api->getClient()` → `requestObject()` → `onEvent()` for storage/delivery modules. Calls `invokeRemoteMethodAsync` for `uploadUrl` / `send`. Single-flight callbacks with safety timeouts. |
| `src/main.cpp` | ✅ Standalone preview app for manual UI iteration without Basecamp |
| `CMakeLists.txt` (workspace root: `../`) | ✅ Builds Qt plugin + preview app, links Rust FFI cdylib + liblogos_sdk.a, finds Qt6 RemoteObjects |
| `../flake.nix` | ✅ **Workspace-root flake builds the full chain.** `.#ffi`, `.#plugin`, `.#lgx`, `.#install` all work. Built `dist/whistleblower-plugin.lgx` (2.4MB) end-to-end on m4pro. |

## What's left

1. ✅ ~~Wire `uploadToStorage()` and `broadcastEnvelope()`~~ — DONE.
   Backend uses `LogosAPIClient::invokeRemoteMethodAsync` for
   `uploadUrl` / `send` and subscribes via `onEvent` for the
   completion signals.

2. ✅ ~~Recompute the FFI cargo-lock hashes~~ — DONE.
   `logos-blockchain-blend-crypto-0.1.2` updated to match our pinned
   rev. Set captured in `flake.nix`.

3. ✅ ~~Build + install via nix~~ — DONE. `nix build .#lgx` produces
   `dist/whistleblower-plugin.lgx` (2.4MB, darwin-arm64 variant with
   both .dylibs + manifest, all references portable).

4. **Wire `whistleblower-batch`** to drop `--mock-delivery` once the
   real Logos Delivery integration is in. Two options:
   a) Add a parallel Rust adapter (`whistleblower-logos-adapter` —
      see `adapters/logos/`) that drives `logos_host` as a subprocess
      for headless use.
   b) Restructure batch CLI to be a Basecamp plugin component that
      reuses the same LogosAPI handle the UI plugin gets.

5. ✅ ~~Test the .lgx in a real Basecamp instance.~~ — DONE (2026-05-09).
   `lgs basecamp install` + `lgs basecamp launch alice` loads the plugin,
   storage_module accepts the upload (manifest CID returned), delivery_module
   broadcasts the CID JSON envelope on `/lp0017-whistleblower/1/cids/json`,
   and the UI reflects the green "Uploaded — CID …" + "Working: broadcasting
   to Logos Delivery…" states.

   **One-time fix required:** the upstream `logos-co/logos-delivery-module#lgx`
   flake omits `librln.dylib` from its install output and bakes a Nix
   sandbox path into `liblogosdelivery.dylib`'s load command. Run
   `scripts/fix_delivery_rln.sh` after every `lgs basecamp launch <profile>`
   that does **not** use `--no-clean` (the clean-slate scrub re-extracts the
   broken upstream output). Documented as `BUGS_FILED.md §8`.

   Pre-built Basecamp binaries are at
   <https://github.com/logos-co/logos-basecamp/releases/latest>:
   - macOS arm64: `LogosBasecamp-Desktop-vX.Y.Z-aarch64.dmg`
   - Linux arm64/x86_64: `…-aarch64.AppImage` / `…-x86_64.AppImage`

   Plugin directory varies by host OS:
   - macOS: `~/Library/Application Support/Logos/LogosBasecampDev/plugins/whistleblower/`
   - Linux: `~/.local/share/Logos/LogosBasecampDev/plugins/whistleblower/` (XDG)

   `nix run .#install` does this automatically. Or use Basecamp's GUI
   "Install plugin" flow against the .lgx file directly.

   ```bash
   # Required env vars before launching Basecamp:
   export NSSA_WALLET_HOME_DIR=/path/to/seeded/wallet
   export NSSA_SEQUENCER_URL=http://127.0.0.1:3040
   open /Applications/LogosBasecamp.app    # macOS
   # or: ./LogosBasecamp-Desktop-vX.Y.Z.AppImage   # Linux
   ```

6. **Devnet deployment + RISC0_DEV_MODE=0 numbers.** Awaits the Logos
   team's devnet RPC URL. See `DEPLOYMENT.md` for the commands ready
   to go.

7. **Record the narrated video demo.** Script in `DEMO.md`. Recording
   happens after #5 + #6.

## Build (development, local)

```bash
# 1. Build the Rust FFI cdylib.
(cd ffi && cargo build --release)

# 2. Configure + build the C++ plugin (needs Qt6 in PATH).
cmake -B build -GNinja -DWHISTLEBLOWER_FFI_LIB_DIR="$PWD/ffi/target/release"
cmake --build build

# 3. Run the standalone preview app.
QML_PATH=$PWD/qml ./build/bin/whistleblower_app
```

The standalone app lets you exercise the QML + Backend signal flow without
a Basecamp host. The Storage/Delivery integration will fail-fast with a
clear error since `LogosAPI` is null in standalone mode.

## Build (nix, production)

The flake is at the workspace root (NOT `ui/`) because the FFI has
path-deps on `core`, `indexing`, `adapters/lez` that nix needs in scope.

```bash
ssh m4pro                                   # or any host with nix + Logos toolchain
git clone <this-repo> wb && cd wb           # workspace root
nix build .#ffi      # Rust cdylib (~3-4 min)
nix build .#plugin   # Qt plugin + standalone preview app
nix build .#lgx      # portable .lgx package — the spec deliverable
nix run  .#install   # copies plugin into Basecamp dev plugin dir
```

Verified end-to-end on m4pro 2026-05-04:
- `.#ffi`     → `lib/libwhistleblower_ffi.dylib` (5.6MB)
- `.#plugin`  → `lib/libwhistleblower_plugin.dylib` (467KB) + `bin/whistleblower_app` (preview)
- `.#lgx`     → `whistleblower-plugin.lgx` (2.4MB, darwin-arm64 variant)

The .lgx is then SCP'd back to the host machine for distribution:
```bash
scp m4pro:/nix/store/<hash>-whistleblower-plugin-lgx-0.1.0/whistleblower-plugin.lgx dist/
```

## Layout

```
ui/
├── manifest.json              Basecamp plugin manifest
├── metadata.json              Module metadata (deps, runtime requirements)
├── flake.nix                  Nix package + .lgx bundle
├── CMakeLists.txt             Standalone build (cmake -B build)
├── README.md                  This file
├── qml/
│   └── Main.qml               File picker + publish UI + progress bar
├── src/
│   ├── WhistleblowerPlugin.{h,cpp}    Plugin entry (loads QML + creates backend)
│   ├── WhistleblowerBackend.{h,cpp}   Qt logic + LogosAPI calls + Rust FFI bridge
│   └── main.cpp                       Standalone preview app
└── ffi/
    ├── Cargo.toml             Rust workspace member
    └── src/
        └── lib.rs             C ABI: anchor_one, query_by_cid*, compute_metadata_hash
```
