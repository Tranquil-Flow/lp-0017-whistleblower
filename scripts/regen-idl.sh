#!/usr/bin/env bash
# Regenerate whistleblower-registry.idl.json from the parse-only SPEL definition.
#
# The `instructions` + `accounts` blocks are emitted VERBATIM by `spel generate-idl`
# (spel 0.2.0). The `constants` + `metadata` blocks are appended by
# scripts/_idl_annotate.py — `spel generate-idl` does not emit them, and the
# constants document the PDA seed derivation that SPEL's IdlSeed enum cannot
# express (see BUGS_FILED.md).
#
# Usage: bash scripts/regen-idl.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DEF="idl/whistleblower_registry.rs"
OUT="whistleblower-registry.idl.json"

command -v spel >/dev/null 2>&1 || { echo "error: spel CLI not on PATH" >&2; exit 1; }

# `spel --version` prints the usage banner (no version flag), so pin the known
# installed version documented in README ("spel 0.2.0"). Override via env if needed.
SPEL_VERSION="${SPEL_VERSION:-0.2.0}"

spel -- generate-idl "$DEF" \
  | SPEL_VERSION="$SPEL_VERSION" DEF="$DEF" python3 scripts/_idl_annotate.py "$OUT"

echo "regenerated $OUT from $DEF via spel generate-idl ($SPEL_VERSION)" >&2
