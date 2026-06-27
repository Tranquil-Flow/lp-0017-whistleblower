# LP-0017 — Public LEZ Testnet Proof

**Primary on-chain evidence.** The `whistleblower-registry` program is deployed and exercised on the **current public LEZ testnet** (`testnet.lez.logos.co`), not a local sequencer. The 2026-06-27 refresh below replaces the older June 3 hashes that no longer resolve after the Logos-side public-testnet reset.

```text
sequencer RPC:  https://testnet.lez.logos.co/   (public, no-auth, JSON-RPC over HTTPS POST)
explorer:       https://explorer.testnet.lez.logos.co/
network:        real consensus, RISC0_DEV_MODE=0 (public-transaction proving is sequencer-side)
date:           2026-06-27
client/runtime: LEZ v0.2.0 current client path (`lee`/`lee_core`/`wallet`)
```

## Current v0.2.0 refresh

Reviewer `xAlisher` correctly observed that the earlier deploy / `anchor_one` / duplicate / `anchor_batch` transactions returned `result: null` after a public-testnet reset. The registry was therefore rebuilt for the current LEZ v0.2.0 runtime path, wrapped as a RISC Zero `ProgramBinary`, redeployed, and re-exercised through the same lifecycle.

```text
ProgramId / ImageID (hex):    1c8a08b62f1cf7b4a92693502bb5522372d937cfe9aa5a60a98a3dac6b5908f7
ProgramId / ImageID (base58): 2vQVdFEVW79Xw3FCxYkeEyF52Ykqitoh57jcQ1NBxGv2
ProgramBinary SHA-256:        29d5c10260b5902bb0951743af9a6d3f0b8570bd4f686fa06cef613e632e251c
```

The release guest ELF was wrapped as a `risc0_binfmt::ProgramBinary` before deployment. Raw legacy guest ELFs are not evaluator-safe on the current endpoint.

## Lifecycle (against `testnet.lez.logos.co`, 2026-06-27)

```text
program_id (hex) = 1c8a08b62f1cf7b4a92693502bb5522372d937cfe9aa5a60a98a3dac6b5908f7
program_id (base58) = 2vQVdFEVW79Xw3FCxYkeEyF52Ykqitoh57jcQ1NBxGv2
cid_a = bafy-lp0017-v020-18bcea4c55bd1170-alpha
cid_b = bafy-lp0017-v020-18bcea4c55bd1170-bravo
cid_a_hash = 56f36df11116608d4a2005475de39049583a72bdecfdc77f8512e5e95e374e95
cid_b_hash = c7ac54044a8df5b38c5aea1df0eac33133d7f3402676e152ef182ad553080fcb
entry_pda(cid_a) = 4MBGdz8UULLERvijXheb54PwSzYGRQDyPhcHG3Ga57SE
entry_pda(cid_b) = 6eBL3uES8uJ9eYR4fCTkjMmLZnSphEmDMk753aGz4xrF

[0] deploy_program     tx=db634916b48628e8f40b42021858f7f6731360dc48f5baa37a04edcd75cc598c
[1] anchor_one(cid_a)  tx=4de6176a58dade3188737e88a9e59b9c922c403452bb2dbc6e8dc66d0b0f3a78
[2] anchor_one dup     tx=7114bce11b90a05c836a5d920da4a8fcb188395a7e9f470be006f66652ad0546
[3] anchor_batch(a,b)  tx=05e7b3763d659ba9cbc1a3b2488edfd6e1d515a2f6468f5f34fcb976c7c70abf
```

### Transaction verdicts

| step | tx hash | verdict |
| --- | --- | --- |
| deploy_program | `db634916b48628e8f40b42021858f7f6731360dc48f5baa37a04edcd75cc598c` | included (`Some(ProgramDeployment)`) |
| anchor_one(cid_a) | `4de6176a58dade3188737e88a9e59b9c922c403452bb2dbc6e8dc66d0b0f3a78` | included (`Some(Public)`) |
| anchor_one dup | `7114bce11b90a05c836a5d920da4a8fcb188395a7e9f470be006f66652ad0546` | included (`Some(Public)`) |
| anchor_batch(a,b) | `05e7b3763d659ba9cbc1a3b2488edfd6e1d515a2f6468f5f34fcb976c7c70abf` | included (`Some(Public)`) |

### PDA readback

| PDA | cid | cid_hash | metadata_hash | anchor_timestamp | owner |
| --- | --- | --- | --- | --- | --- |
| `4MBGdz8UULLERvijXheb54PwSzYGRQDyPhcHG3Ga57SE` | `bafy-lp0017-v020-18bcea4c55bd1170-alpha` | `56f36df11116608d4a2005475de39049583a72bdecfdc77f8512e5e95e374e95` | `0x11×32` | `1782557185761` | program `2vQVdFEVW79Xw3FCxYkeEyF52Ykqitoh57jcQ1NBxGv2` |
| `6eBL3uES8uJ9eYR4fCTkjMmLZnSphEmDMk753aGz4xrF` | `bafy-lp0017-v020-18bcea4c55bd1170-bravo` | `c7ac54044a8df5b38c5aea1df0eac33133d7f3402676e152ef182ad553080fcb` | `0x22×32` | `1782557304261` | program `2vQVdFEVW79Xw3FCxYkeEyF52Ykqitoh57jcQ1NBxGv2` |

Each PDA decodes to a complete `AnchorEntry` (CID string + cid_hash + metadata_hash + timestamp) with zero trailing bytes under the current verifier format.

## What is proved on the public testnet

| Proof | Status | Evidence |
| --- | --- | --- |
| Program deployed on current public testnet | green | deploy tx included; ProgramId `1c8a08b6…08f7` |
| `anchor_one` writes a PDA-per-CID entry | green | cid_a PDA decodes the full `AnchorEntry` |
| Single-tx batch anchoring | green | `anchor_batch` confirmed; cid_b PDA populated in the same tx |
| Idempotency invariant | green | `anchor_one_dup` confirms against the already-created cid_a PDA, and the later batch leaves cid_a's original timestamp intact while creating cid_b |
| public-tx execution model | green | unsigned public txs accepted + sequencer-proved; no fee payer |

## Reproduce (read-only)

```bash
bash scripts/ci-verify-testnet.sh
bash scripts/verify-testnet.sh   # richer PDA decode; needs current LEZ v0.2.0 `wallet` on PATH
```

`ci-verify-testnet.sh` is curl-only and fails closed if any current transaction hash disappears. `verify-testnet.sh` additionally uses `wallet account get --raw` to decode the two entry PDAs.

## Historical evidence superseded by reset

The earlier 2026-06-03 program `54c7f793caa540408ce2ca4c22051d78c466cd5ed8db607feedd19dcb749aa91` and transactions `05781c3c…`, `9f6aee9c…`, `8f2fe8f1…`, and `f5fedf29…` are historical only. They are intentionally not cited as current-live proof because they now return `result: null` on the public sequencer after reset.
