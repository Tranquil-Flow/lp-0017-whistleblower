#!/usr/bin/env python3
"""Validate replay/demo artifacts for the LP-0017 submission.

This intentionally uses only the Python standard library so it can run in
minimal environments where cargo/nix/lgs are unavailable.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def main() -> None:
    demo = ROOT / "scripts" / "demo.sh"
    require(demo.exists(), "scripts/demo.sh must exist for evaluator replay")
    require(demo.stat().st_mode & 0o111, "scripts/demo.sh must be executable")

    rln_fix = ROOT / "scripts" / "fix_delivery_rln.sh"
    require(rln_fix.exists(), "scripts/fix_delivery_rln.sh must exist to repair upstream delivery_module librln installs")
    require(rln_fix.stat().st_mode & 0o111, "scripts/fix_delivery_rln.sh must be executable")

    body = read("scripts/demo.sh")
    for needle in [
        "RISC0_DEV_MODE=0",
        "lgs localnet start",
        "lgs build",
        "lgs deploy",
        "lgs basecamp install",
        "scripts/fix_delivery_rln.sh",
        "whistleblower-batch",
        "spel inspect",
    ]:
        require(needle in body, f"scripts/demo.sh missing replay step: {needle}")

    manifest = json.loads(read("ui/manifest.json"))
    require(manifest["main"]["darwin-arm64-dev"] == "whistleblower.dylib", "Basecamp dev manifest must point to whistleblower.dylib")
    require("storage_module" in manifest["dependencies"], "manifest must depend on storage_module")
    require("delivery_module" in manifest["dependencies"], "manifest must depend on delivery_module")

    metadata = json.loads(read("ui/metadata.json"))
    require(metadata["main"] == "whistleblower", "metadata main must match Basecamp plugin filename")
    require(metadata.get("dependencies") == ["storage_module", "delivery_module"], "metadata dependencies must load storage/delivery first")

    for cfg in ["ui/configs/storage_config.json", "ui/configs/delivery_config.json"]:
        json.loads(read(cfg))

    flake = read("flake.nix")
    require("lgx-portable" in flake, "flake must expose portable .lgx package")
    require("lgx" in flake, "flake must expose dev .lgx package")

    print("demo artifacts ok")


if __name__ == "__main__":
    main()
