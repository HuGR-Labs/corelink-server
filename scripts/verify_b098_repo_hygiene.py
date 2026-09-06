#!/usr/bin/env python3
"""Verify the repository-owned half of B-098.

The worktree/branch census in B-098 is intentionally an operator action: it is
machine-local state and must not be used as a CI gate.  This verifier covers the
portable part instead: workspace lint inheritance, the checked-in population
figures in ``CLAUDE.md``, and the semver release/tag contract.

An absent semver tag is an honest ``open`` result, not a verifier error.  Any
missing source, malformed count, drift, or malformed release evidence fails
closed.  The verifier never creates, moves, or pushes a tag.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TRACKER = ROOT / "crates/corelink-runbook-tracker/Cargo.toml"
CLAUDE = ROOT / "CLAUDE.md"
PACKET = ROOT / "docs/handoff/2026-09-05-b098-owner-action-packet.md"
TAG_DRAFT = ROOT / "docs/release/v1.0.0-GA-tag-draft-final.txt"

SEMVER_TAG = re.compile(
    r"^v"
    r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)
PACKAGE_COUNT = re.compile(r"\*\*(\d+) Rust packages\*\*")
CRATE_DIR_COUNT = re.compile(r"directory holds \*\*(\d+)\*\*")
EXTRA_PACKAGE_COUNT = re.compile(r"and (\d+) live under `tests/`, `tools/` and\s*`apps/`", re.MULTILINE)
OKF_COUNT = re.compile(r"\*\*(\d+) OKF concepts\*\*")
SPECS_COUNT = re.compile(
    r"\*\*(\d+) full-schema \+ (\d+) YAML-only \((\d+) total\)"
)

SKIP_ALL = {"_audits", "_archive", "_schemas", "_compliance"}
SKIP_SCHEMA = SKIP_ALL | {"_templates", "_followups"}


class VerificationError(RuntimeError):
    """The verifier could not establish a trustworthy result."""


@dataclass(frozen=True)
class Audit:
    package_count: int
    crate_dir_count: int
    okf_count: int
    specs_schema_count: int
    specs_yaml_only_count: int
    semver_tags: tuple[str, ...]
    issues: tuple[str, ...]

    @property
    def status(self) -> str:
        return "open" if not self.semver_tags else "ready-to-close"


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"required file is unreadable: {path}: {exc}") from exc


def _one(pattern: re.Pattern[str], text: str, label: str) -> int:
    matches = pattern.findall(text)
    if len(matches) != 1:
        raise VerificationError(f"CLAUDE.md must contain exactly one {label} count")
    return int(matches[0])


def package_count(root: Path = ROOT) -> int:
    """Count package manifests through Cargo's workspace resolver."""
    try:
        completed = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version=1"],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError as exc:
        raise VerificationError(f"cargo metadata could not run: {exc}") from exc
    if completed.returncode != 0:
        raise VerificationError(f"cargo metadata failed: {completed.stderr.strip()}")
    try:
        data = json.loads(completed.stdout)
        packages = data["packages"]
    except (KeyError, TypeError, json.JSONDecodeError) as exc:
        raise VerificationError("cargo metadata returned malformed JSON") from exc
    if not isinstance(packages, list) or not packages:
        raise VerificationError("cargo metadata returned no packages")
    return len(packages)


def okf_count(root: Path = ROOT) -> int:
    bundle = root / "docs/knowledge"
    if not bundle.is_dir():
        raise VerificationError(f"OKF bundle is missing: {bundle}")
    files = {
        path
        for path in bundle.rglob("*.md")
        if path.relative_to(bundle).as_posix() not in {"index.md", "log.md"}
    }
    if not files:
        raise VerificationError("OKF bundle contains no concepts")
    return len(files)


def specs_counts(root: Path = ROOT) -> tuple[int, int]:
    specs = root / "specs"
    if not specs.is_dir():
        raise VerificationError(f"spec corpus is missing: {specs}")
    all_files = [
        path
        for path in specs.rglob("*.md")
        if not any(part in SKIP_ALL for part in path.relative_to(specs).parts)
    ]
    if not all_files:
        raise VerificationError("spec corpus contains no documents")
    schema = [
        path
        for path in all_files
        if not any(part in SKIP_SCHEMA for part in path.relative_to(specs).parts)
    ]
    return len(schema), len(all_files) - len(schema)


def semver_tags(root: Path = ROOT) -> tuple[str, ...]:
    try:
        completed = subprocess.run(
            ["git", "tag", "--list"],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError as exc:
        raise VerificationError(f"git tag could not run: {exc}") from exc
    if completed.returncode != 0:
        raise VerificationError(f"git tag failed: {completed.stderr.strip()}")
    return tuple(sorted(tag for tag in completed.stdout.splitlines() if SEMVER_TAG.fullmatch(tag)))


def _check_release_contract(root: Path, tags: tuple[str, ...]) -> list[str]:
    issues: list[str] = []
    draft = root / TAG_DRAFT.relative_to(ROOT)
    packet = root / PACKET.relative_to(ROOT)
    if not draft.is_file():
        issues.append(f"release draft missing: {draft.relative_to(root)}")
    if not packet.is_file():
        issues.append(f"owner action packet missing: {packet.relative_to(root)}")
    else:
        packet_text = _read(packet)
        for marker in ("B-098", "no semver release tag", "not create a tag"):
            if marker.lower() not in packet_text.lower():
                issues.append(f"owner action packet missing required statement: {marker}")
    for tag in tags:
        body = _tag_body(root, tag)
        if body is None:
            issues.append(f"semver tag {tag} is not an annotated tag")
        else:
            for placeholder in ("__OWNER_SHA__", "__SREL_SHA__", "__OWNER_TIMESTAMP__", "__SREL_TIMESTAMP__", "__SREL_NAME__"):
                if placeholder in body:
                    issues.append(f"semver tag {tag} retains release placeholder {placeholder}")
            if "Signed-off-by:" not in body or "Co-Authored-By:" not in body:
                issues.append(f"semver tag {tag} lacks DCO/provenance trailers")
    return issues


def _tag_body(root: Path, tag: str) -> str | None:
    try:
        completed = subprocess.run(
            ["git", "cat-file", "-t", f"refs/tags/{tag}"],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
        )
        if completed.returncode != 0 or completed.stdout.strip() != "tag":
            return None
        body = subprocess.run(
            ["git", "cat-file", "-p", f"refs/tags/{tag}"],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError as exc:
        raise VerificationError(f"git tag inspection failed: {exc}") from exc
    if body.returncode != 0:
        raise VerificationError(f"cannot inspect semver tag {tag}")
    return body.stdout


def audit(root: Path = ROOT, *, claude_text: str | None = None, tracker_text: str | None = None) -> Audit:
    claude = _read(root / CLAUDE.relative_to(ROOT)) if claude_text is None else claude_text
    tracker = _read(root / TRACKER.relative_to(ROOT)) if tracker_text is None else tracker_text
    packages = package_count(root)
    crates = sum(1 for path in (root / "crates").iterdir() if path.is_dir())
    if crates == 0:
        raise VerificationError("crates directory contains no crate directories")
    okf = okf_count(root)
    schema, yaml_only = specs_counts(root)

    documented = {
        "package_count": _one(PACKAGE_COUNT, claude, "Rust package"),
        "crate_dir_count": _one(CRATE_DIR_COUNT, claude, "crate directory"),
        "extra_package_count": _one(EXTRA_PACKAGE_COUNT, claude, "non-crates package"),
        "okf_count": _one(OKF_COUNT, claude, "OKF concept"),
    }
    specs_match = SPECS_COUNT.search(claude)
    if specs_match is None:
        raise VerificationError("CLAUDE.md must contain the full-schema/YAML-only spec counts")
    documented["specs_schema_count"], documented["specs_yaml_only_count"], documented["specs_total_count"] = map(int, specs_match.groups())

    issues: list[str] = []
    expected = {
        "package_count": packages,
        "crate_dir_count": crates,
        "okf_count": okf,
        "specs_schema_count": schema,
        "specs_yaml_only_count": yaml_only,
        "specs_total_count": schema + yaml_only,
    }
    for key, value in expected.items():
        if documented[key] != value:
            issues.append(f"CLAUDE.md {key}={documented[key]} but repository reports {value}")
    if documented["extra_package_count"] != packages - crates:
        issues.append("CLAUDE.md non-crates package count does not equal metadata minus crates")
    if "cargo metadata --no-deps --format-version=1" not in claude:
        issues.append("CLAUDE.md must name the exact cargo metadata population command")
    if "python3 scripts/validate_okf.py" not in claude:
        issues.append("CLAUDE.md must name the OKF validator command")
    if "python3 scripts/validate_specs.py" not in claude:
        issues.append("CLAUDE.md must name the specs validator command")

    lint_header = re.search(r"(?m)^\[lints\]\s*\nworkspace\s*=\s*true\s*$", tracker)
    if lint_header is None:
        issues.append("corelink-runbook-tracker must inherit [lints] workspace = true")
    if re.search(r"(?m)^\[lints\.(?:rust|clippy)\]", tracker):
        issues.append("corelink-runbook-tracker must not duplicate local rust/clippy lint tables")

    tags = semver_tags(root)
    issues.extend(_check_release_contract(root, tags))
    return Audit(packages, crates, okf, schema, yaml_only, tags, tuple(issues))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="emit the audit as JSON")
    args = parser.parse_args(argv)
    try:
        result = audit()
    except VerificationError as exc:
        print(f"B-098 verifier error (fail-closed): {exc}", file=sys.stderr)
        return 2
    if args.json:
        print(json.dumps(asdict(result) | {"status": result.status}, sort_keys=True))
    else:
        print(
            f"B-098 {result.status}: semver_tags={len(result.semver_tags)} "
            f"packages={result.package_count} crates={result.crate_dir_count} "
            f"okf={result.okf_count} specs={result.specs_schema_count}+{result.specs_yaml_only_count}"
        )
        for issue in result.issues:
            print(f"FAIL: {issue}", file=sys.stderr)
    return 1 if result.issues else 0


if __name__ == "__main__":
    raise SystemExit(main())
