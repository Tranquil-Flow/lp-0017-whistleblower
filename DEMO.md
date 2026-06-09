# Demo

The submission video is a narrated walkthrough of the LP-0017 document publishing and indexing flow. This file is the human recording script; `scripts/record-final-video.sh` is the terminal companion script.

> **Testnet-first.** The registry is deployed on the **public LEZ testnet** (`testnet.lez.logos.co`, program `54c7f793…aa91`). The recording leads with live testnet evidence. Local-sequencer artifacts are used only for compute/proof development traces where the public testnet does not expose per-transaction executor logs.

## What the spec requires the video to show

From `LP-0017.md` §Submission Requirements:
1. A file uploaded and immediately findable via the Logos Delivery topic.
2. The batch anchor tool picking up the broadcast CID and anchoring it on-chain.
3. The on-chain registry confirming the CID registration.
4. Terminal output showing `RISC0_DEV_MODE=0` proof-mode evidence, demonstrating that the workflow is not relying on the development shortcut.

## Honesty framing (state this in the narration)

- The **on-chain registry** is fully real and headless on the public testnet — deploy, anchor, idempotent re-anchor, and batch are confirmed on chain and independently re-verifiable.
- **Basecamp packaging** is shown as build/install/smoke evidence: the Whistleblower `.lgx` package declares `storage_module` as the required dependency, keeps `delivery_module` optional/best-effort, installs into scaffold-managed Alice/Bob Basecamp profiles, and a Basecamp smoke launch discovers the installed runtime modules under `RISC0_DEV_MODE=0` project evidence. The current experimental Basecamp GUI shell is not used as a load-bearing claim.
- The **batch tool** consumes a `MetadataEnvelopeV1` via `--envelopes-from` because a headless Waku subscription is a separate integration (`adapters/logos/README.md`). Everything downstream of the envelope — dedupe, batching, idempotent anchoring against the live testnet — is real. Say this plainly; do not imply the CLI subscribes to Waku headless.

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

### Scene 2 — Basecamp package + Delivery envelope evidence (~60s)

Show the package surfaces and scaffold profile install evidence rather than depending on the experimental Basecamp GUI shell:

```bash
python3 - <<'PY'
from pathlib import Path
import json
for rel in ['metadata.json', 'ui/metadata.json', 'ui/manifest.json']:
    data = json.loads(Path(rel).read_text())
    print(rel, data.get('name'), data.get('dependencies', []))
PY
python3 - <<'PY'
from pathlib import Path
profile = Path('.scaffold/basecamp/profiles/alice/xdg-data/Logos/LogosBasecampDev')
for rel in ['modules/storage_module/manifest.json', 'plugins/whistleblower/manifest.json']:
    p = profile / rel
    assert p.exists(), p
    print('installed', rel)
optional = profile / 'modules/delivery_module/manifest.json'
print('optional delivery installed:', optional.exists())
PY
```

Narrate: "the Whistleblower `.lgx` package requires Storage and keeps Delivery best-effort so upload and anchoring cannot be blocked by Delivery startup. The envelope format shown here is what the batch indexer consumes. Current Basecamp GUI-shell warnings are unrelated, so they are not used as evidence."

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

Narrate: "the public testnet does not persist a per-tx compute-unit value, so CU is reported from deterministic executor-cycle measurements of the deployed ELF. The important result is the single-anchor and batch per-CID cost shape." (Do **not** claim the testnet exposes CU.)

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

The Basecamp evidence scene uses package metadata, `.lgx` build/install output, scaffold-profile manifests, and a smoke-launch log that discovers the installed required Storage module plus optional Delivery availability.

Validate demo artifacts with:

```bash
python3 scripts/validate_demo_artifacts.py
bash -n scripts/demo.sh
```
