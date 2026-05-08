#!/usr/bin/env bash
# scripts/demo.sh — reproducible LP-0017 evaluator/demo runner.
#
# This script prepares and exercises the parts that can be driven from a
# terminal: non-dev localnet, registry build/deploy, idempotent anchor proof,
# live adapter tests, Basecamp .lgx install, and the exact commands to show in
# the narrated video for batch anchoring + registry inspection.
#
# Usage:
#   ./scripts/demo.sh
#   ./scripts/demo.sh --skip-build
#   ./scripts/demo.sh --skip-basecamp
#   ./scripts/demo.sh --keep-running
#
# Requirements on the demo machine: lgs, cargo, spel, wallet CLI, nix/Basecamp
# toolchain for the Basecamp install step.

set -euo pipefail

SKIP_BUILD=0
SKIP_BASECAMP=0
KEEP_RUNNING=0
for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=1 ;;
        --skip-basecamp) SKIP_BASECAMP=1 ;;
        --keep-running) KEEP_RUNNING=1 ;;
        --help|-h) sed -n '2,/^$/p' "$0" | sed 's/^# //; s/^#//'; exit 0 ;;
        *) echo "unknown arg: $arg" >&2; exit 2 ;;
    esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export RISC0_DEV_MODE=0
export NSSA_WALLET_HOME_DIR="${NSSA_WALLET_HOME_DIR:-$PWD/.scaffold/wallet}"
export NSSA_SEQUENCER_URL="${NSSA_SEQUENCER_URL:-http://127.0.0.1:3040}"

TS="$(date +%Y%m%d-%H%M%S)"
ART="artifacts/$TS"
mkdir -p "$ART"

run_step() {
    local name="$1"; shift
    echo "[demo] $name"
    "$@" 2>&1 | tee "$ART/$name.log"
}

cleanup() {
    if [[ "$KEEP_RUNNING" -eq 0 ]]; then
        echo "[demo] stopping localnet…"
        lgs localnet stop 2>&1 | tee -a "$ART/localnet-stop.log" || true
    else
        echo "[demo] --keep-running set; leaving sequencer up. Stop with: lgs localnet stop"
    fi
}
trap cleanup EXIT

echo "demo run at $TS"
echo "artifacts: $ART"
echo "RISC0_DEV_MODE=$RISC0_DEV_MODE"
echo "NSSA_WALLET_HOME_DIR=$NSSA_WALLET_HOME_DIR"
echo "NSSA_SEQUENCER_URL=$NSSA_SEQUENCER_URL"
echo

# Scene 0 support: visible proof generation evidence. This privacy-preserving
# faucet path is the reliable place to show Risc0 proof logs under RISC0_DEV_MODE=0.
echo "[demo] scene 0 command for video proof evidence:"
echo "  env | grep RISC0_DEV_MODE"
echo "  wallet pinata claim --to <our-account-id>"
echo

# 1. Start localnet in non-dev mode (scaffold.toml has risc0_dev_mode=false).
if lsof -nP -iTCP:3040 -sTCP:LISTEN 2>/dev/null | grep -q sequencer; then
    echo "[demo] localnet already listening on :3040; reusing it"
else
    run_step "01-localnet-start" lgs localnet start
fi

# 2. Build the registry guest.
GUEST_BIN="target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin"
if [[ "$SKIP_BUILD" -eq 0 ]] || [[ ! -f "$GUEST_BIN" ]]; then
    run_step "02-lgs-build" lgs build
else
    echo "[demo] skipping lgs build; guest binary exists: $GUEST_BIN"
fi

# 3. Deploy the registry program.
run_step "03-lgs-deploy" lgs deploy --program-path "$GUEST_BIN"
PROGRAM_ID="$(grep -E 'program_id:|Program ID|program id' "$ART/03-lgs-deploy.log" | head -1 | awk '{print $NF}' || true)"
echo "[demo] deployed program id: ${PROGRAM_ID:-unknown — inspect deploy log}"

# 4. Prove registry idempotency and capture inspectable PDA(s).
run_step "04-anchor-spike-build" cargo build -p anchor-spike --release
run_step "05-anchor-spike-run" ./target/release/anchor_spike
PDA="$(grep -E 'pda|PDA|account' "$ART/05-anchor-spike-run.log" | grep -Eo '[1-9A-HJ-NP-Za-km-z]{32,}' | head -1 || true)"

# 5. Run live adapter tests, including batch/idempotency behaviour.
run_step "06-live-lez-tests" cargo test -p whistleblower-lez-adapter --release -- --ignored --nocapture

# 6. Install the Basecamp plugin and dependencies via scaffold.
if [[ "$SKIP_BASECAMP" -eq 0 ]]; then
    run_step "07-basecamp-install" lgs basecamp install
    run_step "07b-fix-delivery-rln" scripts/fix_delivery_rln.sh
else
    echo "[demo] skipping lgs basecamp install (--skip-basecamp)"
fi

# 7. Build the batch CLI so the video can run/show the permissionless anchor tool.
run_step "08-batch-build" cargo build -p whistleblower-batch --release
cat > "$ART/09-batch-command.txt" <<'EOF'
./target/release/whistleblower-batch \
  --topic /lp0017-whistleblower/1/cids/json \
  --batch-size 3 \
  --batch-interval-secs 10 \
  --dedupe-store-path /tmp/wb-demo-queue.db \
  --mock-delivery
EOF

# 8. Registry inspection command. If anchor_spike printed a PDA, write a concrete
# command; otherwise write the template the narrator should fill from spike logs.
if [[ -n "$PDA" ]]; then
    echo "spel inspect $PDA --idl whistleblower-registry-idl.json --type AnchorEntry" > "$ART/10-spel-inspect-command.txt"
else
    echo "spel inspect <pda-base58-from-05-anchor-spike-run.log> --idl whistleblower-registry-idl.json --type AnchorEntry" > "$ART/10-spel-inspect-command.txt"
fi

cat <<EOF
═══════════════════════════════════════════════════════════
[demo] terminal prep complete.

Artifacts:
  $ART

Next on-camera steps:
  1. Show: env | grep RISC0_DEV_MODE
  2. Show real proof generation: wallet pinata claim --to <our-account-id>
  3. Launch Basecamp (if not already open): lgs basecamp launch
  4. In Basecamp, open Whistleblower, choose a small file, Publish, then Anchor.
  5. In another terminal, run the batch command from:
       $ART/09-batch-command.txt
  6. Inspect one on-chain entry with:
       $(cat "$ART/10-spel-inspect-command.txt")

Important honesty note:
  - The UI plugin uses real storage_module + delivery_module via LogosAPI.
  - The headless batch CLI remains --mock-delivery until a non-Qt Delivery
    runtime is verified; its on-chain anchoring path is real.
═══════════════════════════════════════════════════════════
EOF
