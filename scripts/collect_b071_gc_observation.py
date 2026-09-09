#!/usr/bin/env python3
"""Collect and verify B-071 production GC observations without delete authority.

The collector runs the already-deployed native observation binary once for every
explicit ``(tenant_id, region, run_id)`` scope and emits the owner-review packet
named by B-071.  It never discovers tenants, creates runs, or enables live GC.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ALLOWED_REGIONS = frozenset({"iad", "lhr", "nrt", "sam"})
REPORT_PREFIX = "gc_sweep report "
MAX_CANDIDATES = 250
REQUIRED_SECRET_ENV = (
    "CLOUDFLARE_ACCOUNT_ID",
    "D1_DATABASE_ID",
    "CF_API_TOKEN",
    "R2_TDK_HEX",
)
PASSTHROUGH_ENV = ("PATH", "SSL_CERT_FILE", "SSL_CERT_DIR")
IMAGE_DIGEST = re.compile(r"^(?:[^\s@]+@)?sha256:[0-9a-f]{64}$")
APPROVAL_STATES = frozenset({"PENDING_OWNER_REVIEW", "APPROVED", "REJECTED"})
ROOT_FIELDS = frozenset(
    {
        "schema_version",
        "captured_at",
        "image_digest",
        "runs",
        "tenant_region_population",
        "candidates_scanned",
        "reclaimable_count",
        "reclaimable_bytes",
        "delete_count",
        "deleted_bytes",
        "live_delete_flag",
        "approval",
        "operator",
    }
)
RUN_FIELDS = frozenset(
    {
        "run_id",
        "tenant_id",
        "region",
        "started_at",
        "completed_at",
        "candidates_scanned",
        "reclaimable_count",
        "reclaimable_bytes",
        "delete_count",
        "deleted_bytes",
        "skipped_grace_pending",
        "skipped_refcount_non_zero",
        "already_resolved",
        "phase_budget",
        "max_candidates",
        "verdict",
    }
)
NATIVE_REPORT_FIELDS = frozenset(
    {
        "schema_version",
        "mode",
        "observation_only",
        "run_id",
        "tenant_id",
        "region",
        "candidates_scanned",
        "reclaimable_count",
        "reclaimable_bytes",
        "delete_count",
        "deleted_bytes",
        "skipped_grace_pending",
        "skipped_refcount_non_zero",
        "already_resolved",
        "duration_ms",
        "observed_at_ms",
        "phase_budget_ms",
        "max_candidates",
    }
)


class ObservationError(ValueError):
    """A fail-closed input, execution, or evidence error."""


def _regular_file(path: Path, label: str, *, executable: bool = False) -> None:
    try:
        metadata = path.lstat()
    except OSError as exc:
        raise ObservationError(f"{label} is unavailable: {path}: {exc}") from exc
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise ObservationError(f"{label} must be a regular non-symlink file: {path}")
    if executable and not os.access(path, os.X_OK):
        raise ObservationError(f"{label} is not executable: {path}")


def _read_json(path: Path, label: str) -> Any:
    _regular_file(path, label)
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ObservationError(
            f"{label} is not valid UTF-8 JSON: {path}: {exc}"
        ) from exc


def _uuid(value: Any, label: str) -> str:
    if not isinstance(value, str):
        raise ObservationError(f"{label} must be a UUID string")
    try:
        parsed = uuid.UUID(value)
    except ValueError as exc:
        raise ObservationError(f"{label} must be a UUID string") from exc
    if str(parsed) != value.lower():
        raise ObservationError(f"{label} must use canonical lowercase UUID form")
    return str(parsed)


def _uint(value: Any, label: str, maximum: int | None = None) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ObservationError(f"{label} must be a non-negative integer")
    if maximum is not None and value > maximum:
        raise ObservationError(f"{label} exceeds {maximum}")
    return value


def _nonempty(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip() or value != value.strip():
        raise ObservationError(f"{label} must be a non-empty trimmed string")
    return value


def _timestamp(value: Any, label: str) -> datetime:
    value = _nonempty(value, label)
    if not value.endswith("Z"):
        raise ObservationError(f"{label} must be an RFC3339 UTC timestamp")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as exc:
        raise ObservationError(f"{label} must be an RFC3339 UTC timestamp") from exc
    if parsed.tzinfo != timezone.utc:
        raise ObservationError(f"{label} must be an RFC3339 UTC timestamp")
    return parsed


def load_scopes(path: Path) -> list[dict[str, str]]:
    document = _read_json(path, "scope manifest")
    if not isinstance(document, dict) or set(document) != {"schema_version", "scopes"}:
        raise ObservationError(
            "scope manifest must contain exactly schema_version and scopes"
        )
    if document["schema_version"] != 1:
        raise ObservationError("scope manifest schema_version must be 1")
    raw_scopes = document["scopes"]
    if not isinstance(raw_scopes, list) or not raw_scopes:
        raise ObservationError("scope manifest scopes must be a non-empty list")
    scopes: list[dict[str, str]] = []
    identities: set[tuple[str, str]] = set()
    run_ids: set[str] = set()
    for index, raw in enumerate(raw_scopes):
        label = f"scopes[{index}]"
        if not isinstance(raw, dict) or set(raw) != {
            "tenant_id",
            "region",
            "run_id",
            "bucket",
        }:
            raise ObservationError(
                f"{label} must contain exactly tenant_id, region, run_id, bucket"
            )
        tenant_id = _uuid(raw["tenant_id"], f"{label}.tenant_id")
        run_id = _uuid(raw["run_id"], f"{label}.run_id")
        region = _nonempty(raw["region"], f"{label}.region")
        if region not in ALLOWED_REGIONS:
            raise ObservationError(
                f"{label}.region must be one of {sorted(ALLOWED_REGIONS)}"
            )
        bucket = _nonempty(raw["bucket"], f"{label}.bucket")
        identity = (tenant_id, region)
        if identity in identities:
            raise ObservationError(
                f"duplicate tenant/region scope: {tenant_id}/{region}"
            )
        if run_id in run_ids:
            raise ObservationError(f"duplicate run_id: {run_id}")
        identities.add(identity)
        run_ids.add(run_id)
        scopes.append(
            {
                "tenant_id": tenant_id,
                "region": region,
                "run_id": run_id,
                "bucket": bucket,
            }
        )
    return scopes


def _parse_report(stdout: str, scope: dict[str, str]) -> dict[str, Any]:
    reports = [
        line[len(REPORT_PREFIX) :]
        for line in stdout.splitlines()
        if line.startswith(REPORT_PREFIX)
    ]
    if len(reports) != 1:
        raise ObservationError(
            "native observation must emit exactly one structured report"
        )
    try:
        report = json.loads(reports[0])
    except json.JSONDecodeError as exc:
        raise ObservationError("native observation report is not valid JSON") from exc
    if not isinstance(report, dict) or set(report) != NATIVE_REPORT_FIELDS:
        raise ObservationError("native observation report has an unexpected schema")
    if report["schema_version"] != 1:
        raise ObservationError("native observation schema_version must be 1")
    for key, expected in (
        ("mode", "dry_run"),
        ("observation_only", True),
        ("tenant_id", scope["tenant_id"]),
        ("region", scope["region"]),
        ("run_id", scope["run_id"]),
    ):
        if report.get(key) != expected:
            raise ObservationError(f"native observation report has unexpected {key}")
    numeric = {}
    for key in (
        "candidates_scanned",
        "reclaimable_count",
        "reclaimable_bytes",
        "delete_count",
        "deleted_bytes",
        "skipped_grace_pending",
        "skipped_refcount_non_zero",
        "already_resolved",
        "duration_ms",
        "observed_at_ms",
        "phase_budget_ms",
        "max_candidates",
    ):
        numeric[key] = _uint(report.get(key), f"report.{key}")
    if numeric["max_candidates"] > MAX_CANDIDATES or numeric["max_candidates"] == 0:
        raise ObservationError("native observation max_candidates is outside 1..=250")
    if numeric["candidates_scanned"] > numeric["max_candidates"]:
        raise ObservationError("native observation scanned beyond max_candidates")
    classified = sum(
        numeric[key]
        for key in (
            "reclaimable_count",
            "skipped_grace_pending",
            "skipped_refcount_non_zero",
            "already_resolved",
        )
    )
    if classified != numeric["candidates_scanned"]:
        raise ObservationError(
            "native observation classifications do not equal candidates_scanned"
        )
    if numeric["delete_count"] != 0 or numeric["deleted_bytes"] != 0:
        raise ObservationError("native dry-run reported a deletion")
    if numeric["duration_ms"] > numeric["phase_budget_ms"]:
        raise ObservationError("native observation exceeded its phase budget")
    return numeric


def _iso8601(epoch_ms: int) -> str:
    return (
        datetime.fromtimestamp(epoch_ms / 1000, tz=timezone.utc)
        .isoformat()
        .replace("+00:00", "Z")
    )


def _child_env(scope: dict[str, str]) -> dict[str, str]:
    missing = [
        name for name in REQUIRED_SECRET_ENV if not os.environ.get(name, "").strip()
    ]
    if missing:
        raise ObservationError(
            "missing required read-only observation environment: " + ", ".join(missing)
        )
    env = {name: os.environ[name] for name in REQUIRED_SECRET_ENV}
    env.update(
        {name: os.environ[name] for name in PASSTHROUGH_ENV if os.environ.get(name)}
    )
    env.update(
        {
            "GC_OBSERVATION_ONLY": "true",
            "GC_LIVE_DELETE": "false",
            "GC_TENANT_ID": scope["tenant_id"],
            "GC_REGION": scope["region"],
            "GC_RUN_ID": scope["run_id"],
            "GC_R2_BUCKET": scope["bucket"],
        }
    )
    return env


def collect(
    *,
    scopes: list[dict[str, str]],
    binary: Path,
    image_digest: str,
    operator: str,
    timeout: int,
) -> dict[str, Any]:
    _regular_file(binary, "native observation binary", executable=True)
    if not IMAGE_DIGEST.fullmatch(image_digest):
        raise ObservationError("image digest must be an immutable sha256 reference")
    operator = _nonempty(operator, "operator")
    if timeout < 1 or timeout > 1_900:
        raise ObservationError("timeout must be in 1..=1900 seconds")
    runs: list[dict[str, Any]] = []
    for scope in scopes:
        started_at_ms = int(time.time() * 1000)
        try:
            process = subprocess.run(
                [str(binary)],
                env=_child_env(scope),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                encoding="utf-8",
                errors="strict",
                timeout=timeout,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired, UnicodeError) as exc:
            raise ObservationError(
                f"native observation failed for {scope['tenant_id']}/{scope['region']}"
            ) from exc
        completed_at_ms = int(time.time() * 1000)
        if process.returncode != 0:
            # Child output can contain provider diagnostics and is deliberately
            # not copied into logs or the evidence package.
            raise ObservationError(
                f"native observation exited non-zero for {scope['tenant_id']}/{scope['region']}"
            )
        metrics = _parse_report(process.stdout, scope)
        runs.append(
            {
                "run_id": scope["run_id"],
                "tenant_id": scope["tenant_id"],
                "region": scope["region"],
                "started_at": _iso8601(started_at_ms),
                "completed_at": _iso8601(completed_at_ms),
                "candidates_scanned": metrics["candidates_scanned"],
                "reclaimable_count": metrics["reclaimable_count"],
                "reclaimable_bytes": metrics["reclaimable_bytes"],
                "delete_count": 0,
                "deleted_bytes": 0,
                "skipped_grace_pending": metrics["skipped_grace_pending"],
                "skipped_refcount_non_zero": metrics["skipped_refcount_non_zero"],
                "already_resolved": metrics["already_resolved"],
                "phase_budget": {
                    "budget_ms": metrics["phase_budget_ms"],
                    "duration_ms": metrics["duration_ms"],
                },
                "max_candidates": metrics["max_candidates"],
                "verdict": "PASS",
            }
        )
    package: dict[str, Any] = {
        "schema_version": 1,
        "captured_at": _iso8601(int(time.time() * 1000)),
        "image_digest": image_digest,
        "runs": runs,
        "tenant_region_population": len(runs),
        "candidates_scanned": sum(run["candidates_scanned"] for run in runs),
        "reclaimable_count": sum(run["reclaimable_count"] for run in runs),
        "reclaimable_bytes": sum(run["reclaimable_bytes"] for run in runs),
        "delete_count": 0,
        "deleted_bytes": 0,
        "live_delete_flag": False,
        "approval": {
            "status": "PENDING_OWNER_REVIEW",
            "reviewer": None,
            "decided_at": None,
            "live_delete_authorized": False,
        },
        "operator": operator,
    }
    verify_package(package, expected_approval="pending")
    return package


def verify_package(package: Any, *, expected_approval: str = "any") -> None:
    if not isinstance(package, dict) or set(package) != ROOT_FIELDS:
        raise ObservationError("evidence package has an unexpected root schema")
    if package["schema_version"] != 1:
        raise ObservationError("evidence schema_version must be 1")
    captured_at = _timestamp(package["captured_at"], "captured_at")
    image_digest = _nonempty(package["image_digest"], "image_digest")
    if not IMAGE_DIGEST.fullmatch(image_digest):
        raise ObservationError("evidence image_digest must be immutable")
    _nonempty(package["operator"], "operator")
    runs = package["runs"]
    if not isinstance(runs, list) or not runs:
        raise ObservationError("evidence runs must be non-empty")
    identities: set[tuple[str, str]] = set()
    run_ids: set[str] = set()
    totals = {
        key: 0
        for key in (
            "candidates_scanned",
            "reclaimable_count",
            "reclaimable_bytes",
            "delete_count",
            "deleted_bytes",
        )
    }
    for index, run in enumerate(runs):
        label = f"runs[{index}]"
        if not isinstance(run, dict) or set(run) != RUN_FIELDS:
            raise ObservationError(f"{label} has an unexpected schema")
        tenant_id = _uuid(run["tenant_id"], f"{label}.tenant_id")
        run_id = _uuid(run["run_id"], f"{label}.run_id")
        region = _nonempty(run["region"], f"{label}.region")
        if region not in ALLOWED_REGIONS:
            raise ObservationError(f"{label}.region is not provisioned")
        if (tenant_id, region) in identities or run_id in run_ids:
            raise ObservationError(
                "evidence contains a duplicate tenant/region or run_id"
            )
        identities.add((tenant_id, region))
        run_ids.add(run_id)
        started_at = _timestamp(run["started_at"], f"{label}.started_at")
        completed_at = _timestamp(run["completed_at"], f"{label}.completed_at")
        if completed_at < started_at or captured_at < completed_at:
            raise ObservationError(f"{label} timestamps are not monotone")
        if run["verdict"] != "PASS":
            raise ObservationError(f"{label}.verdict must be PASS")
        for key in totals:
            value = _uint(run[key], f"{label}.{key}")
            totals[key] += value
        max_candidates = _uint(
            run["max_candidates"], f"{label}.max_candidates", MAX_CANDIDATES
        )
        if max_candidates == 0 or run["candidates_scanned"] > max_candidates:
            raise ObservationError(f"{label} exceeds its candidate ceiling")
        classifications = sum(
            _uint(run[key], f"{label}.{key}")
            for key in (
                "reclaimable_count",
                "skipped_grace_pending",
                "skipped_refcount_non_zero",
                "already_resolved",
            )
        )
        if classifications != run["candidates_scanned"]:
            raise ObservationError(f"{label} classifications do not balance")
        phase_budget = run["phase_budget"]
        if not isinstance(phase_budget, dict) or set(phase_budget) != {
            "budget_ms",
            "duration_ms",
        }:
            raise ObservationError(f"{label}.phase_budget has an unexpected schema")
        budget = _uint(phase_budget["budget_ms"], f"{label}.phase_budget.budget_ms")
        duration = _uint(
            phase_budget["duration_ms"], f"{label}.phase_budget.duration_ms"
        )
        if budget == 0 or duration > budget:
            raise ObservationError(f"{label} exceeded its phase budget")
    population = _uint(package["tenant_region_population"], "tenant_region_population")
    if population != len(identities):
        raise ObservationError("tenant_region_population does not match unique runs")
    for key, total in totals.items():
        aggregate = _uint(package[key], key)
        if aggregate != total:
            raise ObservationError(f"aggregate {key} does not match runs")
    if (
        totals["delete_count"] != 0
        or totals["deleted_bytes"] != 0
        or package["live_delete_flag"] is not False
    ):
        raise ObservationError("evidence is not a zero-delete dry-run")
    approval = package["approval"]
    if not isinstance(approval, dict) or set(approval) != {
        "status",
        "reviewer",
        "decided_at",
        "live_delete_authorized",
    }:
        raise ObservationError("approval has an unexpected schema")
    status_value = _nonempty(approval["status"], "approval.status")
    if status_value not in APPROVAL_STATES:
        raise ObservationError("approval.status is invalid")
    if approval["live_delete_authorized"] is not False:
        raise ObservationError(
            "B-071 observation evidence cannot authorize live deletion"
        )
    if status_value == "PENDING_OWNER_REVIEW":
        if approval["reviewer"] is not None or approval["decided_at"] is not None:
            raise ObservationError(
                "pending approval cannot name a reviewer or decision time"
            )
    else:
        _nonempty(approval["reviewer"], "approval.reviewer")
        _timestamp(approval["decided_at"], "approval.decided_at")
    expected = {
        "pending": "PENDING_OWNER_REVIEW",
        "approved": "APPROVED",
        "rejected": "REJECTED",
    }.get(expected_approval)
    if expected is not None and status_value != expected:
        raise ObservationError(f"approval.status must be {expected}")


def write_atomic(path: Path, package: dict[str, Any]) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    if path.exists() or path.is_symlink():
        _regular_file(path, "output")
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(package, handle, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    collect_parser = commands.add_parser(
        "collect", help="run explicit scopes and emit a pending owner-review package"
    )
    collect_parser.add_argument("--scopes", type=Path, required=True)
    collect_parser.add_argument(
        "--binary",
        type=Path,
        default=Path("/usr/local/bin/corelink-gc-sweep-production"),
    )
    collect_parser.add_argument("--image-digest", required=True)
    collect_parser.add_argument("--operator", required=True)
    collect_parser.add_argument("--output", type=Path, required=True)
    collect_parser.add_argument("--timeout-seconds", type=int, default=1_900)
    verify_parser = commands.add_parser(
        "verify", help="validate an existing evidence package"
    )
    verify_parser.add_argument("--evidence", type=Path, required=True)
    verify_parser.add_argument(
        "--expect-approval",
        choices=("any", "pending", "approved", "rejected"),
        default="any",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        if args.command == "collect":
            package = collect(
                scopes=load_scopes(args.scopes),
                binary=args.binary,
                image_digest=args.image_digest,
                operator=args.operator,
                timeout=args.timeout_seconds,
            )
            write_atomic(args.output, package)
            print(f"PASS: wrote zero-delete B-071 observation package to {args.output}")
        else:
            package = _read_json(args.evidence, "evidence package")
            verify_package(package, expected_approval=args.expect_approval)
            print(f"PASS: B-071 observation package is valid ({args.expect_approval})")
    except ObservationError as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
