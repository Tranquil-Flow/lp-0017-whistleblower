# Demo

The submission video must be a narrated walkthrough (a silent screencast is explicitly insufficient — see `~/Projects/logos-basecamp/SPECS/README.md` §"Demo requirements"). This file is the script.

> **Testnet-first.** The registry is deployed on the **public LEZ testnet** (`testnet.lez.logos.co`, program `54c7f793…aa91`). The recording leads with the live testnet evidence; the local sequencer appears only as optional white-box corroboration. The previously linked recording (`youtu.be/lMu25io5K-k`) predates the testnet deploy and shows the localnet flow — it must be re-recorded against the testnet before resubmission.

## What the spec requires the video to show

From `LP-0017.md` §Submission Requirements:
1. A file uploaded and immediately findable via the Logos Delivery topic.
2. The batch anchor tool picking up the broadcast CID and anchoring it on-chain.
3. The on-chain registry confirming the CID registration.
4. Terminal output showing proof generation with `RISC0_DEV_MODE=0` so reviewers know real proving was used (not the dev shortcut).

## Honesty framing (state this in the narration)

- The **on-chain registry** is fully real and headless on the public testnet — deploy, anchor, idempotent re-anchor, and batch are confirmed on chain and independently re-verifiable.
- **Upload → broadcast** (Logos Storage + Delivery) runs inside the **Basecamp UI plugin** via the real in-process `LogosAPIClient`. Real Logos Delivery is Waku + RLN behind a Qt `logos_host` module over QtRemoteObjects, so it runs in Basecamp (GUI), not headless.
- The **batch tool** consumes the broadcast envelope. In the recording it is fed that envelope via `--envelopes-from` (the same `MetadataEnvelopeV1` the UI just broadcast over Delivery) because a headless Waku subscription is a separate integration (`adapters/logos/README.md`). Everything downstream of the envelope — dedupe, batching, idempotent anchoring against the live testnet — is real. Say this plainly; do not imply the CLI subscribes to Waku headless.

## Walkthrough script

### Scene 0 — `RISC0_DEV_MODE=0` + real proof generation (~45s)

LEZ wallets generate Risc0 proofs on the **PrivacyPreserving** path (token transfers, faucet claims). Our anchor flow uses **Public** transactions (the registry is public-by-design — see the README "Architecture & key decisions" section), which are sequencer-proved, so the host-side prover does not fire when we anchor. To satisfy spec line 67 ("show terminal output including proof generation"), run the faucet claim early — it is a privacy-preserving tx and emits visible Risc0 prover stages under `RISC0_DEV_MODE=0`.

```bash
env | grep RISC0_DEV_MODE          # show RISC0_DEV_MODE=0
wallet config set sequencer_addr https://testnet.lez.logos.co/
wallet pinata claim --to <our-account-id>
```

Expected on-screen — a visible delay with prover stages:

```
risc0_zkvm::host::server::prove::executor: execution complete
risc0_zkvm::host::server::prove::recursion: lift
risc0_zkvm::host::server::prove::recursion: join
risc0_zkvm::host::server::prove: receipt complete
```

Narrate: "this is the real Risc0 proving stack on the public testnet — line 67 asks us to show this; the same prover handles every privacy-preserving tx on LEZ. Our anchors are Public txs, sequencer-proved."

### Scene 1 — Architecture intro (~30s)

Walk through the design (README "Architecture & key decisions" + `REGISTRY_SPIKE.md`):
- The 4-layer breakdown (Basecamp app → adapter layer → indexing module → LEZ program / batch CLI).
- The PDA-per-CID storage decision and why (idempotency, unbounded capacity, O(1) anchor cost).
- Why the indexing module is Qt-free (reusable by any other Logos app).

### Scene 2 — Upload + broadcast in Basecamp (~60s)

Open the Whistleblower Basecamp UI. Pick a small (~10KB) markdown file. Fill metadata (title, description, tags). Click Upload.

Expected on-screen:
```
Uploading to Logos Storage...
  ↳ storageUploadInit returned sessionId=upload-1
  ↳ uploadChunk x3
  ↳ storageUploadDone: CID = bafy<...>
Broadcasting envelope to /lp0017-whistleblower/1/cids/json
  ↳ messageSent: hash=<...>
✓ Document published. CID is now discoverable.
```

Narrate: "real Logos Storage upload and real Logos Delivery broadcast, in-process via `LogosAPIClient`. The CID is now on the Delivery topic." Copy the broadcast `MetadataEnvelopeV1` into an envelope file for the next scene (the bytes that went over Delivery).

### Scene 3 — Batch tool anchors the CID on the testnet (~60s)

Run the **real** `whistleblower-batch` binary against the **deployed testnet program** — no `--mock-delivery`. Feed it the envelope from Scene 2 (here via `--envelopes-from`; in production the same envelope arrives over the live Delivery subscription):

```bash
./scripts/demo.sh --batch
# which runs, against testnet.lez.logos.co with the deployed program:
#   whistleblower-batch --topic /lp0017-whistleblower/1/cids/json \
#     --batch-size 3 --batch-interval-secs 10 \
#     --program-bin target/riscv32im-risc0-zkvm-elf/docker/whistleblower_registry.bin \
#     --envelopes-from demo/sample-envelopes.jsonl
```

Expected log lines:
```
whistleblower-batch starting:
  topic = /lp0017-whistleblower/1/cids/json
  delivery = FILE replay (3 envelope(s) from demo/sample-envelopes.jsonl)
[whistleblower-batch] anchored batch: 3/3 entries
[whistleblower-batch] clean exit.
```

Narrate: "the batch tool is permissionless — anyone can run it; it dedupes, batches, and anchors idempotently against the on-chain registry. The CID source here is the broadcast envelope fed from a file; the on-chain anchoring is real against the public testnet."

### Scene 4 — Registry confirms the CID on chain (~45s)

Re-verify the deployed lifecycle straight from the public sequencer (no transaction, no GUI):

```bash
./scripts/demo.sh           # verify mode — re-queries every deployed tx + decodes the entry PDAs
```

Expected: every tx returns its `Some(ProgramDeployment)` / `Some(Public)` verdict and both entry PDAs decode to a complete `AnchorEntry`. Then show a single-entry query without a transaction:

```bash
spel inspect <pda-base58> --idl whistleblower-registry.idl.json --type AnchorEntry
```

Narrate: "no transaction needed — anyone with the CID hash derives the PDA and reads the entry directly off the public testnet."

### Scene 5 — CU benchmark (~20s)

Show the `BENCHMARKS.md` framing:

```
single-CID anchor : deterministic deployed-ELF executor cycles (≈ on-chain CU)
50-CID batch       : ~50× single + fixed per-tx overhead; per-CID cost ~constant
```

Narrate: "the public testnet does not persist a per-tx compute-unit value (we filed that upstream), so CU is the executor-cycle cost of the deployed ELF — deterministic, therefore equal to the on-chain cost. Absolute rc3 figures are a pending re-measure; the per-CID shape is the headline." (Do **not** claim the testnet exposes CU.)

### Scene 6 — Wrap (~15s)

- "All code is at github.com/Tranquil-Flow/lp-0017-whistleblower under MIT/Apache-2.0."
- "The registry is deployed on the public LEZ testnet — program `54c7f793…`, explorer at explorer.testnet.lez.logos.co."
- "Upstream issues we filed/queued during the build: see `BUGS_FILED.md`."

Total run time target: 3–4 minutes.

## Recording checklist

- [ ] Terminal font size large enough to read at 1080p (18–22pt).
- [ ] `RISC0_DEV_MODE=0` is visible in `env | grep RISC` near the start.
- [ ] `wallet config` shows `sequencer_addr = https://testnet.lez.logos.co/` (real network on camera).
- [ ] Audio levels normalised (no clipping).
- [ ] Captions / subtitles for accessibility.
- [ ] No `/Users/evinova/...` paths visible — use `$PWD` or repo-relative paths.
- [ ] The batch scene is narrated honestly: envelope fed from a file; on-chain anchoring is real on the testnet.

## Reproducibility script

`scripts/demo.sh` is the reproducible terminal side of the demo:

- **default (verify)** — re-verifies the deployed deploy/anchor/dup/batch lifecycle on the public testnet, read-only, clone-and-run safe (only needs `curl`+`python3`, or `wallet` for the richer PDA decode).
- **`--batch`** — runs the real `whistleblower-batch` tool against the deployed testnet program from an envelope file (no mock).
- **`--full`** — fresh build + deploy + lifecycle on the testnet, then `--batch`.
- **`--localnet`** — the spec-literal `RISC0_DEV_MODE=0` local-sequencer path, retained as corroboration.

The Basecamp upload/broadcast scene (Scene 2) is human-driven inside Basecamp because file selection and module loading are GUI interactions.

Validate demo artifacts with:

```bash
python3 scripts/validate_demo_artifacts.py
bash -n scripts/demo.sh
```
