#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export TERM="${TERM:-xterm-256color}"
BOLD='[1m'; DIM='[2m'; GREEN='[32m'; CYAN='[36m'; YELLOW='[33m'; RESET='[0m'
SECTION_PAUSE="${SECTION_PAUSE:-4}"
COMMAND_PAUSE="${COMMAND_PAUSE:-1}"
SCENE_PAUSE="${SCENE_PAUSE:-5}"
RUN_COMMANDS="${RUN_COMMANDS:-1}"
pause(){ sleep "${1:-$SECTION_PAUSE}"; }
scene(){ printf '
%b════════════════════════════════════════════════════════════%b
' "$BOLD$CYAN" "$RESET"; printf '%b  %s%b
' "$BOLD$CYAN" "$1" "$RESET"; printf '%b════════════════════════════════════════════════════════════%b

' "$BOLD$CYAN" "$RESET"; pause "$SECTION_PAUSE"; }
say(){ printf '%s
' "$*"; }
cmd(){ printf '%b$ %s%b
' "$DIM" "$*" "$RESET"; if [[ "$RUN_COMMANDS" == "1" ]]; then bash -lc "$*"; else printf '  [dry display only]
'; fi; printf '
'; pause "$COMMAND_PAUSE"; }
soft(){ printf '%b$ %s%b
' "$DIM" "$*" "$RESET"; if [[ "$RUN_COMMANDS" == "1" ]]; then bash -lc "$*" || true; else printf '  [dry display only]
'; fi; printf '
'; pause "$COMMAND_PAUSE"; }

clear || true
scene "LP-0017 final resubmission demo — whistleblower anchors on public testnet"
say "This recording demonstrates LP-0017 whistleblower disclosure anchors on the public LEZ testnet."
say "The goal is content-addressed disclosure evidence with deterministic duplicate handling and batch anchoring."
pause "$SCENE_PAUSE"
scene "1. Repository and proof documents"
cmd "git log -1 --oneline"
cmd "grep -n 'Program\|Image\|deploy_program\|anchor_one\|anchor_batch' TESTNET_PROOF.md | head -100"
scene "2. Public-testnet transaction verifier"
say "This walletless verifier queries already deployed public-testnet transactions."
cmd "bash scripts/ci-verify-testnet.sh"
scene "3. Basecamp / storage / delivery framing"
say "Now I show the documented application framing. The resubmission should not overclaim beyond this evidence until the video shows the flow."
cmd "grep -RIn 'Basecamp\|Storage\|Delivery\|GUI\|testnet' README.md docs solutions submission 2>/dev/null | head -100"
scene "4. Final reviewer summary"
say "LP-0017 is ready for resubmission once this fresh video URL is inserted."
say "The public testnet verifier shows deploy_program, anchor_one, duplicate anchor handling, and anchor_batch are live."
printf '\n%bLP-0017 final video script complete.%b\n' "$GREEN" "$RESET"
