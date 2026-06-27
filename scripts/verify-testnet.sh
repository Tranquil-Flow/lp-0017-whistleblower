#!/usr/bin/env bash
# scripts/verify-testnet.sh — richer LP-0017 public-testnet verifier.
set -euo pipefail
SEQ="https://testnet.lez.logos.co/"
PROGRAM_ID="1c8a08b62f1cf7b4a92693502bb5522372d937cfe9aa5a60a98a3dac6b5908f7"
PROGRAM_ID_BASE58="2vQVdFEVW79Xw3FCxYkeEyF52Ykqitoh57jcQ1NBxGv2"
PDA_A="4MBGdz8UULLERvijXheb54PwSzYGRQDyPhcHG3Ga57SE"
PDA_B="6eBL3uES8uJ9eYR4fCTkjMmLZnSphEmDMk753aGz4xrF"
CID_A="bafy-lp0017-v020-18bcea4c55bd1170-alpha"
CID_B="bafy-lp0017-v020-18bcea4c55bd1170-bravo"
TXS=(
  "deploy_program:db634916b48628e8f40b42021858f7f6731360dc48f5baa37a04edcd75cc598c:ProgramDeployment"
  "anchor_one:4de6176a58dade3188737e88a9e59b9c922c403452bb2dbc6e8dc66d0b0f3a78:Public"
  "anchor_one_dup:7114bce11b90a05c836a5d920da4a8fcb188395a7e9f470be006f66652ad0546:Public"
  "anchor_batch:05e7b3763d659ba9cbc1a3b2488edfd6e1d515a2f6468f5f34fcb976c7c70abf:Public"
)
if ! command -v wallet >/dev/null 2>&1; then
  echo "wallet binary not found on PATH — build/use the current LEZ v0.2.0 wallet." >&2
  exit 1
fi
HOME_DIR="$(mktemp -d -t lp0017-verify-XXXX)"
trap 'rm -rf "$HOME_DIR"' EXIT
export LEE_WALLET_HOME_DIR="$HOME_DIR"
wallet config set sequencer_addr "$SEQ" >/dev/null 2>&1 || true
echo "LP-0017 current public-testnet evidence re-verification"
echo "Network: $SEQ"
echo "Program: $PROGRAM_ID ($PROGRAM_ID_BASE58)"
fail=0
for entry in "${TXS[@]}"; do
  label="${entry%%:*}"; rest="${entry#*:}"; h="${rest%%:*}"; want="${rest##*:}"
  out="$(wallet chain-info transaction --hash "$h" 2>&1 || true)"
  if [[ "$out" == *"$want"* ]]; then
    printf '  ok   %-16s Some(%s) (%s)
' "$label" "$want" "$h"
  else
    printf '  FAIL %-16s expected %s, got: %s
' "$label" "$want" "$(echo "$out" | tr '
' ' ')" >&2
    fail=1
  fi
done

decode_pda() {
  local pda="$1" want_cid="$2" want_mh="$3"
  local raw; raw="$(wallet account get --account-id "Public/$pda" --raw 2>&1 || true)"
  if DATA_HEX="$(printf '%s' "$raw" | python3 -c 'import sys,json; print(json.load(sys.stdin)["data"])' 2>/dev/null)"; then
    if python3 - "$DATA_HEX" "$want_cid" "$want_mh" <<'PY'
import sys
data=bytes.fromhex(sys.argv[1]); want_cid=sys.argv[2]; want_mh=sys.argv[3]
i=0
slen=int.from_bytes(data[i:i+4],'little'); i+=4
cid=data[i:i+slen].decode(); i+=slen
cidh=data[i:i+32].hex(); i+=32
mh=data[i:i+32].hex(); i+=32
ts=int.from_bytes(data[i:i+8],'little'); i+=8
ok = cid==want_cid and mh==want_mh*32 and i==len(data)
print(f"  decoded {want_cid}: cid_hash={cidh} metadata_hash={mh} anchor_timestamp={ts}")
sys.exit(0 if ok else 1)
PY
    then echo "  ok   PDA $pda"; else echo "  FAIL PDA $pda mismatch" >&2; fail=1; fi
  else
    echo "  FAIL PDA $pda unreadable: $(echo "$raw" | tr '
' ' ')" >&2; fail=1
  fi
}
decode_pda "$PDA_A" "$CID_A" "11"
decode_pda "$PDA_B" "$CID_B" "22"
if [[ "$fail" == "0" ]]; then
  echo "All current public-testnet evidence re-verified live."
else
  exit 1
fi
