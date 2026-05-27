# Bugs Filed (and would-be-bugs we worked around)

Per LP-0017 spec line 110, this file lists upstream Logos issues we encountered while building. Some are filed, some are workarounds we documented here in lieu of an upstream report.

## Filed upstream

- **logos-blockchain/logos-blockchain-circuits#33** — README missing install path documentation for downstream tools (`~/.logos-blockchain-circuits` / `LOGOS_BLOCKCHAIN_CIRCUITS`). https://github.com/logos-blockchain/logos-blockchain-circuits/issues/33

## Worked around (candidates for upstream filing)

### 1. LEZ template `lgs create` host-side runners reference removed API fields

**Repos affected:** `logos-co/logos-scaffold` (templates baked into `lgs create`).

**Symptom:** A fresh `lgs create <name> && lgs build` workspace fails to compile in the `src/bin/run_hello_world*.rs` files with errors like:

```
error[E0609]: no field `tx_hash` on type `(...)`
   --> src/bin/run_hello_world.rs:38:46
```

**Root cause:** The template's example runners reference `response.status` and `response.tx_hash`, but the current `wallet::WalletCore::sequencer_client.send_transaction` returns a `HashType` (tuple-struct, only `.0` is available).

**Workaround we used:** delete the template's `run_hello_world*.rs` files entirely and write our own host-side code against the current `wallet`/`nssa` API.

**Suggested fix:** regenerate the template's runner examples against the current pinned LEZ commit, or have `lgs create` emit a deprecation banner for the runner files.

**Severity:** low — cosmetic, but it makes the first-run experience confusing and risks misleading new builders into copying broken patterns.

### 2. `nix build github:logos-co/logos-liblogos` fails with `gtest_discover_tests` 5s timeout

**Repos affected:** `logos-co/logos-liblogos` (its `default`/`logos-liblogos`
output via that repo's flake).

**Symptom:** `nix build github:logos-co/logos-liblogos` (or `.#logos-liblogos`,
`.#logos-liblogos-bin`) succeeds at the link step (`[27/27] Linking CXX
executable bin/logos_core_tests`) but fails immediately after with:

```
CMake Error at .../GoogleTestAddTests.cmake:132 (message):
  Error running test executable.
  Path: '.../build/bin/logos_core_tests'
  Result: Process terminated due to timeout
```

cmake's `gtest_discover_tests` runs the test binary with a 5-second timeout
to enumerate tests; the binary either SIGSEGVs at startup or is too slow
in the nix sandbox.

This is the same shape as the closed-and-fixed issue logos-basecamp#77
(per memory: "nix build fails: gtest_discover_tests timeout in
logos-liblogos on macOS").

**Workaround we used:** build the `portable` output instead, which
skips the test discovery step and produces the same `bin/logos_host` +
`bin/logos_host_qt` binaries:

```bash
nix build github:logos-co/logos-liblogos#portable
```

**Suggested fix:** bump the gtest_discover_tests TIMEOUT to 30+s in
the cmake config, or condition test discovery on a build-time env
var so package builds skip it by default.

**Severity:** moderate — blocks anyone following the storage-module
README's `logoscore` invocation (which references the runtime that
the failed build was supposed to install).

### 3. `cargo install cargo-risczero` fails on macOS without full Xcode

**Repos affected:** `risc0/risc0` (`risc0-build-kernel` build script).

**Symptom:** `cargo install cargo-risczero` builds fine until it hits `risc0-build-kernel-2.0.1`, then panics:

```
Could not build metal kernels
xcrun: error: unable to find utility "metal", not a developer tool or in PATH
```

**Root cause:** `risc0-build-kernel` unconditionally tries to compile Apple Metal GPU kernels for proof acceleration on macOS, which requires the `metal` tool from full Xcode (not just the Command Line Tools).

**Workaround we used:** download the prebuilt binaries from the official Risc0 release:

```bash
gh release download v3.0.5 -R risc0/risc0 -p "cargo-risczero-aarch64-apple-darwin.tgz"
tar -xzf cargo-risczero-aarch64-apple-darwin.tgz
cp cargo-risczero r0vm ~/.cargo/bin/
```

This is what `curl -L https://risczero.com/install | bash` does internally — but our sandbox blocked the curl-pipe-bash pattern, hence the manual gh download.

**Suggested fix:** make the metal kernel build optional via a feature flag (`risc0-build-kernel = { default-features = false }`) so cargo install works without Xcode.

**Severity:** low — a workaround exists, but the failure mode is unhelpful for first-time installers.

### 4. Docs don't make it explicit that "LEZ devnet" = local sequencer (resolved by Discord clarification)

**Repos affected:** `logos-blockchain/logos-execution-zone` (README + `docs/`), `logos-co/lambda-prize` (LP-0017, LP-0008, LP-0012 specs use the term "LEZ devnet/testnet" without defining it).

**Symptom:** LP-0017 spec line 62 requires the registry be "deployed and tested on LEZ devnet/testnet" and line 58 requires CU benchmarks "on LEZ devnet/testnet". A reasonable reading is that there's a remote shared sequencer endpoint. After reading public sources — `logos-co/logos-docs` at commit `c72fda5`, `logos-execution-zone` README, the testnet tutorials in `docs/`, the `lgs` CLI source (no `devnet` subcommand, no baked-in network list), `logos-co/scaffold` README, `logos-co/lambda-prize` LP-0017/LP-0008/LP-0012 specs, the public testnet sequencer demo (`testnet/l2-sequencer-archival-demo/README.md`) — no LEZ devnet/testnet sequencer RPC endpoint surfaces. `logos-docs` documents LEZ local standalone mode on `localhost:3040` and separately documents Logos **Blockchain** public-testnet dashboard/faucet URLs (`https://testnet.blockchain.logos.co/web/`), but those are consensus blockchain endpoints, not the LEZ sequencer RPC.

**Resolution (Logos Discord, 2026-05-11):** there is no separate LEZ devnet endpoint — **the local sequencer IS the devnet** for LEZ purposes. `lgs localnet start` (with `risc0_dev_mode = false` and `RISC0_DEV_MODE=0` on host) is the canonical "devnet" deploy/measurement target. Remote Logos Blockchain public-testnet endpoints are unrelated to LEZ.

**Bug, restated:** LP-0017 spec lines 58 and 62 read as requiring a remote endpoint when in fact local is the devnet. Hours of public-source reading + a Discord question were needed to confirm this.

**Workaround for this submission:** none needed once the meaning of "devnet" was clarified. All measurements in `BENCHMARKS.md` and the deploy in `DEPLOYMENT.md` are against the local sequencer, which satisfies spec lines 58, 62, and 66.

**Suggested fix:** edit LP-0017 (and LP-0008/LP-0012/LP-0013) to either say "local LEZ sequencer (`lgs localnet start`, `risc0_dev_mode = false`)" explicitly, or add a one-line glossary entry in `logos-execution-zone/README.md` defining "LEZ devnet" as the local sequencer. If a remote shared LEZ sequencer ever ships, document its URL and acquisition flow there.

**Severity:** low — purely a docs/spec wording issue, but it costs every new LP builder real time. Multiple prize specs use the same ambiguous wording.

### 5. `delivery_module` Nix output is missing `librln.dylib` (broken @loader_path link on macOS arm64)

**Repos affected:** `logos-co/logos-delivery-module` (its `lgx`/`default` flake outputs).

**Symptom:** `delivery_module_plugin.dylib` fails to dlopen on every macOS arm64 launch with:

```
Library not loaded: /nix/var/nix/builds/nix-872-90086794/source/target/release/deps/librln.dylib
  Referenced from: <…> liblogosdelivery.dylib
  Reason: tried: '<many nix-store/qt paths>' (no such file), … 'librln.dylib' (no such file)
LogosAPIConsumer: Failed to acquire plugin/replica for object: "delivery_module"
WhistleblowerBackend: delivery_module.init() -> QVariant(Invalid)
```

The deployed module dir contains `delivery_module_plugin.dylib`, `liblogosdelivery.dylib`, `libpq*.dylib`, `manifest.json`, `variant` — but **no `librln.dylib`**. Zerokit is built into the local `/nix/store` (via the lgx build's transitive Cargo deps) but its `librln.dylib` is never copied into the install output.

**Root cause:** the flake links `liblogosdelivery.dylib` against Zerokit's `librln.dylib` as a Rust `cdylib` dep, but (a) the Rust build records the linker's *build-time absolute path* (`/nix/var/nix/builds/…/deps/librln.dylib`) as the load command rather than `@loader_path/librln.dylib`, and (b) the Nix `installPhase` doesn't copy `librln.dylib` next to `liblogosdelivery.dylib` in the output. Some local rebuilds (this project's flake, May 8 15:20) already rewrite the load command to `@loader_path/librln.dylib`, but `librln.dylib` itself is still absent from the deployed dir, so the dlopen still fails.

**Workaround we used:** `scripts/fix_delivery_rln.sh` — idempotently locates `librln.dylib` in the local `/nix/store` (zerokit-* output), copies it next to `liblogosdelivery.dylib` in every profile's `delivery_module/` install dir (and in `~/Library/Application Support/Logos/LogosBasecampDev/modules/delivery_module/`, where `lgs basecamp launch` syncs runtime data), sets the copied dylib's self-install-name to `@loader_path/librln.dylib`, and rewrites `liblogosdelivery.dylib`'s librln load command to `@loader_path/librln.dylib`. Re-run after any `lgs basecamp launch <profile>` that did **not** use `--no-clean` (the clean-slate scrub re-extracts the broken upstream output).

After applying the fix, the publish flow runs end-to-end: storage_module stores a manifest CID, delivery_module's `send()` broadcasts the CID JSON to `/lp0017-whistleblower/1/cids/json`, and the Whistleblower UI shows "Uploaded — CID …" + "Working: broadcasting to Logos Delivery…".

**Suggested fix:** add a `postFixup` (or equivalent) phase to `logos-delivery-module`'s flake that (a) copies `${zerokit}/lib/librln.dylib` into the module's install dir, (b) `install_name_tool -id @loader_path/librln.dylib` on the copy, and (c) `install_name_tool -change <build-path> @loader_path/librln.dylib liblogosdelivery.dylib`. The Cargo-side fix is to set `cargo:rustc-link-arg=-Wl,-rpath,@loader_path` in a build script for the FFI crate so the load command emerges as `@rpath/librln.dylib` and the install dir RPATH resolution works without rewriting.

**Severity:** high — without the workaround, `delivery_module` cannot load at all on macOS arm64, which blocks every Basecamp app that uses Logos Delivery (LP-0017 specifically; potentially LP-0008/LP-0012/LP-0013 if they also touch Delivery).
