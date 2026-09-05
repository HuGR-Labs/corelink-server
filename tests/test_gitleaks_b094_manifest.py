"""Regression tests for the narrow B-094 manifest digest allowlist."""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

import pytest


REPO_ROOT = Path(__file__).resolve().parent.parent
CONFIG = REPO_ROOT / ".gitleaks.toml"


def _gitleaks() -> str:
    binary = shutil.which("gitleaks")
    if binary is None:
        pytest.skip("gitleaks is not installed")
    return binary


def _scan(source: Path, *, git: bool = False) -> subprocess.CompletedProcess[str]:
    args = [_gitleaks(), "detect"]
    if not git:
        args.append("--no-git")
    args.extend(
        [
            "--source",
            str(source),
            "--config",
            str(CONFIG),
            "--redact",
            "--no-banner",
            "--exit-code",
            "1",
        ]
    )
    return subprocess.run(args, capture_output=True, text=True, check=False)


def test_b094_manifest_baseline_has_no_findings(tmp_path: Path):
    copied = tmp_path / "scripts" / "b094_published_inventory.json"
    copied.parent.mkdir()
    # One generated entry is enough to exercise the exact path + line-shape
    # boundary without making every local pytest run rescan the 956-entry file.
    copied.write_text(
        '{\n  "population": {\n    "file_hashes": {\n'
        '      "apps/docs/docs/example.mdx": "'
        + "6b73d8c0f5a19e2b4d6087c3a9f1b5e7c2d4a6f8091b3d5e7f9a0c2e4b6d8f01"
        + '"\n    }\n  }\n}\n',
        encoding="utf-8",
    )
    result = _scan(tmp_path)
    assert result.returncode == 0, result.stdout + result.stderr


def test_real_secret_in_manifest_is_still_detected(tmp_path: Path):
    copied = tmp_path / "scripts" / "b094_published_inventory.json"
    copied.parent.mkdir()
    # This is a deterministic AWS-shaped mutation. It does not match the
    # digest-line exception, so the purpose-built rule must still fire.
    copied.write_text('{"mutation": "AKIAIOSFODNN7EXAMPL2"}\n', encoding="utf-8")
    result = _scan(tmp_path)
    assert result.returncode == 1, result.stdout + result.stderr
