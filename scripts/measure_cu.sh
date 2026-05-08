#!/usr/bin/env bash
# scripts/measure_cu.sh — capture LEZ Risc0 execution times for CU benchmarks.
#
# Drives the on-chain registry through 1, 10, and 50 CID transaction shapes
# while reading the sequencer log for the executor's `execution time: <X>ms`
# metric. That number is the closest direct compute-unit proxy currently
# exposed by the LEZ localnet/testnet stack.
#
# Output: appends a timestamped markdown table to BENCHMARKS.md and writes raw
# command logs under artifacts/cu-<timestamp>/.
#
# Prereqs:
#   - sequencer running (lgs localnet start)
#   - program already deployed (lgs deploy ...)
#   - target/release/anchor_spike built
#   - NSSA_WALLET_HOME_DIR exported, or .scaffold/wallet exists

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if ! lsof -nP -iTCP:3040 -sTCP:LISTEN 2>/dev/null | grep -q sequencer; then
    echo "ERROR: no sequencer on :3040. Run 'lgs localnet start' first." >&2
    exit 1
fi
if [[ -z "${NSSA_WALLET_HOME_DIR:-}" ]]; then
    export NSSA_WALLET_HOME_DIR="$PWD/.scaffold/wallet"
fi
if [[ ! -d "$NSSA_WALLET_HOME_DIR" ]]; then
    echo "ERROR: NSSA_WALLET_HOME_DIR does not exist: $NSSA_WALLET_HOME_DIR" >&2
    exit 1
fi

SEQ_LOG="$PWD/.scaffold/logs/sequencer.log"
if [[ ! -f "$SEQ_LOG" ]]; then
    echo "ERROR: sequencer log not at $SEQ_LOG" >&2
    exit 1
fi
if [[ ! -x ./target/release/anchor_spike ]]; then
    echo "ERROR: ./target/release/anchor_spike missing. Run: cargo build -p anchor-spike --release" >&2
    exit 1
fi

DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
ART="artifacts/cu-${DATE//[:]/-}"
mkdir -p "$ART"

snapshot_before() { wc -l < "$SEQ_LOG"; }
exec_times_since() {
    local before="$1"
    tail -n "+$((before + 1))" "$SEQ_LOG" \
        | grep -oE 'execution time: [0-9.]+ms' \
        | grep -oE '[0-9.]+' || true
}
last_exec_time_since() {
    local before="$1"
    exec_times_since "$before" | tail -1
}

# --- N=1 + N=10 via anchor_spike ---
LINES_BEFORE=$(snapshot_before)
echo "[measure_cu] running anchor-spike (covers N=1 and N=10)…"
./target/release/anchor_spike 2>&1 | tee "$ART/anchor-spike.log"
sleep 2  # let executor log lines flush
mapfile -t SPIKE_EXEC_TIMES < <(exec_times_since "$LINES_BEFORE")
echo "[measure_cu] captured ${#SPIKE_EXEC_TIMES[@]} executor lines from anchor-spike"

if [[ "${#SPIKE_EXEC_TIMES[@]}" -ge 4 ]]; then
    N1_TIME="${SPIKE_EXEC_TIMES[1]}"     # median-ish of the first two single-anchor txs
    N10_TIME="${SPIKE_EXEC_TIMES[-1]}"   # last anchor-spike tx is the 10-fresh-CID batch
else
    N1_TIME="N/A"
    N10_TIME="N/A"
fi

# --- N=50 via the live adapter test that exists solely for spec line 41 headroom ---
LINES_BEFORE=$(snapshot_before)
echo "[measure_cu] running 50-CID live adapter test…"
set +e
RISC0_DEV_MODE="${RISC0_DEV_MODE:-0}" \
NSSA_WALLET_HOME_DIR="$NSSA_WALLET_HOME_DIR" \
cargo test -p whistleblower-lez-adapter --release \
    lez_adapter_anchor_50_cids_in_one_tx \
    -- --ignored --nocapture --test-threads=1 \
    2>&1 | tee "$ART/lez-adapter-50.log"
TEST_STATUS=${PIPESTATUS[0]}
set -e
if [[ "$TEST_STATUS" -ne 0 ]]; then
    echo "ERROR: 50-CID live adapter benchmark test failed; see $ART/lez-adapter-50.log" >&2
    exit "$TEST_STATUS"
fi
sleep 2
N50_TIME="$(last_exec_time_since "$LINES_BEFORE")"
[[ -n "$N50_TIME" ]] || N50_TIME="N/A"
N50_WALL="$(grep -Eo '50-CID batch wall-clock = [^ ]+' "$ART/lez-adapter-50.log" | tail -1 | sed 's/^50-CID batch wall-clock = //' || true)"
[[ -n "$N50_WALL" ]] || N50_WALL="see $ART/lez-adapter-50.log"

{
    echo
    echo "## Run $DATE"
    echo
    echo "| N (CIDs in tx) | Risc0 execution time | Wall-clock | Notes |"
    echo "|---|---:|---:|---|"
    echo "| 1   | ${N1_TIME}ms | see $ART/anchor-spike.log | median-ish across the first two anchor_one txs in anchor_spike |"
    echo "| 10  | ${N10_TIME}ms | see $ART/anchor-spike.log | 10-fresh-CID batch in anchor_spike test 4 |"
    echo "| 50  | ${N50_TIME}ms | ${N50_WALL} | lez_adapter_anchor_50_cids_in_one_tx live test |"
    echo
    echo "Sequencer commit: $(grep -E '^pin' scaffold.toml | awk -F'"' '{print $2}' | head -1)"
    echo "RISC0_DEV_MODE: $([ "${RISC0_DEV_MODE:-0}" = "0" ] && echo "0 (real-mode env)" || echo "${RISC0_DEV_MODE:-unset}")"
    echo "Raw logs: $ART"
} >> BENCHMARKS.md

echo "[measure_cu] appended results to BENCHMARKS.md"
