#!/usr/bin/env bash
# scripts/ci-verify-testnet.sh — read-only check against the current public LEZ testnet.
set -euo pipefail
SEQ="${LP0017_SEQUENCER:-https://testnet.lez.logos.co/}"
TXS=(
  "deploy_program:db634916b48628e8f40b42021858f7f6731360dc48f5baa37a04edcd75cc598c"
  "anchor_one:4de6176a58dade3188737e88a9e59b9c922c403452bb2dbc6e8dc66d0b0f3a78"
  "anchor_one_dup:7114bce11b90a05c836a5d920da4a8fcb188395a7e9f470be006f66652ad0546"
  "anchor_batch:05e7b3763d659ba9cbc1a3b2488edfd6e1d515a2f6468f5f34fcb976c7c70abf"
)
echo "Verifying LP-0017 current deployed transactions on $SEQ (read-only)"
fail=0
for entry in "${TXS[@]}"; do
  label="${entry%%:*}"
  h="${entry#*:}"
  payload=$(python3 - "$h" <<'PY'
import json, sys
print(json.dumps({"jsonrpc":"2.0","id":1,"method":"getTransaction","params":[sys.argv[1]]}))
PY
)
  resp="$(curl -s --max-time 30 -X POST "$SEQ" -H 'content-type: application/json' -d "$payload" || true)"
  if printf '%s' "$resp" | python3 -c 'import sys,json; r=json.load(sys.stdin).get("result"); sys.exit(0 if isinstance(r,str) and len(r)>0 else 1)' 2>/dev/null; then
    echo "  ok   $label  ($h)"
  else
    echo "  FAIL $label  ($h) -> $(printf '%s' "$resp" | head -c 200)" >&2
    fail=1
  fi
done
if [[ "$fail" == "0" ]]; then
  echo "All current LP-0017 transactions are live on the public testnet."
else
  echo "One or more current transactions could not be confirmed on the public testnet." >&2
  exit 1
fi
