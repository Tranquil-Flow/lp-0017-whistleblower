# LP-0017 — Public LEZ Testnet Proof

**Primary on-chain evidence.** The `whistleblower-registry` program was deployed and exercised on the **public LEZ testnet** (`testnet.lez.logos.co`), not a local sequencer. Every hash below is independently re-verifiable from any machine with a `wallet` binary pointed at the testnet — see "Reproduce" at the end. The earlier local-sequencer runs (`REGISTRY_SPIKE.md`, `DEPLOYMENT.md`) are retained as historical corroboration only; localnet-only evidence is what closed PR #58, and this run exists to replace it.

```text
sequencer RPC:  https://testnet.lez.logos.co/   (public, no-auth, JSON-RPC over HTTPS POST)
explorer:       https://explorer.testnet.lez.logos.co/
network:        real consensus, RISC0_DEV_MODE=0 (public-transaction proving is sequencer-side)
date:           2026-06-03
```

## Version-pin landmine (why this guest is a fresh rc3 build)

The testnet runs LEZ **`v0.1.2` ≡ `v0.2.0-rc3`** (commit `cf3639d8`). The original LP-0017 submission pinned `nssa_core` to **`v0.2.0-rc1`** (`35d8df0d`); that binary will not execute on the testnet (`core/src/program.rs` differs by ~300 lines). Three issues were defused:

1. **LEZ deps re-pinned** in the workspace `Cargo.toml` — `nssa` / `nssa_core` / `wallet` / `common` / `sequencer_service_rpc` to `tag = "v0.2.0-rc3"`, `spel-framework` to the `chore/bump-lez-to-v0.2.0-rc3` branch (`31e52c52`, because spel's own `v0.2.0-rc.3` tag pins `nssa_core` back to rc1). Verified: `Cargo.lock` resolves `nssa_core` to `cf3639d8`, zero `35d8df0d`.
2. **`ruint` MSRV.** The rc3 graph pulled `ruint@1.18.0`, which requires rustc 1.90, but the risc0 guest-builder image (`r0.1.88.0`) ships rustc 1.88. Pinned down with `cargo update -p ruint --precise 1.17.0`.
3. **rc3 PDA API change.** rc3 removed the `From<(&ProgramId, &PdaSeed)> for AccountId` impl and replaced it with the explicit constructor `AccountId::for_public_pda(program_id, seed)`. Updated in the guest (`methods/guest/src/bin/whistleblower_registry.rs`), the adapter (`adapters/lez/src/lib.rs`), and the driver. Host and guest derive PDAs through the *same* function, so the guest's re-derivation check still matches.

## Guest binary (rc3 / testnet-matching)

```text
file:    target/riscv32im-risc0-zkvm-elf/docker/whistleblower_registry.bin
size:    439 KB
ImageID: 54c7f793caa540408ce2ca4c22051d78c466cd5ed8db607feedd19dcb749aa91
```

The ImageID is the on-chain program id.

## Deploy + execution model

`wallet deploy-program` is fire-and-forget — it discards the deploy tx hash. The driver `anchor_spike/src/bin/testnet_lifecycle.rs` instead builds a typed `nssa::ProgramDeploymentTransaction`, submits it, captures the hash, and polls `get_transaction`. The registry's anchor transactions are **public, unsigned** (`WitnessSet::for_message(&message, &[])`) — the program takes no signer; the testnet accepts and sequencer-proves them. No gas is charged to a fee payer (the registry is public-state PDA writes).

## Lifecycle (against `testnet.lez.logos.co`, 2026-06-03)

```text
program_id (hex) = 54c7f793caa540408ce2ca4c22051d78c466cd5ed8db607feedd19dcb749aa91
entry_pda(cid_a) = B1GxfUsX5hE73EFumBfdPXSTK7pJjPCmP7dnvEtibZ7i
entry_pda(cid_b) = 2qoQ8niS9UtSKRmgZH1XF7mgfyVhzwv43cS8dBnyT5wV

[0] deploy_program     tx=05781c3c5fa65d72d1ee9ee8f0964144f9a5688ef8ad14f445581e308026608f
[1] anchor_one(cid_a)  tx=9f6aee9cc97a62300780f0e576e76c61c4e1fb32bef5067d574a798a1a0de227
[2] anchor_one dup     tx=8f2fe8f103a9c6a7a65547e9244db9ef4a1d3ef42caf8067288316f2d920dfbc
[3] anchor_batch(a,b)  tx=f5fedf2910dad89c91a62ec257f7a722c638c07203fac914a9766cdfe148e22f
```

> The wallet's confirmation poll window (~45s) expired before inclusion on some txs ("NOT confirmed" in the raw run log), but **all four landed** — testnet inclusion is seconds-to-minutes and the poll is short. The chain-info verdicts and PDA readback below were taken after inclusion and are the authoritative result.

### Chain-info verdicts (queried live, post-inclusion)

| step | tx hash | `wallet chain-info transaction` verdict |
| --- | --- | --- |
| deploy_program | `05781c3c…608f` | `Some(ProgramDeployment)` |
| anchor_one(cid_a) | `9f6aee9c…de227` | `Some(Public)` |
| anchor_one dup | `8f2fe8f1…20dfbc` | `Some(Public)` |
| anchor_batch(a,b) | `f5fedf29…48e22f` | `Some(Public)` |

### PDA readback (pure chain reads, decode `account.data` as borsh `AnchorEntry`)

| PDA | cid | metadata_hash | anchor_timestamp | owner |
| --- | --- | --- | --- | --- |
| `B1GxfUsX…bZ7i` (cid_a) | `bafy-lp0017-testnet-18b597589606e650-alpha` | `0x11×32` | `1780495656451` | program `6hx6iSyX…` |
| `2qoQ8niS…T5wV` (cid_b) | `bafy-lp0017-testnet-18b597589606e650-bravo` | `0x22×32` | `1780495716310` | program `6hx6iSyX…` |

Each PDA decodes to a complete `AnchorEntry` (CID string + cid_hash + metadata_hash + timestamp) with **zero trailing bytes** — exact borsh.

## What is proved on the public testnet

| Proof | Status | Evidence |
| --- | --- | --- |
| Program deployed on public testnet | green | deploy tx → `Some(ProgramDeployment)`; ProgramId `54c7f793…aa91` |
| `anchor_one` writes a PDA-per-CID entry | green | cid_a PDA decodes the full `AnchorEntry` |
| Single-tx batch anchoring | green | `anchor_batch` confirmed; cid_b PDA populated in the same tx |
| **Idempotency invariant** (re-anchor = no-op, no overwrite) | green | cid_a kept its *original* `anchor_one` timestamp `1780495656451` even though it was re-included in the later batch (cid_b, anchored in that same batch, has the batch's later ts `1780495716310`). The batch's no-op on cid_a did not overwrite it. |
| public-tx execution model | green | unsigned public txs accepted + sequencer-proved; no fee payer |

## Reproduce (read-only, no build/faucet — needs only the `wallet` binary)

```bash
bash scripts/verify-testnet.sh
```

This points a throwaway wallet home at `https://testnet.lez.logos.co/` and re-queries each tx hash + decodes both entry PDAs straight from the sequencer. To re-run the full deploy + lifecycle from a clean build, see `anchor_spike/src/bin/testnet_lifecycle.rs` (set `NSSA_WALLET_HOME_DIR` + `LP0017_PROGRAM_BIN`).
