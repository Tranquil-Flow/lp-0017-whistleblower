# Whistleblower Basecamp UI plugin (LP-0017)

Qt6 + QML + Rust FFI plugin for the Logos Basecamp app. Lets a user pick a
document, upload it to Logos Storage, broadcast the `(CID, metadata)`
envelope over Logos Delivery, and anchor the CID on the LEZ registry.

## Status

**Scaffold**, not yet end-to-end functional. Modeled on `whisper-wall/ui/`
which is the canonical reference for this pattern.

| Component | State |
|---|---|
| `manifest.json` + `metadata.json` | ✅ Configured for `whistleblower` plugin name + `storage_module`/`delivery_module` deps |
| `qml/Main.qml` | ✅ File picker + metadata form + 4-stage progress bar + Publish/Anchor buttons |
| `ffi/` (Rust C ABI) | ✅ Compiles + 4 unit tests pass. Three exported FFI calls: `whistleblower_anchor_one`, `whistleblower_query_by_cid`, `whistleblower_compute_metadata_hash` |
| `src/WhistleblowerPlugin.{h,cpp}` | ✅ Same shape as `WhisperWallPlugin` — Qt plugin entry that constructs the backend + QQuickWidget |
| `src/WhistleblowerBackend.{h,cpp}` | 🟡 Shape complete. `anchorLast()` calls the Rust FFI cleanly. `uploadToStorage()` and `broadcastEnvelope()` are STUBBED — they emit a clear "not yet wired" error. See TODO(Phase-1.7-runtime) markers. |
| `src/main.cpp` | ✅ Standalone preview app for manual UI iteration without Basecamp |
| `CMakeLists.txt` | ✅ Builds the Qt plugin + a standalone preview app, linking against the Rust FFI cdylib |
| `flake.nix` | ✅ Nix package + .lgx bundle. Cargo-lock hashes need recomputing on first build (see comment in flake.nix) |

## What's left for "demo works end-to-end"

1. **Wire `uploadToStorage()` and `broadcastEnvelope()`** to actually call
   the storage_module / delivery_module via `LogosAPI`. Pattern TBD —
   whisper-wall doesn't use the LogosAPI handle (it's on-chain only),
   so we don't have a worked example. Probable shape:
   ```cpp
   QObject* storage = m_api->getModule("storage_module");
   connect(storage, SIGNAL(storageUploadDone(QString, QString)),
           this, [...](QString sessionId, QString cid) { ... });
   QMetaObject::invokeMethod(storage, "uploadUrl",
       Q_ARG(QUrl, QUrl::fromLocalFile(filePath)),
       Q_ARG(int, 65536));
   ```
   Need to read `LogosAPI`'s actual interface — see
   `logos-co/logos-cpp-sdk` or build the Basecamp host with an inspector
   on first launch.

2. **Recompute the FFI cargo-lock hashes** in `flake.nix` for our
   transitive deps (we have `document-indexing` instead of whisper-wall's
   `spel-client-gen`, so the hashes differ). Run `nix build .#ffi` once
   and copy the hashes nix complains about into the `outputHashes` map.

3. **Build + install via nix** (must be done on a host with the Logos
   toolchain — m4pro is set up):
   ```bash
   ssh m4pro
   cd ~/whistleblower-ui
   nix build ./ui#install
   ```

4. **Wire `whistleblower-batch`** to drop `--mock-delivery` once the real
   Logos Delivery integration is in. The CLI should subscribe directly
   via the Rust delivery adapter (a counterpart to LezRegistryClient
   that wraps a logos_host process — or via a shared LogosAPI handle
   exposed by Basecamp).

5. **Re-record the demo video** showing real upload → broadcast → batch
   anchor → registry query, with `RISC0_DEV_MODE=0` visible.

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

## Build (nix, production, on m4pro)

```bash
ssh m4pro
git clone <this-repo> wb && cd wb/ui
nix build           # builds plugin + ffi
nix build .#lgx     # produces a portable .lgx for distribution
nix run .#install   # copies into ~/.local/share/Logos/LogosBasecampDev/plugins/whistleblower/
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
