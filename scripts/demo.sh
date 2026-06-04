#!/usr/bin/env bash
# scripts/demo.sh — LP-0017 reproducible demo / evaluator runner.
#
# DEFAULT (no args): real, headless, no-mock, clone-and-run evidence against the
# PUBLIC LEZ TESTNET. Re-verifies the deployed whistleblower-registry program's
# full lifecycle (deploy → anchor_one → idempotent re-anchor → batch) straight
# from the public sequencer. No localnet, no mock delivery, no GUI, no manual
# on-camera steps. Needs only `curl`+`python3` (or the `wallet` binary for the
# richer PDA-decoding check).
#
# Modes:
#   ./scripts/demo.sh                 # verify deployed testnet lifecycle (clone-and-run safe)
#   ./scripts/demo.sh --batch         # run the REAL whistleblower-batch tool against the deployed
#                                     #   testnet program from an envelope file — NO --mock-delivery
#   ./scripts/demo.sh --full          # fresh build + deploy + lifecycle on testnet, then --batch
#   ./scripts/demo.sh --localnet      # spec-literal path: real LOCAL sequencer, RISC0_DEV_MODE=0
#   ./scripts/demo.sh --help
#
# The upload→broadcast leg (Logos Storage + Delivery) runs inside the Basecamp
# UI plugin via the real in-process LogosAPIClient. It is GUI-driven and shown
# in the narrated video — not part of this unattended script — because real
# Logos Delivery is Waku + RLN behind a Qt `logos_host` module over
# QtRemoteObjects (see adapters/logos/README.md). The batch tool's on-chain work
# below is fully real against the live testnet.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MODE="verify"
for arg in "$@"; do
    case "$arg" in
        --batch) MODE="batch" ;;
        --full) MODE="full" ;;
        --localnet) MODE="localnet" ;;
        --help|-h) sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown arg: $arg (try --help)" >&2; exit 2 ;;
    esac
done

# Deployed public-testnet program (see TESTNET_PROOF.md).
TESTNET_SEQ="https://testnet.lez.logos.co/"
PROGRAM_ID="54c7f793caa540408ce2ca4c22051d78c466cd5ed8db607feedd19dcb749aa91"
DEPLOYED_BIN="target/riscv32im-risc0-zkvm-elf/docker/whistleblower_registry.bin"
ENVELOPES="demo/sample-envelopes.jsonl"
TOPIC="/lp0017-whistleblower/1/cids/json"

hr() { printf '%s\n' "════════════════════════════════════════════════════════════════"; }

# ---------------------------------------------------------------------------
# verify — read-only re-verification of the deployed testnet lifecycle.
# Clone-and-run safe: prefers the wallet-based check (decodes PDAs), falls back
# to the curl+python JSON-RPC check (no wallet, no secrets).
# ---------------------------------------------------------------------------
do_verify() {
    hr; echo "  LP-0017 demo — verify deployed lifecycle on the PUBLIC LEZ TESTNET"
    echo "  program: $PROGRAM_ID"
    echo "  network: $TESTNET_SEQ (real consensus, RISC0_DEV_MODE=0, sequencer-proved)"; hr
    if command -v wallet >/dev/null 2>&1; then
        echo "[demo] wallet binary found — running rich re-verification (decodes entry PDAs)"
        bash scripts/verify-testnet.sh
    else
        echo "[demo] wallet binary not on PATH — running curl-only re-verification"
        echo "       (build the wallet from LEZ tag v0.1.2 for the PDA-decoding check)"
        bash scripts/ci-verify-testnet.sh
    fi
    echo
    echo "[demo] Verified the deployed registry lifecycle is live on chain."
    echo "       Run a FRESH real anchor with:  ./scripts/demo.sh --batch   (needs a funded testnet wallet)"
    echo "       Full fresh deploy + lifecycle:  ./scripts/demo.sh --full"
}

# ---------------------------------------------------------------------------
# batch — run the REAL whistleblower-batch tool against the deployed testnet
# program from an envelope file. No mock delivery: --envelopes-from replays the
# exact MetadataEnvelopeV1 records the Delivery topic carries through the real
# dedupe + batch + on-chain anchor pipeline. Idempotent, so re-runs are no-ops.
# ---------------------------------------------------------------------------
require_testnet_wallet() {
    if [[ -z "${NSSA_WALLET_HOME_DIR:-}" ]]; then
        echo "[demo] ERROR: set NSSA_WALLET_HOME_DIR to a funded public-testnet wallet home." >&2
        echo "       (account initialised + pinata-claimed; sequencer_addr=$TESTNET_SEQ)" >&2
        exit 1
    fi
}

do_batch() {
    require_testnet_wallet
    [[ -f "$ENVELOPES" ]] || { echo "[demo] ERROR: $ENVELOPES missing" >&2; exit 1; }
    if [[ ! -f "$DEPLOYED_BIN" ]]; then
        echo "[demo] ERROR: deployed program bin not found at $DEPLOYED_BIN" >&2
        echo "       Build it (route heavy risc0 work to a build host):" >&2
        echo "         cargo risczero build --manifest-path methods/guest/Cargo.toml" >&2
        exit 1
    fi
    export RISC0_DEV_MODE=0
    export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
    echo "[demo] building whistleblower-batch…"
    cargo build -p whistleblower-batch --release
    hr; echo "  Running the permissionless batch anchor tool against the live testnet"
    echo "  source   : $ENVELOPES (real broadcast envelopes — NO --mock-delivery)"
    echo "  program  : deployed $PROGRAM_ID (via --program-bin)"; hr
    ./target/release/whistleblower-batch \
        --topic "$TOPIC" \
        --batch-size 3 \
        --batch-interval-secs 10 \
        --dedupe-store-path /tmp/wb-demo-queue.db \
        --program-bin "$DEPLOYED_BIN" \
        --envelopes-from "$ENVELOPES"
    echo "[demo] batch anchor complete. Re-run to observe idempotency (no-op)."
}

# ---------------------------------------------------------------------------
# full — fresh deploy + lifecycle on the testnet via the typed driver, then the
# batch tool. Heavy: requires the rc3 guest .bin + a funded testnet wallet.
# ---------------------------------------------------------------------------
do_full() {
    require_testnet_wallet
    if [[ ! -f "$DEPLOYED_BIN" ]]; then
        echo "[demo] ERROR: $DEPLOYED_BIN missing — build the rc3 guest first:" >&2
        echo "         cargo risczero build --manifest-path methods/guest/Cargo.toml" >&2
        exit 1
    fi
    export RISC0_DEV_MODE=0
    export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
    export LP0017_PROGRAM_BIN="$PWD/$DEPLOYED_BIN"
    hr; echo "  Fresh deploy + anchor lifecycle on the PUBLIC TESTNET (captures tx hashes)"; hr
    cargo run --release -p anchor-spike --bin testnet_lifecycle
    echo
    do_batch
}

# ---------------------------------------------------------------------------
# localnet — the spec's literal "real LOCAL sequencer with RISC0_DEV_MODE=0"
# path. Retained as corroboration; the public testnet (above) is the primary
# evidence. Build + deploy + idempotent anchor + Basecamp install + batch tool.
# ---------------------------------------------------------------------------
do_localnet() {
    export RISC0_DEV_MODE=0
    export NSSA_WALLET_HOME_DIR="${NSSA_WALLET_HOME_DIR:-$PWD/.scaffold/wallet}"
    export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
    GUEST_BIN="target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin"
    hr; echo "  LP-0017 demo — LOCAL sequencer, RISC0_DEV_MODE=0 (spec-literal corroboration)"; hr
    echo "[demo] env | grep RISC0_DEV_MODE:"; env | grep RISC0_DEV_MODE || true

    # Real proof generation evidence (privacy-preserving faucet path fires the prover).
    echo "[demo] proof-generation evidence command (privacy-preserving tx):"
    echo "         wallet pinata claim --to <our-account-id>    # visible Risc0 prover stages"

    lgs localnet start
    lgs build
    lgs deploy --program-path "$GUEST_BIN"
    spel inspect "$GUEST_BIN" || true

    cargo build -p anchor-spike --release
    ./target/release/anchor_spike

    # Basecamp UI plugin (real Storage + Delivery via in-process LogosAPIClient).
    lgs basecamp install
    scripts/fix_delivery_rln.sh

    # Permissionless batch tool against the localnet program, real envelope file (no mock).
    cargo build -p whistleblower-batch --release
    ./target/release/whistleblower-batch \
        --topic "$TOPIC" \
        --batch-size 3 \
        --batch-interval-secs 10 \
        --dedupe-store-path /tmp/wb-localnet-queue.db \
        --envelopes-from "$ENVELOPES"

    # Query an entry without a transaction.
    echo "[demo] query a registry entry (no tx):"
    echo "         spel inspect <pda-base58-from-anchor_spike> --idl whistleblower-registry.idl.json --type AnchorEntry"

    echo "[demo] localnet flow complete. Stop with: lgs localnet stop"
}

case "$MODE" in
    verify) do_verify ;;
    batch) do_batch ;;
    full) do_full ;;
    localnet) do_localnet ;;
esac
