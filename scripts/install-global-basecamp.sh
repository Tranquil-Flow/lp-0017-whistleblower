#!/usr/bin/env bash
set -euo pipefail

# Install this LP-0017 Whistleblower Basecamp package and its Storage/Delivery
# dependencies into the real local Logos Basecamp dev profile, not only the
# scaffold Alice/Bob profiles.

export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/bin:$PATH"

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if ! command -v lgs >/dev/null 2>&1; then
  echo "error: lgs not found. Install Logos scaffold CLI or add it to PATH." >&2
  exit 1
fi

if ! command -v nix >/dev/null 2>&1; then
  echo "error: nix not found. Logos Basecamp packages build through Nix." >&2
  exit 1
fi

app_data="${LOGOS_BASECAMP_DEV_DIR:-$HOME/Library/Application Support/Logos/LogosBasecampDev}"
modules_dir="$app_data/modules"
plugins_dir="$app_data/plugins"

mkdir -p "$modules_dir" "$plugins_dir"

echo "==> Repo: $repo_root"
echo "==> Target Basecamp dev data: $app_data"

echo "==> Preparing scaffold Basecamp toolchain"
lgs basecamp setup

echo "==> Building and installing scaffold profile packages"
lgs basecamp modules
lgs basecamp install

lgpm="$(python3 - <<'PY'
from pathlib import Path
home = Path.home()
patterns = [
    Path('.scaffold').glob('cache/basecamp/*/lgpm-result/bin/lgpm'),
    Path('.scaffold').glob('basecamp/*/lgpm-result/bin/lgpm'),
    (home / 'Library/Caches/logos-scaffold/basecamp').glob('*/lgpm-result/bin/lgpm'),
]
cands = []
for pat in patterns:
    cands.extend(sorted(pat))
if not cands:
    raise SystemExit('missing lgpm; lgs basecamp setup did not produce lgpm-result/bin/lgpm')
print(cands[-1])
PY
)"

lgx_list="$(python3 - <<'PY'
from pathlib import Path
home = Path.home()
roots = [
    Path('.scaffold/cache/basecamp/lgx-links'),
    home / 'Library/Caches/logos-scaffold/basecamp/lgx-links',
]
files = []
for root in roots:
    if root.exists():
        for entry in root.iterdir():
            target = entry.resolve()
            if target.is_dir():
                files.extend(target.glob('*.lgx'))
            elif target.suffix == '.lgx':
                files.append(target)
keep = []
for p in files:
    s = str(p)
    if any(token in s for token in ['logos_delivery_module', 'logos-delivery_module', 'logos_storage_module', 'logos-storage_module', 'lp_0017_whistleblower', 'whistleblower-plugin']):
        keep.append(p)
# Dependency modules first, project package last. Directory names beginning with
# github... are dependencies; path... is this project.
files = sorted({str(p): p for p in keep}.values(), key=lambda p: (0 if 'github' in str(p) else 1, str(p)))
for p in files:
    print(p)
PY
)"

if [ -z "$lgx_list" ]; then
  echo "error: no .lgx files found under project/global scaffold lgx-links" >&2
  exit 1
fi

count="$(printf '%s\n' "$lgx_list" | sed '/^$/d' | wc -l | tr -d ' ')"
echo "==> Installing $count LGX package(s) into real local Basecamp profile"
printf '%s\n' "$lgx_list" | while IFS= read -r lgx; do
  [ -n "$lgx" ] || continue
  echo "-- $lgx"
  "$lgpm" --modules-dir "$modules_dir" --ui-plugins-dir "$plugins_dir" install --file "$lgx"
done

echo "==> Installed manifests"
python3 - <<'PY'
from pathlib import Path
import json, os
base = Path(os.environ.get('LOGOS_BASECAMP_DEV_DIR', Path.home() / 'Library/Application Support/Logos/LogosBasecampDev'))
for rel in ['modules/delivery_module/manifest.json', 'modules/storage_module/manifest.json', 'plugins/whistleblower/manifest.json']:
    p = base / rel
    if not p.exists():
        raise SystemExit(f'missing expected installed artifact: {p}')
    data = json.loads(p.read_text())
    print(f'✓ {rel}: name={data.get("name")} version={data.get("version")}')
PY

cat <<'MSG'

Done. Launch local Basecamp with the normal app, or run:

  scripts/launch-global-basecamp.sh

For the video, show the Basecamp window only if Whistleblower is visibly present.
If the GUI shell is noisy, the terminal install/manifests/smoke evidence is still the honest fallback.
MSG
