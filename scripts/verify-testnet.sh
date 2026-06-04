#!/usr/bin/env bash
# scripts/verify-testnet.sh — re-verify LP-0017's public-testnet evidence.
#
# Read-only: points a throwaway wallet home at https://testnet.lez.logos.co/
# and re-queries each transaction hash from the 2026-06-03 deploy + anchor
# lifecycle, then reads both entry PDAs straight off chain and decodes the
# borsh AnchorEntry. Needs ONLY the `wallet` binary (built from LEZ v0.1.2 /
# v0.2.0-rc3) on PATH — no build, no faucet, no local state, no signing.
#
# Full evidence + context: TESTNET_PROOF.md
set -euo pipefail

BOLD='\033[1m'; DIM='\033[2m'; GREEN='\033[32m'; CYAN='\033[36m'; YELLOW='\033[33m'; RED='\033[31m'; RESET='\033[0m'

SEQ="https://testnet.lez.logos.co/"
PROGRAM_ID="54c7f793caa540408ce2ca4c22051d78c466cd5ed8db607feedd19dcb749aa91"
PDA_A="B1GxfUsX5hE73EFumBfdPXSTK7pJjPCmP7dnvEtibZ7i"   # cid_a (…-alpha, metadata 0x11)
PDA_B="2qoQ8niS9UtSKRmgZH1XF7mgfyVhzwv43cS8dBnyT5wV"   # cid_b (…-bravo, metadata 0x22)

# label:hash:expected-kind
TXS=(
  "deploy_program:05781c3c5fa65d72d1ee9ee8f0964144f9a5688ef8ad14f445581e308026608f:ProgramDeployment"
  "anchor_one:9f6aee9cc97a62300780f0e576e76c61c4e1fb32bef5067d574a798a1a0de227:Public"
  "anchor_one_dup:8f2fe8f103a9c6a7a65547e9244db9ef4a1d3ef42caf8067288316f2d920dfbc:Public"
  "anchor_batch:f5fedf2910dad89c91a62ec257f7a722c638c07203fac914a9766cdfe148e22f:Public"
)

export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
if ! command -v wallet >/dev/null 2>&1; then
  echo -e "${RED}wallet binary not found on PATH${RESET} — build it from LEZ tag v0.1.2."
  exit 1
fi

HOME_DIR="$(mktemp -d -t lp0017-verify-XXXX)"
trap 'rm -rf "$HOME_DIR"' EXIT
export NSSA_WALLET_HOME_DIR="$HOME_DIR"

echo -e "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo -e "${BOLD}${CYAN}  LP-0017 — public-testnet evidence re-verification (read-only)${RESET}"
echo -e "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo ""
echo "  Network : $SEQ (real consensus, RISC0_DEV_MODE=0)"
echo "  Program : $PROGRAM_ID"
echo ""

wallet config set sequencer_addr "$SEQ" >/dev/null 2>&1 || true
echo -e "${BOLD}${YELLOW}▶ Sequencer reachable; current block id${RESET}"
wallet chain-info current-block-id 2>&1 | sed 's/^/  /' | tail -1
echo ""

echo -e "${BOLD}${YELLOW}▶ Per-transaction verdicts (queried live from the public sequencer)${RESET}"
fail=0
for entry in "${TXS[@]}"; do
  label="${entry%%:*}"; rest="${entry#*:}"; h="${rest%%:*}"; want="${rest##*:}"
  out="$(wallet chain-info transaction --hash "$h" 2>&1 || true)"
  if [[ "$out" == *"$want"* ]]; then
    printf "  ${GREEN}✓${RESET} %-16s Some(%s)  ${DIM}(%s…)${RESET}\n" "$label" "$want" "${h:0:8}"
  else
    printf "  ${RED}✗${RESET} %-16s expected %s, got: %s\n" "$label" "$want" "$(echo "$out" | tr '\n' ' ')"
    fail=1
  fi
done
echo ""

echo -e "${BOLD}${YELLOW}▶ Entry PDA readback (decode account.data as borsh AnchorEntry)${RESET}"
decode_pda() {
  local pda="$1" want_cid="$2" want_mh="$3"
  local raw; raw="$(wallet account get --account-id "Public/$pda" --raw 2>&1 || true)"
  echo -e "  ${DIM}\$ wallet account get --account-id Public/$pda --raw${RESET}"
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
ok = cid==want_cid and mh==want_mh*32 and (len(data)-i)==0
print(f"    decoded AnchorEntry: cid={cid}")
print(f"      cid_hash={cidh[:16]}…  metadata_hash=0x{want_mh}×32 {'✓' if mh==want_mh*32 else '✗ '+mh}  anchor_timestamp={ts}")
sys.exit(0 if ok else 1)
PY
    then echo -e "    ${GREEN}✓ entry present and correct${RESET}"
    else echo -e "    ${RED}✗ entry mismatch${RESET}"; fail=1; fi
  else
    echo -e "    ${RED}✗ PDA empty or unreadable${RESET} (raw: $(echo "$raw" | tr '\n' ' '))"; fail=1
  fi
}
decode_pda "$PDA_A" "bafy-lp0017-testnet-18b597589606e650-alpha" "11"
decode_pda "$PDA_B" "bafy-lp0017-testnet-18b597589606e650-bravo" "22"
echo ""

if [[ "$fail" == "0" ]]; then
  echo -e "${BOLD}${GREEN}  ✓ All public-testnet evidence re-verified live.${RESET}"
  echo "  Full proof log: TESTNET_PROOF.md"
else
  echo -e "${BOLD}${RED}  ✗ One or more checks failed (see above).${RESET}"
  exit 1
fi
