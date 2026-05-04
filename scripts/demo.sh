#!/usr/bin/env bash
# scripts/demo.sh — deterministic end-to-end LP-0017 demo runner.
#
# Drives the full registry path: (re)build the guest, deploy to a local
# sequencer, run the spike, run the live integration tests. Captures all
# logs + outputs to a timestamped artifacts/ dir so the run is auditable
# after the fact (and so the recorded video can be reproduced exactly).
#
# Real Logos Storage / Delivery integrations are deferred to Phase 1.7
# (the nix-built Basecamp UI plugin). Until that lands this script
# proves the on-chain side end-to-end and the indexing crate's mock
# pipeline. The two together cover ~70% of the spec's E2E surface.
#
# Usage:
#     ./scripts/demo.sh
#     ./scripts/demo.sh --skip-build   # if guest binary is already current
#     ./scripts/demo.sh --keep-running # leave the sequencer up after
#
# Exit code is the first non-zero from any step. All stdout+stderr from
# every step is teed to artifacts/<timestamp>/.

set -euo pipefail

# ---- arg parsing ----
SKIP_BUILD=0
KEEP_RUNNING=0
for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=1 ;;
        --keep-running) KEEP_RUNNING=1 ;;
        --help|-h)
            sed -n '2,/^$/p' "$0" | sed 's/^# //; s/^#//'
            exit 0
            ;;
        *) echo "unknown arg: $arg" >&2; exit 2 ;;
    esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TS="$(date +%Y%m%d-%H%M%S)"
ART="artifacts/$TS"
mkdir -p "$ART"
echo "demo run at $TS — artifacts in $ART"
echo

# ---- ensure cleanup on exit ----
cleanup() {
    if [[ "$KEEP_RUNNING" -eq 0 ]]; then
        echo "[demo] stopping localnet…"
        lgs localnet stop 2>&1 | tee -a "$ART/05-stop.log" || true
    else
        echo "[demo] --keep-running set; leaving sequencer up (lgs localnet stop to stop)."
    fi
}
trap cleanup EXIT

# ---- step 1: ensure local sequencer ----
echo "[demo] step 1/5: starting localnet (idempotent)"
if lsof -nP -iTCP:3040 -sTCP:LISTEN 2>/dev/null | grep -q sequencer; then
    echo "[demo]   sequencer already on :3040, reusing it"
else
    lgs localnet start 2>&1 | tee "$ART/01-localnet-start.log"
fi
echo

# ---- step 2: build guest (skippable on warm cache) ----
GUEST_BIN="target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin"

if [[ "$SKIP_BUILD" -eq 0 ]] || [[ ! -f "$GUEST_BIN" ]]; then
    echo "[demo] step 2/5: lgs build"
    lgs build 2>&1 | tee "$ART/02-build.log"
else
    echo "[demo] step 2/5: --skip-build set + binary present, skipping"
fi
echo

# ---- step 3: deploy ----
echo "[demo] step 3/5: lgs deploy"
lgs deploy --program-path "$GUEST_BIN" 2>&1 | tee "$ART/03-deploy.log"
PROGRAM_ID=$(grep "program_id:" "$ART/03-deploy.log" | head -1 | awk '{print $NF}')
echo "[demo]   deployed program_id=$PROGRAM_ID"
echo

# ---- step 4: run the spike ----
echo "[demo] step 4/5: anchor_spike (PDA-per-CID idempotency proof)"
cargo build -p anchor-spike --release 2>&1 | tee -a "$ART/04-spike-build.log"
NSSA_WALLET_HOME_DIR="$PWD/.scaffold/wallet" \
    ./target/release/anchor_spike 2>&1 | tee "$ART/04-spike-run.log"
echo

# ---- step 5: live integration tests via the adapter ----
echo "[demo] step 5/5: live integration tests via LezRegistryClient"
NSSA_WALLET_HOME_DIR="$PWD/.scaffold/wallet" \
    cargo test -p whistleblower-lez-adapter --release -- --ignored --nocapture 2>&1 \
    | tee "$ART/05-live-tests.log"
echo

# ---- summary ----
echo "═══════════════════════════════════════════════════════════"
echo "[demo] ✅ E2E run complete."
echo "[demo] artifacts: $ART"
echo "[demo]   01-localnet-start.log"
echo "[demo]   02-build.log"
echo "[demo]   03-deploy.log  (program_id: $PROGRAM_ID)"
echo "[demo]   04-spike-run.log"
echo "[demo]   05-live-tests.log"
echo "═══════════════════════════════════════════════════════════"
echo
echo "What this demo did NOT cover (deferred to Phase 1.7):"
echo "  - Real Logos Storage upload from the Basecamp UI"
echo "  - Real Logos Delivery broadcast subscription"
echo "  - whistleblower-batch CLI driving real subscribed traffic"
echo "  - Narrated video walkthrough"
