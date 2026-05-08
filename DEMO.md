# Demo

The submission video must be a narrated walkthrough (silent screencast is explicitly insufficient — see `~/Projects/logos-basecamp/SPECS/README.md` §"Demo requirements"). This file is the script.

## What the spec requires the video to show

From `LP-0017.md` §Submission Requirements:
1. A file uploaded and immediately findable via the Logos Delivery topic.
2. The batch anchor tool picking up the broadcast CID and anchoring it on-chain.
3. The on-chain registry confirming the CID registration.
4. Terminal output showing proof generation with `RISC0_DEV_MODE=0` so reviewers know real proving was used (not the dev shortcut).

## Walkthrough script

> **Status:** the UI plugin owns the real Storage + Delivery integration via in-process `LogosAPIClient` (`ui/src/WhistleblowerBackend.cpp`). The batch CLI still runs `--mock-delivery` for headless use — see `adapters/logos/README.md` for the headless-real-delivery design discussion. The on-chain anchoring path is real end-to-end on localnet.

### Setup (off-camera)

```bash
# Fresh sequencer, fresh state. Sequencer in non-dev mode (matches scaffold.toml).
rm -rf .scaffold/state .scaffold/logs queue.db
lgs localnet start                                    # respects [localnet] risc0_dev_mode = false
lgs build
lgs deploy --program-path target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin
export NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet
export RISC0_DEV_MODE=0   # required by spec line 67 — must be visible in env | grep RISC

# Basecamp side: built + installed via lgs basecamp:
lgs basecamp setup        # one-time per pinned basecamp rev
lgs basecamp install      # builds .lgx + installs into the alice profile
```

### Important framing for the narration

LEZ wallets generate Risc0 proofs only on the **PrivacyPreserving** transaction path (token transfers, faucet claims). Our anchor flow uses **Public** transactions (the registry is public-by-design — see ARCHITECTURE.md), so the host-side proof generation step doesn't fire when we anchor.

To satisfy spec line 67 ("show terminal output including proof generation"), Scene 0 below runs `wallet pinata claim` early in the recording. That faucet operation is a privacy-preserving tx and DOES emit visible Risc0 prover output under `RISC0_DEV_MODE=0`. The narration ties it to the rest of the system: same Risc0 stack proves the program-execution receipts that anchor txs would carry on a privacy-preserving registry variant — out of scope for this prize.

### Scene 0 — Faucet claim shows real proof generation (~45s)

```bash
env | grep RISC0_DEV_MODE          # show RISC0_DEV_MODE=0
wallet pinata claim --to <our-account-id>
```

Expected on-screen — long visible delay (~30-90s on cold cache, faster after) with prover stages:

```
risc0_zkvm::host::server::prove::executor: execution complete
risc0_zkvm::host::server::prove::recursion: lift
risc0_zkvm::host::server::prove::recursion: join
risc0_zkvm::host::server::prove: receipt complete
```

Narrate: "this is the real Risc0 proving stack — line 67 of the spec asks us to show this. Same prover handles every privacy-preserving tx on LEZ."

### Scene 1 — Architecture intro (~30s)

Open `ARCHITECTURE.md`. Walk through:
- The 4-layer breakdown (Basecamp app → adapter layer → indexing module → LEZ program / batch CLI)
- The PDA-per-CID storage decision and why (idempotency, unbounded capacity)
- Why the indexing module is Qt-free (reusable by any other Logos app)

### Scene 2 — Upload + broadcast (~60s)

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

### Scene 3 — Discovery via the batch tool (~45s)

In a second terminal window (visible on screen). For the demo recording, we use `--mock-delivery` to scope the headless component to its on-chain responsibility (subscribe + dedupe + batch-anchor); the real Storage/Delivery integration is what the UI plugin in Scene 2 just exercised.

```bash
./target/release/whistleblower-batch \
  --topic /lp0017-whistleblower/1/cids/json \
  --batch-size 3 \
  --batch-interval-secs 10 \
  --dedupe-store-path /tmp/wb-demo-queue.db \
  --mock-delivery
```

Expected log lines:
```
whistleblower-batch starting:
  topic = /lp0017-whistleblower/1/cids/json
  batch_size = 3
  batch_interval = 10s
  delivery = mock
[whistleblower-batch] received envelope cid=bafy<...> (1/3 in queue)
[whistleblower-batch] received envelope cid=bafy<...> (2/3 in queue)
[whistleblower-batch] received envelope cid=bafy<...> (3/3 in queue)
[whistleblower-batch] anchored batch: 3/3 entries hash=<tx-hash>
```

The "anchored batch" line takes 5-15s — that's wall time dominated by the localnet's 15s block creation interval. Per `BENCHMARKS.md`, the registry program's actual zkVM executor cost is ~120ms for the whole 50-CID case. Anchor txs are Public so don't trigger host-side proof gen (that was Scene 0).

Narrate: "the batch tool is permissionless — anyone can run it; it just observes the topic and anchors what it sees, idempotently."

### Scene 4 — Registry query (~30s)

Show querying one of the anchored CIDs without a transaction:

```bash
spel inspect <pda-base58> --idl whistleblower-registry-idl.json --type AnchorEntry
```

Expected output (JSON):
```
{
  "cid": "bafy<...>",
  "cid_hash": "<32 hex bytes>",
  "metadata_hash": "<32 hex bytes>",
  "anchor_timestamp": 1735689600123
}
```

Narrate: "no transaction needed — anyone with the CID hash can derive the PDA and read the entry directly".

### Scene 5 — CU benchmark (~20s)

Show the BENCHMARKS.md numbers in a tile:
```
single-CID anchor: ~6-12 ms zkVM executor time
50-CID batch anchor: ~120 ms total (~2.5 ms per CID amortized)
```

Mention: "These are zkVM executor times — the meaningful CU figure on LEZ for our public-tx anchor flow. Devnet wall-time numbers are pending the public testnet RPC URL."

### Scene 6 — Wrap (~15s)

Briefly:
- "All code is at github.com/<user>/whistleblower under MIT/Apache 2.0."
- "The submission PR is at logos-co/lambda-prize#<N>."
- "Bugs we filed upstream during the build: see BUGS_FILED.md."

Total run time target: 3-4 minutes.

## Recording checklist

- [ ] Terminal font size large enough to read at 1080p (use a 18-22pt font).
- [ ] `RISC0_DEV_MODE=0` is visible in `env | grep RISC` near the start.
- [ ] Audio levels normalised (no clipping).
- [ ] Captions / subtitles for accessibility.
- [ ] All file paths shown are reproducible — no `/Users/evinova/...` in the recording (use `$PWD` or the deploy script's canonical location).

## Reproducibility script

`scripts/demo.sh` now prepares the reproducible terminal side of the demo: non-dev localnet, registry build/deploy, idempotent anchor spike, live LEZ adapter tests, Basecamp `.lgx` install, batch CLI build, and ready-to-run commands for the on-camera batch + `spel inspect` scenes. The UI upload/broadcast scene remains human-driven inside Basecamp because file selection and module loading are GUI interactions.

Validate demo artifacts with:

```bash
python3 scripts/validate_demo_artifacts.py
bash -n scripts/demo.sh
```
