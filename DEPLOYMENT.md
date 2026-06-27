# Deployment

## Build the program

```bash
lgs build
```

Produces `target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin` (~417KB).

The program's deterministic ID is computed from the binary's image hash:

```bash
spel inspect target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin
```

This prints a stable `ProgramId` in three encodings (decimal `[u32; 8]`, hex string, ImageID hex bytes). The hex string is what `lgs deploy` reports as `program_id`.

## Local sequencer (development)

```bash
# Start a local LEZ sequencer (RPC on :3040, RISC0_DEV_MODE=true).
lgs localnet start

# Deploy the program.
lgs deploy --program-path \
  target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin

# Stop when done.
lgs localnet stop
```

Sequencer logs: `.scaffold/logs/sequencer.log`.
Wallet state: `.scaffold/wallet/`. Pre-seeded accounts:
- `Public/2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2`
- `Public/CbgR6tj5kWx5oziiFptM7jMvrQeYY3Mzaao6ciuhSr2r`
- `Private/9DGDXnrNo4QhUUb2F8WDuDrPESja3eYDkZG5HkzvAvMC`
- `Private/A6AT9UvsgitUi8w4BH43n6DyX1bK37DtSCfjEWXQQUrQ`

The local sequencer runs `RISC0_DEV_MODE=true` by default — proofs are stubbed. The submission video MUST switch to `RISC0_DEV_MODE=0` for real proof generation; see `DEMO.md`.

## Public LEZ testnet (primary deployment)

**Status: DEPLOYED.** The registry is live on the public LEZ testnet — full hashes, `chain-info` verdicts, and PDA decodes are in [`TESTNET_PROOF.md`](TESTNET_PROOF.md); re-verify any time with `bash scripts/verify-testnet.sh` (read-only).

```
sequencer RPC:  https://testnet.lez.logos.co/   (public, no-auth, JSON-RPC over HTTPS POST)
explorer:       https://explorer.testnet.lez.logos.co/
program id:     1c8a08b62f1cf7b4a92693502bb5522372d937cfe9aa5a60a98a3dac6b5908f7   (= ImageID)
deploy tx:      db634916b48628e8f40b42021858f7f6731360dc48f5baa37a04edcd75cc598c   (Some(ProgramDeployment))
anchor_one:     4de6176a58dade3188737e88a9e59b9c922c403452bb2dbc6e8dc66d0b0f3a78   (Some(Public))
anchor_one dup: 7114bce11b90a05c836a5d920da4a8fcb188395a7e9f470be006f66652ad0546   (idempotent no-op)
anchor_batch:   05e7b3763d659ba9cbc1a3b2488edfd6e1d515a2f6468f5f34fcb976c7c70abf   (Some(Public))
```

### Version-pin landmine (must match for the binary to execute)

The current public-testnet refresh was produced with the LEZ v0.2.0 client/runtime path. The guest, adapter, and driver are pinned to rc3 (`nssa`/`nssa_core`/`common`/`wallet`/`sequencer_service_rpc` → `tag = v0.2.0-rc3`; `spel` → branch `chore/bump-lez-to-v0.2.0-rc3` = `31e52c52`, because spel's own `v0.2.0-rc.3` tag pins `nssa_core` back to rc1). `ruint` is pinned to `1.17.0` (the rc3 graph otherwise pulls `1.18.0`, which needs rustc 1.90 > the risc0 guest-builder's 1.88). Verify: `Cargo.lock` resolves `nssa_core` to `cf3639d8`, zero `35d8df0d`.

### Reproduce the deploy + lifecycle (fresh)

`wallet deploy-program` is fire-and-forget (it discards the tx hash). The driver `anchor_spike/src/bin/testnet_lifecycle.rs` instead builds a typed `ProgramDeploymentTransaction`, submits it, captures the hash, and polls — then runs the anchor/dup/batch lifecycle, capturing every hash.

```bash
# 1. Build the rc3 guest (heavy risc0 build — route to a build host):
cargo risczero build --manifest-path methods/guest/Cargo.toml
#    -> a release RISC-V guest wrapped as a RISC Zero ProgramBinary (ImageID 1c8a08b6…)

# 2. Fund an initialised testnet account (one-time):
wallet config set sequencer_addr https://testnet.lez.logos.co/
wallet check-health                         # expect ✅
wallet account new public                   # note Public/<id>
wallet pinata claim --to Public/<id>

# 3. Deploy + run the lifecycle, capturing hashes:
export NSSA_WALLET_HOME_DIR=<funded testnet wallet home>
export LP0017_PROGRAM_BIN=$PWD/target/riscv32im-risc0-zkvm-elf/docker/whistleblower_registry.bin
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
cargo run --release -p anchor-spike --bin testnet_lifecycle
#    or: bash scripts/demo.sh --full
```

Public-tx execution is **sequencer-proved** (no host-side proving in the anchor path) and charges no gas to a fee payer — see `BENCHMARKS.md`. CU is the deterministic executor-cycle cost of the deployed ELF (the testnet does not persist per-tx CU — filed upstream, `BUGS_FILED.md` #7).

The IDL (`whistleblower-registry.idl.json`) is generated from the same program shape via `bash scripts/regen-idl.sh` and decodes on-chain entries with `spel inspect`.

## Verification

After deployment, exercise the registry end-to-end:

```bash
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet ./target/release/anchor_spike
```

Expected output: all four spike behaviours pass plus the bonus query, finishing with:

```
✅ Task 1.0B spike PASSED on PDA-per-CID design
   Anchored 12 fresh CIDs total this run.
```

For broader verification, run the live integration suite:

```bash
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet \
  cargo test -p whistleblower-lez-adapter --release -- --ignored --nocapture
```

Expected: 2 tests pass (~30s).

## Rolling back

The PDA-per-CID design has no rollback story — the registry is append-only and entries are owned by the program forever once claimed. If a bug ships, redeploy with a new build (new program_id) and migrate; old entries remain readable but are owned by the old program. The CID itself is a hash of the file contents, so existing publications never need re-uploading.
