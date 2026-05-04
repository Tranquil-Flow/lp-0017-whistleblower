# Demo

The submission video must be a narrated walkthrough (silent screencast is explicitly insufficient — see `~/Projects/logos-basecamp/SPECS/README.md` §"Demo requirements"). This file is the script.

## What the spec requires the video to show

From `LP-0017.md` §Submission Requirements:
1. A file uploaded and immediately findable via the Logos Delivery topic.
2. The batch anchor tool picking up the broadcast CID and anchoring it on-chain.
3. The on-chain registry confirming the CID registration.
4. Terminal output showing proof generation with `RISC0_DEV_MODE=0` so reviewers know real proving was used (not the dev shortcut).

## Walkthrough script

> **Status:** the steps below are written for the post-Phase-1.7 state where real Logos Storage and Delivery adapters are wired in. The current code uses mocked Storage + Delivery; the registry path is real. Update this script once `--mock-delivery` is dropped.

### Setup (off-camera)

```bash
# Fresh sequencer, fresh state.
rm -rf .scaffold/state .scaffold/logs queue.db
lgs localnet start
lgs build
lgs deploy --program-path target/.../whistleblower_registry.bin
export NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet
export RISC0_DEV_MODE=0   # critical — must be visible in the recorded terminal
```

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

In a second terminal window (visible on screen):

```bash
./target/release/whistleblower-batch \
  --topic /lp0017-whistleblower/1/cids/json \
  --batch-size 3 \
  --batch-interval-secs 10 \
  --dedupe-store-path /tmp/wb-demo-queue.db
```

Expected log lines:
```
whistleblower-batch starting:
  topic = /lp0017-whistleblower/1/cids/json
  batch_size = 3
  batch_interval = 10s
  delivery = REAL
[whistleblower-batch] received envelope cid=bafy<...> (1/3 in queue)
```

Now drop two more files via the Basecamp UI to trigger the size threshold. The batch tool should log:
```
[whistleblower-batch] received envelope cid=bafy<...> (2/3 in queue)
[whistleblower-batch] received envelope cid=bafy<...> (3/3 in queue)
[whistleblower-batch] anchored batch: 3/3 entries
```

The "anchored batch" line is when proof generation runs — `RISC0_DEV_MODE=0` makes this take 30-90s, audible/visible delay before the next message. Wait it out on camera.

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
single-CID anchor: <X> CU
50-CID batch anchor: <Y> CU per CID amortized
```

Mention: "This was measured on the same local sequencer used in this demo."

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

After Phase 1.7 lands, `scripts/demo.sh` will run scenes 2-5 non-interactively so reviewers can replay the exact flow from a clean clone. The video shows the human-driven version; the script is for evaluation.
