# Bugs Filed (and would-be-bugs we worked around)

Per LP-0017 spec line 110, this file lists upstream Logos issues we encountered while building. Some are filed, some are workarounds we documented here in lieu of an upstream report.

## Filed upstream

_None yet — pending review by Logos team via #builder-hub before opening issues, to avoid duplicating known reports._

## Worked around (candidates for upstream filing)

### 1. `spel-framework` cannot be a guest dependency on macOS arm64

**Repos affected:** `logos-co/spel`, `logos-co/spel-framework-core` (transitively).

**Symptom:** Adding `spel-framework` to a `methods/guest/Cargo.toml` causes `cargo risczero build` to fail at the `ring` C compile step with:

```
riscv32-unknown-elf-gcc: error: unrecognized command-line option '-arch'; did you mean '-march='?
riscv32-unknown-elf-gcc: error: unrecognized command-line option '-mmacosx-version-min=15.5'
error occurred in cc-rs: command did not execute successfully
```

**Root cause:** `spel-framework` and `spel-framework-core` both depend on `nssa_core` with `features = ["host"]`, which transitively pulls `bonsai-sdk → reqwest → rustls → ring`. When the guest is cross-compiled to `riscv32im-risc0-zkvm-elf`, `cc-rs` leaks the host's macOS `-arch arm64` and `-mmacosx-version-min` flags into the riscv32 cross-compiler, which doesn't understand them.

**Workaround we used:** drop `spel-framework` from the guest's deps and write the program against raw `nssa_core::program::{ProgramInput, ProgramOutput, AccountPostState, ...}` directly. Hand-write the IDL JSON (see `whistleblower-registry-idl.json`) since `spel generate-idl` only scans `#[lez_program]` annotations.

**Suggested fix:** spel-framework-core should split `host` into a separate feature group from the proc-macro support so guests can use `#[lez_program]` without dragging in the bonsai dep tree. Or document the workaround prominently in the framework README.

**Severity:** moderate — blocks any builder targeting LP prizes from macOS arm64 who tries to follow the SPEL framework's getting-started guide as written.

### 2. `Claim::Authorized` vs `Claim::Pda(seed)` is not documented in the SPEL/nssa READMEs

**Repos affected:** `logos-co/spel` (README), `logos-blockchain/logos-execution-zone` (`nssa_core::program::Claim` rustdoc).

**Symptom:** Following the SPEL framework's `whisper-wall` reference (which uses `Claim::Authorized`) leads to a confusing `InvalidProgramBehavior` error when applied to a PDA-claim use case. The sequencer log just says "failed execution check" — no indication that the claim type is the issue.

**Root cause:** `Claim::Authorized` requires the input account to be authorized by the transaction signer (`is_authorized(&account_id)` check in `nssa::validated_state_diff`). For PDA-owned accounts there is no signer, so the check fails. The correct claim type is `Claim::Pda(seed)`, which makes the runtime verify `account_id == AccountId::from(&program_id, &seed)` instead.

**Workaround we used:** switched to `AccountPostState::new_claimed_if_default(post, Claim::Pda(seed))`. Detection took ~30 minutes of staring at logs.

**Suggested fix:** add a short "claim cheat sheet" to the SPEL README distinguishing the two claim types and when to use each. Bonus: have the sequencer log include the specific NssaError variant in the failure message instead of just "failed execution check".

**Severity:** moderate — affects every builder writing a PDA-owned program.

### 3. LEZ template `lgs create` host-side runners reference removed API fields

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

### 4. `logos-blockchain-circuits` is an undocumented prerequisite for `cargo install --git logos-co/spel`

**Repos affected:** `logos-co/spel` (README install section), `logos-blockchain/logos-blockchain-circuits` (no install docs).

**Symptom:** Running the install order documented in `whisper-wall`'s README:

```bash
cargo install --git https://github.com/logos-co/logos-scaffold
cargo install --git https://github.com/logos-co/spel spel
```

The second step fails with:

```
panicked at zk/circuits/utils/src/lib.rs:35:9:
Could not find logos-blockchain-circuits directory. Please either:
1. Set the LOGOS_BLOCKCHAIN_CIRCUITS environment variable to point to your logos-blockchain-circuits directory, or
2. Place the logos-blockchain-circuits release at /Users/<user>/.logos-blockchain-circuits
```

**Workaround we used:** downloaded the macOS arm64 release tarball manually:

```bash
gh release download v0.4.2 -R logos-blockchain/logos-blockchain-circuits -p "*macos-aarch64.tar.gz"
tar -xzf logos-blockchain-circuits-v0.4.2-macos-aarch64.tar.gz
mv logos-blockchain-circuits-v0.4.2-macos-aarch64 ~/.logos-blockchain-circuits
```

**Suggested fix:** document the circuits prerequisite in spel's README install section, OR have spel's build script auto-download the circuits release matching its pinned LEZ commit.

**Severity:** moderate — blocks first-run install for new builders.

### 5. `cargo install cargo-risczero` fails on macOS without full Xcode

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
