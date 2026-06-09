#!/usr/bin/env bash
set -euo pipefail

export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/bin:$PATH"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

# Non-portable Basecamp app appends "Dev" to LOGOS_DATA_DIR internally.
# Therefore point LOGOS_DATA_DIR at .../LogosBasecamp, not .../LogosBasecampDev.
basecamp_data_base="${LOGOS_BASECAMP_DATA_BASE:-$HOME/Library/Application Support/Logos/LogosBasecamp}"
dev_profile="${basecamp_data_base}Dev"
global_plugin_dir="$dev_profile/plugins/whistleblower"

if [ "${LOGOS_ALLOW_EXISTING_BASECAMP:-0}" != "1" ]; then
  existing_basecamp="$(pgrep -fl 'LogosBasecamp|logos_host' || true)"
  if [ -n "$existing_basecamp" ]; then
    echo "error: Basecamp or logos_host is already running. Quit it before launching the demo:" >&2
    echo "$existing_basecamp" >&2
    echo "Set LOGOS_ALLOW_EXISTING_BASECAMP=1 only if you intentionally need a second instance." >&2
    exit 1
  fi
fi

if [ -L "$global_plugin_dir" ]; then
  echo "error: global Whistleblower plugin install is a symlink: $global_plugin_dir" >&2
  echo "Run scripts/install-global-basecamp.sh to install a real plugin directory before recording." >&2
  exit 1
fi

app="${LOGOS_BASECAMP_BIN:-}"
# lgs 0.1.1 removed `launch --dry-run`; resolve the Qt wrapper directly.
# `logos-basecamp` is the wrapper that sets QT/DYLD env then execs LogosBasecamp.
if [ -z "$app" ]; then
  app="$(ls -t "$repo_root"/.scaffold/cache/basecamp/*/app-result/bin/logos-basecamp 2>/dev/null | head -1 || true)"
fi
if [ -z "$app" ]; then
  app="$(ls -t "$HOME/Library/Caches/logos-scaffold/basecamp"/*/app-result/bin/logos-basecamp 2>/dev/null | head -1 || true)"
fi

if [ -z "$app" ] || [ ! -x "$app" ]; then
  echo "error: could not resolve executable Basecamp binary. Set LOGOS_BASECAMP_BIN=/path/to/logos-basecamp" >&2
  exit 1
fi

echo "Launching Basecamp binary: $app"
echo "LOGOS_DATA_DIR=$basecamp_data_base"
echo "Expected real dev profile: $dev_profile"

export NSSA_WALLET_HOME_DIR="${NSSA_WALLET_HOME_DIR:-$repo_root/../../.nssa-testnet-wallet}"
export NSSA_SEQUENCER_URL="${NSSA_SEQUENCER_URL:-https://testnet.lez.logos.co}"
export WHISTLEBLOWER_ANCHOR_CONFIRM_SECS="${WHISTLEBLOWER_ANCHOR_CONFIRM_SECS:-180}"
export WHISTLEBLOWER_PROGRAM_BIN="${WHISTLEBLOWER_PROGRAM_BIN:-$repo_root/ui/artifacts/whistleblower_registry.bin}"

echo "NSSA_WALLET_HOME_DIR=$NSSA_WALLET_HOME_DIR"
echo "NSSA_SEQUENCER_URL=$NSSA_SEQUENCER_URL"
echo "WHISTLEBLOWER_PROGRAM_BIN=$WHISTLEBLOWER_PROGRAM_BIN"
if [ ! -f "$WHISTLEBLOWER_PROGRAM_BIN" ]; then
  echo "error: WHISTLEBLOWER_PROGRAM_BIN does not exist: $WHISTLEBLOWER_PROGRAM_BIN" >&2
  echo "Build the docker/deployed guest first, or set WHISTLEBLOWER_PROGRAM_BIN explicitly." >&2
  exit 1
fi

LOGOS_DATA_DIR="$basecamp_data_base" "$app"
