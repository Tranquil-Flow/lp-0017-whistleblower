#!/usr/bin/env bash
# scripts/fix_delivery_rln.sh — repair delivery_module's broken librln.dylib link.
#
# Why this exists: see BUGS_FILED.md §8. The upstream `logos-co/logos-delivery-module`
# Nix flake builds liblogosdelivery.dylib against librln.dylib (Zerokit/RLN) using a
# hardcoded build-time sandbox path, and does not include librln.dylib in the
# installed module output. On macOS arm64 the dlopen fails with:
#
#   Library not loaded: /nix/var/nix/builds/nix-872-90086794/source/target/release/deps/librln.dylib
#
# This script idempotently:
#   1. Locates librln.dylib in the local Nix store (zerokit output).
#   2. For each known module install dir (per-profile xdg-data + system Application Support),
#      copies librln.dylib next to liblogosdelivery.dylib.
#   3. Sets the copied librln.dylib's self-install_name to @loader_path/librln.dylib.
#   4. Rewrites liblogosdelivery.dylib's load command for librln.dylib to @loader_path/librln.dylib.
#
# Re-run after any `lgs basecamp launch <profile>` that did NOT use `--no-clean`,
# since the clean-slate scrub re-extracts the broken upstream module output.
#
# Usage:
#   scripts/fix_delivery_rln.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

LIBRLN_CANDIDATES="$(find /nix/store -maxdepth 4 -name librln.dylib 2>/dev/null || true)"
LIBRLN_SRC="$(printf '%s\n' "$LIBRLN_CANDIDATES" | awk 'index($0, "/zerokit-") && /\/lib\/librln\.dylib$/ { print; exit }')"
[[ -n "$LIBRLN_SRC" ]] || LIBRLN_SRC="$(printf '%s\n' "$LIBRLN_CANDIDATES" | awk 'NF { print; exit }')"
if [[ -z "$LIBRLN_SRC" ]]; then
    echo "fix_delivery_rln: ERROR — could not find zerokit librln.dylib in /nix/store." >&2
    echo "  Try: nix build .#lgx (or whatever pulls zerokit into the store)." >&2
    exit 1
fi
echo "fix_delivery_rln: source librln = $LIBRLN_SRC"

INT="$(find /nix/store -maxdepth 4 -name install_name_tool 2>/dev/null | awk '/\/bin\/install_name_tool$/ { print; exit }' || true)"
[[ -x "$INT" ]] || INT="$(xcrun --find install_name_tool 2>/dev/null || true)"
if [[ -z "$INT" || ! -x "$INT" ]]; then
    echo "fix_delivery_rln: ERROR — install_name_tool not found." >&2
    exit 1
fi

OTOOL="$(find /nix/store -maxdepth 4 -name otool 2>/dev/null | awk '/\/bin\/otool$/ { print; exit }' || true)"
[[ -x "$OTOOL" ]] || OTOOL="$(xcrun --find otool 2>/dev/null || echo otool)"

BROKEN_PATH="/nix/var/nix/builds/nix-872-90086794/source/target/release/deps/librln.dylib"

patch_dir() {
    local dir="$1"
    [[ -d "$dir" ]] || return 0
    [[ -f "$dir/liblogosdelivery.dylib" ]] || return 0
    echo "fix_delivery_rln: patching $dir"

    local librln="$dir/librln.dylib"
    local need_copy=1
    if [[ -f "$librln" ]] && cmp -s "$LIBRLN_SRC" "$librln"; then
        need_copy=0
    fi
    if (( need_copy )); then
        cp "$LIBRLN_SRC" "$librln"
        chmod u+w "$librln"
        "$INT" -id @loader_path/librln.dylib "$librln"
        echo "  - copied librln.dylib + set @loader_path id"
    else
        echo "  - librln.dylib already present (matches /nix/store source)"
    fi

    local logos="$dir/liblogosdelivery.dylib"
    local current_ref
    current_ref="$("$OTOOL" -L "$logos" 2>/dev/null | awk '/librln\.dylib/ { print $1; exit }' || true)"
    case "$current_ref" in
        "@loader_path/librln.dylib")
            echo "  - liblogosdelivery already references @loader_path/librln.dylib"
            ;;
        "")
            echo "  - WARN: liblogosdelivery has no librln dep (already detached?); skipping"
            ;;
        *)
            chmod u+w "$logos"
            "$INT" -change "$current_ref" @loader_path/librln.dylib "$logos"
            echo "  - rewrote liblogosdelivery librln ref: $current_ref -> @loader_path/librln.dylib"
            ;;
    esac
}

PROFILES_ROOT="$ROOT/.scaffold/basecamp/profiles"
if [[ -d "$PROFILES_ROOT" ]]; then
    for prof_dir in "$PROFILES_ROOT"/*/; do
        [[ -d "$prof_dir" ]] || continue
        patch_dir "$prof_dir/xdg-data/Logos/LogosBasecampDev/modules/delivery_module"
    done
fi

patch_dir "$HOME/Library/Application Support/Logos/LogosBasecampDev/modules/delivery_module"

echo "fix_delivery_rln: done."
