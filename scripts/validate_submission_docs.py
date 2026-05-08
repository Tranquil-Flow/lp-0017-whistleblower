#!/usr/bin/env python3
"""Check that submission-facing docs match the current LP-0017 repo state.

Stdlib-only by design: this can run even in minimal review containers without
cargo, nix, or the Logos toolchain.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT.parent / "TASKS.md"
PR_DRAFT = ROOT / "PR_DRAFT.md"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def table_row(text: str, task: str) -> str:
    pattern = re.compile(rf"^\|\s*{re.escape(task)}\s*\|(.+)$", re.MULTILINE)
    match = pattern.search(text)
    require(match is not None, f"TASKS.md missing status row for {task}")
    return match.group(0)


def main() -> None:
    tasks = read(PLAN) if PLAN.exists() else ""
    readme = read(ROOT / "README.md")
    demo = read(ROOT / "DEMO.md")
    benchmarks = read(ROOT / "BENCHMARKS.md")
    measure_cu = read(ROOT / "scripts" / "measure_cu.sh")
    pr = read(PR_DRAFT) if PR_DRAFT.exists() else ""

    if tasks:
        row_17 = table_row(tasks, "1.7 — Basecamp UI plugin")
        require("✅ Done" in row_17, "TASKS.md must mark Basecamp UI plugin done after .lgx pipeline landed")
        require("dist/whistleblower-plugin.lgx" in row_17 or ".#lgx" in row_17, "Task 1.7 row must cite .lgx evidence")

        row_18 = table_row(tasks, "1.8 — E2E demo script")
        require("✅ Done" in row_18 or "🟡" in row_18, "TASKS.md must not call the demo script not-started")
        require("scripts/demo.sh" in row_18, "Task 1.8 row must cite scripts/demo.sh")

        row_111 = table_row(tasks, "1.11 — Narrated video demo")
        require("⚪ Not started" in row_111, "Narrated video should remain not started until an actual recording exists")

    require("scripts/demo.sh" in demo, "DEMO.md must point reviewers to scripts/demo.sh")
    require("python3 scripts/validate_demo_artifacts.py" in demo, "DEMO.md must include demo artifact validation command")
    if pr:
        require("lgs basecamp install" in pr, "PR draft pre-submit gates must include the lgs Basecamp install path")
    require("lgs basecamp install" in readme or "nix run  .#install" in readme, "README must document plugin install path")
    require("lez_adapter_anchor_50_cids_in_one_tx" in measure_cu, "scripts/measure_cu.sh must capture the 50-CID live benchmark path")
    require("TBD (needs anchor_spike --batch=50 flag" not in measure_cu, "scripts/measure_cu.sh must not leave the 50-CID benchmark as a stale TBD")
    require("50 is unverified" not in benchmarks, "BENCHMARKS.md must not contradict the completed 50-CID live benchmark")

    print("submission docs ok")


if __name__ == "__main__":
    main()
