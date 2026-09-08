"""Mutation-backed structural contract for B-257's Cargo.lock entry."""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "scripts/verify_b257_cargo_lock_parity.py"
MANIFEST = ROOT / "crates/corelink-handler-customer/Cargo.toml"
LOCK = ROOT / "Cargo.lock"


def _fixture(tmp_path: Path) -> tuple[Path, Path]:
    manifest = tmp_path / "crates/corelink-handler-customer/Cargo.toml"
    manifest.parent.mkdir(parents=True)
    shutil.copy2(MANIFEST, manifest)
    lock = tmp_path / "Cargo.lock"
    shutil.copy2(LOCK, lock)
    return manifest, lock


def _run(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(VERIFIER), "--root", str(root)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def _mutate_customer_lock(lock: str, replacement: str) -> str:
    start = lock.index('name = "corelink-handler-customer"')
    end = lock.index("\n\n[[package]]", start)
    block = lock[start:end]
    mutated, count = re.subn(r'^ "uuid",\n', replacement, block, count=1, flags=re.MULTILINE)
    assert count == 1
    return lock[:start] + mutated + lock[end:]


def test_b257_baseline_is_green():
    result = _run(ROOT)
    assert result.returncode == 0, result.stdout + result.stderr


def test_b257_missing_lock_dependency_is_red_even_with_comment_bait(tmp_path: Path):
    _manifest, lock = _fixture(tmp_path)
    lock.write_text(
        _mutate_customer_lock(lock.read_text(encoding="utf-8"), "# uuid\n"),
        encoding="utf-8",
    )
    result = _run(tmp_path)
    assert result.returncode != 0
    assert "missing manifest dependencies" in (result.stdout + result.stderr)
