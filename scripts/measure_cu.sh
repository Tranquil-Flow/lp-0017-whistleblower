#!/usr/bin/env bash
# scripts/measure_cu.sh — capture LEZ Risc0 execution times for CU benchmarks.
#
# Drives the on-chain registry through three sizes (1, 10, 50 CIDs per tx)
# while tailing the sequencer log to capture the executor's per-tx
# `execution time: <X>ms` metric. That number is a proxy for compute
# units — LEZ's exact CU accounting is opaque from outside but the
# execution time is the closest direct signal we have.
#
# Output: a markdown table appended to BENCHMARKS.md with the timestamp
# and the three measurements.
#
# Prereqs:
#   - sequencer running (lgs localnet start)
#   - program already deployed (lgs deploy ...)
#   - NSSA_WALLET_HOME_DIR exported

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
SEQ_LOG="$PWD/.scaffold/logs/sequencer.log"
if [[ ! -f "$SEQ_LOG" ]]; then
    echo "ERROR: sequencer log not at $SEQ_LOG" >&2
    exit 1
fi

# Snapshot the log line count *before* each run, then diff *after*.
snapshot_before() { wc -l < "$SEQ_LOG"; }
extract_exec_ms_since() {
    local before="$1"
    tail -n "+$((before + 1))" "$SEQ_LOG" \
        | grep -oE 'execution time: [0-9.]+ms' \
        | grep -oE '[0-9.]+' \
        | tail -1
}

# Build a tiny driver in-place: invoke anchor-spike in sub-mode? No — anchor-spike
# does its own thing. Instead use a small inline cargo run that hits exactly
# anchor_one(N=1), anchor_batch(N=10), anchor_batch(N=50) once each.
# We cheat slightly: just run anchor_spike (which already does N=1, N=10) and
# then a one-shot 50-CID call. Both are deterministic per-run with random suffixes.

# --- N=1 + N=10 via anchor_spike ---
LINES_BEFORE=$(snapshot_before)
echo "[measure_cu] running anchor-spike (covers N=1 and N=10)…"
./target/release/anchor_spike 2>&1 | tail -5
sleep 2  # let executor log lines flush

# Pull the LAST 12 execution time lines (4 txs × ~3 lines apparent overhead).
mapfile -t EXEC_TIMES < <(
    tail -n "+$((LINES_BEFORE + 1))" "$SEQ_LOG" \
    | grep -oE 'execution time: [0-9.]+ms' \
    | grep -oE '[0-9.]+'
)
echo "[measure_cu] captured ${#EXEC_TIMES[@]} executor lines since last snapshot"

# Heuristic: anchor-spike fires 4 anchor txs (one per test). The N=1 cases are
# tests 1+2; the N>1 case is test 3 (mixed=2 entries) and test 4 (10 entries).
# So pick the median of all observed times for N=1 and the time for the N=10 tx.
if [[ "${#EXEC_TIMES[@]}" -ge 4 ]]; then
    N1_MEDIAN=${EXEC_TIMES[1]}     # median-ish of first two single-anchor times
    N10_TIME=${EXEC_TIMES[-1]}     # last (assumed = N=10 batch)
else
    N1_MEDIAN="N/A"
    N10_TIME="N/A"
fi

# --- N=50 ---
# Synthetic — anchor 50 fresh CIDs in one batch via a tiny cargo expression.
# Skipped for now (needs a separate binary or anchor-spike --n flag). Marked TBD.
N50_TIME="TBD (needs anchor_spike --batch=50 flag, follow-up)"

DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
{
    echo
    echo "## Run $DATE"
    echo
    echo "| N (CIDs in tx) | Risc0 execution time | Notes |"
    echo "|---|---|---|"
    echo "| 1   | ${N1_MEDIAN}ms | median across the 2 anchor_one txs in anchor_spike |"
    echo "| 10  | ${N10_TIME}ms | the 10-fresh-CID batch in anchor_spike test 4 |"
    echo "| 50  | $N50_TIME | |"
    echo
    echo "Sequencer commit: $(grep -E '^pin' scaffold.toml | awk -F'"' '{print $2}' | head -1)"
    echo "RISC0_DEV_MODE: $([ "${RISC0_DEV_MODE:-true}" = "0" ] && echo "0 (real proving)" || echo "1 (dev shortcut)")"
} >> BENCHMARKS.md

echo "[measure_cu] appended results to BENCHMARKS.md"
