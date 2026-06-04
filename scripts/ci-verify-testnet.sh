#!/usr/bin/env bash
# scripts/ci-verify-testnet.sh — on-push CI check against the PUBLIC LEZ testnet.
#
# No wallet build, no secrets, no localnet, no mock: re-queries each transaction
# from the 2026-06-03 whistleblower-registry deploy + anchor lifecycle straight
# from the public sequencer's JSON-RPC (`getTransaction`) and fails if any are
# missing. This is the real on-push end-to-end check that replaces the old
# never-running `workflow_dispatch` + `exit 1` stub.
#
# The richer wallet-based check (decodes the entry PDAs) is scripts/verify-testnet.sh.
# Full evidence: TESTNET_PROOF.md
set -euo pipefail

SEQ="${LP0017_SEQUENCER:-https://testnet.lez.logos.co/}"

# label:hash
TXS=(
  "deploy_program:05781c3c5fa65d72d1ee9ee8f0964144f9a5688ef8ad14f445581e308026608f"
  "anchor_one:9f6aee9cc97a62300780f0e576e76c61c4e1fb32bef5067d574a798a1a0de227"
  "anchor_one_dup:8f2fe8f103a9c6a7a65547e9244db9ef4a1d3ef42caf8067288316f2d920dfbc"
  "anchor_batch:f5fedf2910dad89c91a62ec257f7a722c638c07203fac914a9766cdfe148e22f"
)

echo "Verifying LP-0017 deployed transactions on $SEQ (read-only)"
fail=0
for entry in "${TXS[@]}"; do
  label="${entry%%:*}"; h="${entry#*:}"
  resp="$(curl -s --max-time 30 -X POST "$SEQ" -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getTransaction\",\"params\":[\"$h\"]}" || true)"
  # result is a (long) serialized-tx string when included, null when not.
  if printf '%s' "$resp" | python3 -c 'import sys,json; r=json.load(sys.stdin).get("result"); sys.exit(0 if isinstance(r,str) and len(r)>0 else 1)' 2>/dev/null; then
    echo "  ok   $label  ($h)"
  else
    echo "  FAIL $label  ($h) -> $(printf '%s' "$resp" | head -c 200)"
    fail=1
  fi
done

if [[ "$fail" == "0" ]]; then
  echo "All deployed LP-0017 transactions are live on the public testnet."
else
  echo "One or more transactions could not be confirmed on the public testnet." >&2
  exit 1
fi
