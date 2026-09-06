#!/usr/bin/env python3
"""Verify the executable contract of the remediation roadmap (B-061).

The roadmap is a control document, not a second source of truth for measured
state.  This guard derives the repository-local counts from the same committed
objects the OKF and billing code use, then checks the explicitly marked
canonical projection in the roadmap.  The Actions census is an external,
dated snapshot; it is checked as a complete record rather than silently
re-derived from a local workflow-file count.

Any missing input, malformed marker, unavailable git object, or workflow path
drift is an error.  A partial answer is not a green answer.
"""

from __future__ import annotations

import argparse
from functools import lru_cache
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
ROADMAP = Path("docs/campaigns/remediation/ROADMAP.md")
METRICS = Path("reports/b061-remediation-roadmap.v1.json")
ACTIONS_EXPORT = Path("reports/b061-actions-census.v1.json")
BACKLOG = Path("BACKLOG.md")
CHANGELOG = Path("changelog.d/b061-remediation-roadmap.md")
WORKFLOW = Path(".github/workflows/backlog-verify.yml")
SCRIPT = Path("scripts/verify_b061_remediation_roadmap.py")
TEST = Path("tests/test_verify_b061_remediation_roadmap.py")

CANONICAL_KEYS = {
    "okf_documents",
    "okf_checkpoint_claims",
    "okf_orphan_anchors",
    "tier_kind_variants",
    "rate_ladder_variants",
    "actions_zero_success_lanes",
    "actions_ran_never_success_lanes",
    "actions_never_run_lanes",
    "actions_runs_without_success",
}
CANONICAL_MARKERS = {
    "ROADMAP.md": re.compile(
        r"<!-- B061-CANONICAL-METRICS\n(?P<body>.*?)\nB061-CANONICAL-METRICS -->",
        re.DOTALL,
    ),
    "metric": re.compile(
        r"^(?P<key>[a-z0-9_]+)=(?P<value>[0-9]+|unavailable)$", re.MULTILINE
    ),
}
CHECKPOINT_RE = re.compile(r"^checkpoint_sha:\s*\"?([0-9a-f]{8,40})", re.MULTILINE)
DEFERRED_RE = re.compile(r"^deferred:\s*", re.MULTILINE)
TIER_ENUM_RE = re.compile(r"pub\s+enum\s+TierKind\s*\{(?P<body>.*?)^\}", re.DOTALL | re.MULTILINE)
TIER_VARIANT_RE = re.compile(r"^\s*([A-Z][A-Za-z0-9_]*)\s*(?:,|=)", re.MULTILINE)
LADDER_RE = re.compile(r"TIER_RATE_LADDER:\s*\[\(Tier,\s*u32,\s*u32\);\s*(\d+)\]")


class RoadmapVerificationError(ValueError):
    """The roadmap contract cannot be established from committed inputs."""


def _read(repo_root: Path, relative: Path) -> str:
    path = repo_root / relative
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise RoadmapVerificationError(f"required input is unreadable: {relative}") from exc


def _git(repo_root: Path, *args: str) -> str:
    try:
        return subprocess.check_output(
            ("git", *args), cwd=repo_root, text=True, stderr=subprocess.PIPE
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise RoadmapVerificationError(f"git input unavailable: {' '.join(args)}") from exc


def _git_blob(repo_root: Path, relative: str, revision: str = "HEAD") -> str:
    try:
        return subprocess.check_output(
            ("git", "show", f"{revision}:{relative}"),
            cwd=repo_root,
            text=True,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        raise RoadmapVerificationError(f"committed OKF input unavailable: {relative}") from exc


def _canonical_metrics(roadmap: str) -> dict[str, int | str]:
    match = CANONICAL_MARKERS["ROADMAP.md"].search(roadmap)
    if not match:
        raise RoadmapVerificationError("ROADMAP.md has no B061 canonical metrics block")
    values: dict[str, int | str] = {}
    for item in CANONICAL_MARKERS["metric"].finditer(match.group("body")):
        key = item.group("key")
        if key in values:
            raise RoadmapVerificationError(f"duplicate B061 canonical metric: {key}")
        raw_value = item.group("value")
        values[key] = raw_value if raw_value == "unavailable" else int(raw_value)
    if set(values) != CANONICAL_KEYS:
        missing = sorted(CANONICAL_KEYS - set(values))
        extra = sorted(set(values) - CANONICAL_KEYS)
        raise RoadmapVerificationError(
            f"B061 canonical metric set drifted (missing={missing}, extra={extra})"
        )
    return values


def _derived_okf(repo_root: Path) -> tuple[int, int, int]:
    head = _git(repo_root, "rev-parse", "HEAD")
    return _derived_okf_cached(str(repo_root), head)


@lru_cache(maxsize=4)
def _derived_okf_cached(repo_root_string: str, head: str) -> tuple[int, int, int]:
    repo_root = Path(repo_root_string)
    files = [
        path
        for path in _git(repo_root, "ls-tree", "-r", "--name-only", head, "--", "docs/knowledge/").splitlines()
        if path.endswith(".md") and not re.search(r"/(index|log)\.md$", path)
    ]
    if not files:
        raise RoadmapVerificationError("OKF corpus enumeration is empty")
    reachable = set(_git(repo_root, "rev-list", head).splitlines())
    checkpoints: list[str] = []
    for relative in files:
        blob = _git_blob(repo_root, relative, head)
        # A deferred concept is intentionally outside the claimed population:
        # it carries provenance for future authoring but is not an active
        # checkpoint claim and must not manufacture an orphan in this census.
        if DEFERRED_RE.search(blob):
            continue
        match = CHECKPOINT_RE.search(blob)
        if match:
            checkpoints.append(match.group(1))
    if not checkpoints:
        raise RoadmapVerificationError("OKF corpus has no checkpoint claims")
    orphaned = 0
    for checkpoint in checkpoints:
        resolved = _git(repo_root, "rev-parse", f"{checkpoint}^{{commit}}")
        if resolved not in reachable:
            orphaned += 1
    return len(files), len(checkpoints), orphaned


def _derived_tiers(repo_root: Path) -> tuple[int, int]:
    tier_source = _read(repo_root, Path("crates/corelink-tier-selection/src/tier.rs"))
    enum = TIER_ENUM_RE.search(tier_source)
    if not enum:
        raise RoadmapVerificationError("TierKind enum disappeared or changed shape")
    variants = [m.group(1) for m in TIER_VARIANT_RE.finditer(enum.group("body"))]
    if not variants:
        raise RoadmapVerificationError("TierKind enum has no variants")
    rate_source = _read(repo_root, Path("crates/corelink-ratelimit/src/tier.rs"))
    ladder = LADDER_RE.search(rate_source)
    if not ladder:
        raise RoadmapVerificationError("canonical rate ladder declaration disappeared")
    return len(variants), int(ladder.group(1))


def _actions_metrics(
    repo_root: Path,
    metrics_path: Path,
    actions_export_path: Path | None = None,
) -> dict[str, int | str]:
    """Derive lane counts from the committed inventory, never from constants."""
    sidecar_path = repo_root / metrics_path
    try:
        sidecar = json.loads(sidecar_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise RoadmapVerificationError("B061 metrics sidecar is invalid or unreadable") from exc
    if not isinstance(sidecar, dict) or sidecar.get("version") != 1:
        raise RoadmapVerificationError("B061 metrics sidecar has an unsupported version")
    if set(sidecar) != {"version", "canonical_metrics", "actions_evidence"}:
        raise RoadmapVerificationError("B061 metrics sidecar has ambiguous fields")
    evidence = sidecar.get("actions_evidence")
    if not isinstance(evidence, dict):
        raise RoadmapVerificationError("B061 Actions evidence metadata is missing")
    required = {"as_of", "status", "export", "sha256", "source_query"}
    if set(evidence) != required or not isinstance(evidence["as_of"], str):
        raise RoadmapVerificationError("B061 Actions evidence metadata is incomplete")
    if evidence["status"] not in {"available", "unavailable"}:
        raise RoadmapVerificationError("B061 Actions evidence status is invalid")
    if evidence["export"] != ACTIONS_EXPORT.as_posix():
        raise RoadmapVerificationError("B061 Actions evidence export path drifted")
    export = actions_export_path or Path(evidence["export"])
    if not isinstance(export, Path) or (
        actions_export_path is None and (export.is_absolute() or ".." in export.parts)
    ):
        raise RoadmapVerificationError("B061 Actions evidence export path is unsafe")
    export_path = export if export.is_absolute() else repo_root / export
    try:
        raw_export = export_path.read_bytes()
    except OSError as exc:
        raise RoadmapVerificationError("B061 Actions evidence export is missing") from exc
    actual_digest = hashlib.sha256(raw_export).hexdigest()
    if actual_digest != evidence["sha256"]:
        raise RoadmapVerificationError("B061 Actions evidence export digest mismatches sidecar")
    try:
        export_data = json.loads(raw_export.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise RoadmapVerificationError("B061 Actions evidence export is invalid JSON") from exc
    if not isinstance(export_data, dict) or set(export_data) != {"version", "status", "as_of", "source", "lanes"}:
        raise RoadmapVerificationError("B061 Actions evidence export has ambiguous fields")
    if export_data["version"] != 1 or export_data["status"] != evidence["status"] or export_data["as_of"] != evidence["as_of"]:
        raise RoadmapVerificationError("B061 Actions evidence metadata disagrees with export")
    source = export_data["source"]
    if not isinstance(source, dict) or source.get("query") != evidence["source_query"]:
        raise RoadmapVerificationError("B061 Actions source query disagrees with export")
    lanes = export_data["lanes"]
    if not isinstance(lanes, list) or not lanes:
        raise RoadmapVerificationError("B061 Actions lane inventory is empty")
    seen: set[str] = set()
    for lane in lanes:
        if not isinstance(lane, dict) or set(lane) != {"workflow", "result"}:
            raise RoadmapVerificationError("B061 Actions lane record is malformed")
        workflow = lane["workflow"]
        if not isinstance(workflow, str) or workflow in seen or not workflow.startswith(".github/workflows/"):
            raise RoadmapVerificationError("B061 Actions lane identity is invalid or duplicated")
        if lane["result"] not in {"ran_no_success", "never_ran"}:
            raise RoadmapVerificationError("B061 Actions lane result is invalid")
        if not (repo_root / workflow).is_file():
            raise RoadmapVerificationError(f"B061 Actions workflow is missing: {workflow}")
        seen.add(workflow)
    ran = sum(lane["result"] == "ran_no_success" for lane in lanes)
    never = sum(lane["result"] == "never_ran" for lane in lanes)
    if evidence["status"] == "available":
        raise RoadmapVerificationError(
            "B061 Actions export claims available but has no per-lane run counts"
        )
    return {
        "actions_zero_success_lanes": len(lanes),
        "actions_ran_never_success_lanes": ran,
        "actions_never_run_lanes": never,
        "actions_runs_without_success": "unavailable",
    }


def _validate_projection(
    repo_root: Path,
    metrics: dict[str, int | str],
    *,
    roadmap_path: Path = ROADMAP,
    metrics_path: Path = METRICS,
) -> None:
    roadmap = _read(repo_root, roadmap_path)
    canonical = _canonical_metrics(roadmap)
    if canonical != metrics:
        mismatches = {
            key: (canonical[key], metrics[key])
            for key in sorted(CANONICAL_KEYS)
            if canonical[key] != metrics[key]
        }
        raise RoadmapVerificationError(f"ROADMAP canonical metrics drifted: {mismatches}")
    sidecar = _read(repo_root, metrics_path)
    try:
        data = json.loads(sidecar)
    except json.JSONDecodeError as exc:
        raise RoadmapVerificationError("B061 metrics sidecar is invalid JSON") from exc
    if set(data) != {"version", "canonical_metrics", "actions_evidence"} or data.get("version") != 1:
        raise RoadmapVerificationError("B061 metrics sidecar has ambiguous fields")
    if data.get("canonical_metrics") != metrics:
        raise RoadmapVerificationError("B061 metrics sidecar disagrees with derived metrics")


def _validate_projections(
    repo_root: Path,
    *,
    backlog_path: Path = BACKLOG,
    changelog_path: Path = CHANGELOG,
    workflow_path: Path = WORKFLOW,
) -> None:
    backlog = _read(repo_root, backlog_path)
    if not re.search(
        r"(?ms)^id: B-061\s+.*?^verify:\s*\|\s*\n\s+python3 scripts/verify_b061_remediation_roadmap\.py\s*$",
        backlog,
    ):
        raise RoadmapVerificationError("BACKLOG B-061 does not invoke its canonical verifier")
    if "B-061" not in _read(repo_root, changelog_path):
        raise RoadmapVerificationError("B061 changelog projection is missing its id")
    workflow = _read(repo_root, workflow_path)
    required_paths = {
        str(BACKLOG),
        str(ROADMAP),
        str(METRICS),
        str(ACTIONS_EXPORT),
        str(CHANGELOG),
        str(SCRIPT),
        str(TEST),
        "docs/knowledge/**",
        "crates/corelink-tier-selection/src/tier.rs",
        "crates/corelink-ratelimit/src/tier.rs",
    }
    missing_paths = sorted(path for path in required_paths if f'"{path}"' not in workflow)
    if missing_paths:
        raise RoadmapVerificationError(f"backlog workflow paths omit B061 inputs: {missing_paths}")
    if "python3 scripts/verify_b061_remediation_roadmap.py" not in workflow:
        raise RoadmapVerificationError("backlog workflow does not execute B061 verifier")


def verify(
    repo_root: Path = REPO_ROOT,
    *,
    roadmap_path: Path = ROADMAP,
    metrics_path: Path = METRICS,
    backlog_path: Path = BACKLOG,
    changelog_path: Path = CHANGELOG,
    workflow_path: Path = WORKFLOW,
    actions_export_path: Path | None = None,
) -> dict[str, int | str]:
    if _git(repo_root, "rev-parse", "--is-shallow-repository") != "false":
        raise RoadmapVerificationError("shallow clone: ancestry cannot be verified")
    documents, checkpoints, orphaned = _derived_okf(repo_root)
    tier_variants, rate_ladder = _derived_tiers(repo_root)
    actions = _actions_metrics(repo_root, metrics_path, actions_export_path)
    metrics = {
        "okf_documents": documents,
        "okf_checkpoint_claims": checkpoints,
        "okf_orphan_anchors": orphaned,
        "tier_kind_variants": tier_variants,
        "rate_ladder_variants": rate_ladder,
        **actions,
    }
    _validate_projection(repo_root, metrics, roadmap_path=roadmap_path, metrics_path=metrics_path)
    _validate_projections(
        repo_root,
        backlog_path=backlog_path,
        changelog_path=changelog_path,
        workflow_path=workflow_path,
    )
    return {**metrics, "status": "b061_remediation_roadmap_verified"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    args = parser.parse_args()
    try:
        report = verify(args.repo_root.resolve())
    except RoadmapVerificationError as exc:
        print(f"B061-ROADMAP: RED: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
