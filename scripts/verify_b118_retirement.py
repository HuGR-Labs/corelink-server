#!/usr/bin/env python3
"""Verify the closed, retired state of the former B-118 OCI signing lane.

B-118 was resolved by removing ``cosign-sign.yml`` after inspecting the lane,
not by manufacturing a successful run.  The local retirement gate keeps that
decision executable: the former workflow must stay absent, the independent
release-SLSA and CAS signing paths must retain their sign/verify operations,
and the B-134 control ledger must continue to call the former lane retired.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = Path(".github/workflows/cosign-sign.yml")
RELEASE_WORKFLOW = Path(".github/workflows/release-slsa3.yml")
CAS_WORKFLOW = Path(".github/workflows/cas_foundation.yml")
LEDGER = Path("docs/campaigns/remediation/B-134-docker-shim-experiment.md")
CHANGELOG = Path("changelog.d/1490-remove-cosign-sign.md")
PUBLIC_SURFACES = (
    Path("apps/docs"),
    Path("legal"),
    Path("docs/internal/secrets-checklist.md"),
    Path("scripts/compliance-weekly-digest.py"),
)
STALE_PUBLIC_MARKERS = (
    r"cosign-sign\.yml",
    r"worker container image.*signed keyless",
    r"transparency-log entries for the \*\*worker container image\*\*",
)


class RetirementError(ValueError):
    """The B-118 retirement contract is missing or has been reopened."""


def _read(root: Path, relative: Path) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        raise RetirementError(f"missing/non-regular evidence: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise RetirementError(f"cannot read {relative}: {exc}") from exc


def _require_signing_round_trip(root: Path, relative: Path) -> None:
    text = _read(root, relative)
    for operation in ("sign-blob", "verify-blob"):
        if not re.search(rf"(?m)^\s*cosign\s+{re.escape(operation)}\b", text):
            raise RetirementError(f"{relative}: executable cosign {operation} path is missing")


def _check_public_surfaces(root: Path) -> None:
    patterns = tuple(re.compile(marker, re.IGNORECASE) for marker in STALE_PUBLIC_MARKERS)
    for relative in PUBLIC_SURFACES:
        path = root / relative
        if path.is_dir():
            files = sorted(path.rglob("*"))
        else:
            files = [path]
        for file in files:
            if not file.is_file() or file.is_symlink():
                continue
            try:
                text = file.read_text(encoding="utf-8")
            except UnicodeError:
                continue
            except OSError as exc:
                raise RetirementError(f"cannot read public surface {file}: {exc}") from exc
            for pattern in patterns:
                if pattern.search(text):
                    raise RetirementError(f"retired B-118 claim remains in {file}: {pattern.pattern}")


def verify(root: Path = ROOT) -> dict[str, str]:
    workflow = root / WORKFLOW
    if workflow.exists() or workflow.is_symlink():
        raise RetirementError(f"retired workflow was reintroduced: {WORKFLOW}")
    _require_signing_round_trip(root, RELEASE_WORKFLOW)
    _require_signing_round_trip(root, CAS_WORKFLOW)

    ledger = _read(root, LEDGER)
    if not re.search(r"(?m)^\|\s*cosign-sign\s*\|.*\b2026-09-08\b.*\|\s*RETIRED \(B-118\)\s*\|", ledger):
        raise RetirementError("B-134 ledger must retain the exact B-118 retirement row")
    if re.search(r"(?im)^\|\s*cosign-sign\s*\|.*\|\s*(?:PASS|GREEN|UNMEASURED)\s*\|", ledger):
        raise RetirementError("B-134 ledger must not promote the retired lane")

    changelog = _read(root, CHANGELOG)
    for marker in ("### Removed", "cosign-sign.yml", "release-SLSA", "CAS"):
        if marker not in changelog:
            raise RetirementError(f"retirement changelog is missing {marker!r}")
    _check_public_surfaces(root)
    return {"workflow": "absent", "release_slsa": "preserved", "cas": "preserved"}


def _expect_failure(root: Path, label: str) -> None:
    try:
        verify(root)
    except (RetirementError, OSError, UnicodeDecodeError):
        return
    raise RetirementError(f"self-test mutation escaped: {label}")


def self_test() -> int:
    """Exercise the closed-world mutations without contacting GitHub."""
    import shutil
    import tempfile

    with tempfile.TemporaryDirectory(prefix="b118-retirement-") as raw:
        root = Path(raw)
        for relative in (RELEASE_WORKFLOW, CAS_WORKFLOW, LEDGER, CHANGELOG):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text((ROOT / relative).read_text(encoding="utf-8"), encoding="utf-8")
        for relative in PUBLIC_SURFACES:
            source = ROOT / relative
            target = root / relative
            if source.is_dir():
                shutil.copytree(source, target)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(source.read_text(encoding="utf-8"), encoding="utf-8")
        verify(root)

        (root / WORKFLOW).parent.mkdir(parents=True, exist_ok=True)
        (root / WORKFLOW).write_text("name: stale\n", encoding="utf-8")
        _expect_failure(root, "workflow reintroduced")
        (root / WORKFLOW).unlink()

        ledger = root / LEDGER
        original = ledger.read_text(encoding="utf-8")
        ledger.write_text(original.replace("RETIRED (B-118)", "UNMEASURED", 1), encoding="utf-8")
        _expect_failure(root, "ledger promoted")
        ledger.write_text(original, encoding="utf-8")

        changelog = root / CHANGELOG
        original = changelog.read_text(encoding="utf-8")
        changelog.write_text(original.replace("release-SLSA", "release-path", 1), encoding="utf-8")
        _expect_failure(root, "independent signing evidence removed")
    print("B-118 retirement self-test: clean state and three fail-closed mutations passed")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            return self_test()
        verify(args.root.resolve())
    except (RetirementError, OSError, UnicodeDecodeError) as exc:
        print(f"B-118 retirement gate: FAIL: {exc}", file=sys.stderr)
        return 1
    print("B-118 retirement gate: PASS: former OCI lane absent; independent signing paths preserved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
