#!/usr/bin/env python3
"""Mutation guard: each safety-critical B-072 deletion must fail verification."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERIFY = ROOT / "scripts/verify_b072_receiver.py"


def run_guard(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(root / "scripts/verify_b072_receiver.py"), "--root", str(root)],
        check=False,
        capture_output=True,
        text=True,
    )


def mutate_and_require_failure(name: str, relative: str, old: str, new: str) -> None:
    # The repository test runners may have no writable system temp volume;
    # keep the short-lived copy beside this small fixture instead.
    scratch = ROOT / ".mutation-tmp"
    scratch.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f"b072-{name}-", dir=scratch) as directory:
        copy_root = Path(directory)
        for relative_path in (
            "wrangler.toml",
            "pnpm-workspace.yaml",
            "worker/src/index_schedule.ts",
            "apps/synthetic-pager-worker/wrangler.toml",
            "apps/synthetic-pager-worker/src/index.ts",
            "apps/synthetic-pager-worker/src/contract.ts",
            "migrations/d1/0116_synthetic_page_delivery_lifecycle.sql",
            "scripts/verify_b072_receiver.py",
            ".github/workflows/synthetic-pager-worker-deploy.yml",
        ):
            target = copy_root / relative_path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative_path, target)
        target = copy_root / relative
        text = target.read_text()
        if old not in text:
            raise AssertionError(f"mutation {name} did not find its target")
        target.write_text(text.replace(old, new, 1))
        result = run_guard(copy_root)
        if result.returncode == 0:
            raise AssertionError(f"mutation {name} unexpectedly passed")


def main() -> None:
    baseline = run_guard(ROOT)
    if baseline.returncode != 0:
        raise SystemExit(baseline.stderr or baseline.stdout)
    mutations = [
        (
            "remove-prod-guard",
            "apps/synthetic-pager-worker/src/contract.ts",
            'if (environment === "prod" || environment?.startsWith("prod-")) {',
            'if (false) {',
        ),
        (
            "enable-prod-receiver",
            "apps/synthetic-pager-worker/wrangler.toml",
            '[env.prod.triggers]\ncrons = []',
            '[env.prod.triggers]\ncrons = ["0 14 * * 1"]',
        ),
        (
            "remove-dedup-correlation",
            "apps/synthetic-pager-worker/src/index.ts",
            'if (envelope === null || deliveryId !== envelope.synthetic_page.dedup_key) {',
            'if (envelope === null) {',
        ),
        (
            "enable-by-default",
            "apps/synthetic-pager-worker/wrangler.toml",
            'SYNTHETIC_DRILL_ENABLED = "false"',
            'SYNTHETIC_DRILL_ENABLED = "true"',
        ),
        (
            "drop-scheduler-retry",
            "worker/src/index_schedule.ts",
            'if (!response.ok) {',
            'if (response.ok) {',
        ),
        (
            "enable-auto-deploy",
            ".github/workflows/synthetic-pager-worker-deploy.yml",
            "on:\n  workflow_dispatch:",
            "on:\n  push:",
        ),
        (
            "allow-production-input",
            ".github/workflows/synthetic-pager-worker-deploy.yml",
            "options: [staging]",
            "options: [staging, prod]",
        ),
    ]
    for mutation in mutations:
        mutate_and_require_failure(*mutation)
    print(f"B-072 receiver mutation guard: PASS ({len(mutations)} mutations rejected)")


if __name__ == "__main__":
    main()
