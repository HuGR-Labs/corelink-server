#!/usr/bin/env python3
"""Run B-251's opt-in latency probe and emit fail-closed evidence.

This runner deliberately does not synthesize either side of the D02/D03
identity comparison. Both retained inputs must exist before Cargo runs. The
output is written atomically only after the identity comparison, exact test,
sample count, fixture boundary, and inclusive p99 limit all pass.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable, Sequence


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_D02_IDENTITY = ROOT / "reports/owner-actions/b251-d02-identity.json"
PROBE_MARKER = "B251_LATENCY_PROBE_JSON="
SAMPLE_COUNT = 1_000
LIMIT_US = 5_000
IDENTITY_FIELDS = ("seed", "failure", "blob")
COMMAND = (
    "cargo",
    "test",
    "-p",
    "corelink-billing",
    "--test",
    "quota_cas_prop_quota_cas",
    "real_latency_probe_under_5ms_p99",
    "--",
    "--ignored",
    "--exact",
    "--nocapture",
)
TEST_PATH = "crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs"


class ProbeError(RuntimeError):
    """A fail-closed B-251 precondition or measurement failure."""


def _load_identity(path: Path, expected_source: str) -> dict[str, str]:
    if path.is_symlink() or not path.is_file():
        raise ProbeError(f"identity input must be a regular non-symlink file: {path}")
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ProbeError(f"identity input is unreadable or invalid JSON: {path}") from exc
    required = {"source", *IDENTITY_FIELDS}
    if not isinstance(payload, dict) or set(payload) != required:
        raise ProbeError(f"identity input has wrong schema: {path}")
    if payload.get("source") != expected_source:
        raise ProbeError(f"identity input has wrong source: {path}")
    for field in IDENTITY_FIELDS:
        value = payload.get(field)
        if (
            not isinstance(value, str)
            or not value.strip()
            or len(value) > 256
            or any(ord(char) < 0x20 or ord(char) == 0x7f for char in value)
        ):
            raise ProbeError(f"identity input has empty {field}: {path}")
    return payload


def compare_identity(d02: dict[str, str], observed: dict[str, str]) -> None:
    mismatches = [field for field in IDENTITY_FIELDS if d02[field] != observed[field]]
    if mismatches:
        raise ProbeError("D02 identity mismatch: " + ", ".join(mismatches))


def _identity_receipt(identity: dict[str, str]) -> dict[str, str]:
    """Bind receipts without copying operator supplied values into artifacts."""
    return {
        f"{field}_sha256": hashlib.sha256(identity[field].encode("utf-8")).hexdigest()
        for field in IDENTITY_FIELDS
    }


def parse_probe_output(output: str) -> dict[str, object]:
    records = [line[len(PROBE_MARKER) :] for line in output.splitlines() if line.startswith(PROBE_MARKER)]
    if len(records) != 1:
        raise ProbeError(f"expected exactly one probe record, got {len(records)}")
    try:
        record = json.loads(records[0])
    except json.JSONDecodeError as exc:
        raise ProbeError("probe record is not valid JSON") from exc
    expected_keys = {
        "sample_count",
        "p99_us",
        "limit_us",
        "fixture",
        "production_latency_measured",
    }
    if not isinstance(record, dict) or set(record) != expected_keys:
        raise ProbeError("probe record has wrong schema")
    if record["sample_count"] != SAMPLE_COUNT:
        raise ProbeError("probe did not measure exactly 1,000 samples")
    if record["limit_us"] != LIMIT_US:
        raise ProbeError("probe reported an unexpected p99 limit")
    if record["fixture"] != "InMemoryAtomicQuotaChecker":
        raise ProbeError("probe did not use the declared isolated fixture")
    if record["production_latency_measured"] is not False:
        raise ProbeError("in-memory probe falsely claimed production latency")
    p99_us = record["p99_us"]
    if isinstance(p99_us, bool) or not isinstance(p99_us, int) or p99_us < 0:
        raise ProbeError("probe p99 is not a non-negative integer")
    if p99_us > LIMIT_US:
        raise ProbeError(f"probe p99 exceeded 5ms: {p99_us}us")
    return record


def _git_value(*args: str) -> str:
    result = subprocess.run(
        ("git", *args), cwd=ROOT, check=True, capture_output=True, text=True, timeout=30
    )
    value = result.stdout.strip()
    if not value:
        raise ProbeError(f"git {' '.join(args)} returned an empty value")
    return value


def _atomic_write(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink():
        raise ProbeError(f"refusing symlink output: {path}")
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, sort_keys=True, indent=2)
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


def _invalidate_output(path: Path) -> None:
    """Remove a prior regular artifact before spawning the measurement."""
    if path.is_symlink():
        raise ProbeError(f"refusing symlink output: {path}")
    if path.exists() and not path.is_file():
        raise ProbeError(f"output path is not a regular file: {path}")
    try:
        path.unlink(missing_ok=True)
    except OSError as exc:
        raise ProbeError(f"could not invalidate prior evidence: {path}") from exc


def run_probe(
    d02_path: Path,
    observed_path: Path,
    output_path: Path,
    runner: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> dict[str, object]:
    _invalidate_output(output_path)
    d02 = _load_identity(d02_path, "D02")
    observed = _load_identity(observed_path, "D03-observed")
    compare_identity(d02, observed)

    revision = _git_value("rev-parse", "HEAD")
    committed_test_blob = _git_value("rev-parse", f"HEAD:{TEST_PATH}")
    working_test_blob = _git_value("hash-object", TEST_PATH)
    if working_test_blob != committed_test_blob:
        raise ProbeError("probe source differs from the recorded revision")

    result = runner(
        COMMAND,
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
        timeout=900,
    )
    combined = result.stdout + "\n" + result.stderr
    if result.returncode != 0:
        raise ProbeError(f"focused Cargo probe failed with exit {result.returncode}")
    probe = parse_probe_output(combined)
    evidence: dict[str, object] = {
        "schema": "corelink.b251.latency-probe.v1",
        "revision": revision,
        "test_blob": committed_test_blob,
        "identity": {
            "baseline_source": "D02",
            "observed_source": "D03-observed",
            "fields_compared": list(IDENTITY_FIELDS),
            "match": True,
            "baseline": _identity_receipt(d02),
            "observed": _identity_receipt(observed),
        },
        "measurement": probe,
        "environment": {
            "os": platform.system(),
            "machine": platform.machine(),
        },
        "command": list(COMMAND),
    }
    _atomic_write(output_path, evidence)
    return evidence


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--allow-run", action="store_true", help="required opt-in for wall-clock measurement"
    )
    parser.add_argument("--d02-identity", type=Path, default=DEFAULT_D02_IDENTITY)
    parser.add_argument("--observed-identity", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    if not args.allow_run:
        raise ProbeError("--allow-run is required")
    run_probe(args.d02_identity, args.observed_identity, args.output)
    print("B-251 latency probe: PASS (identity matched; samples=1000; p99<=5ms)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ProbeError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
