#!/usr/bin/env python3
"""Derive D02 and protected-main B-251 identities from the checker operation."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path


D02_COMMIT = "f88c6ca41868f6a02e78ba9f4357d3abf67da4be"
OPERATION = "crates/corelink-billing/tests/b251_identity_operation.rs"
MARKER = "B251_OPERATION_TRANSCRIPT_HEX="
FAILURES = "B251_OPERATION_FAILURES_HEX="
COUNT = "B251_OPERATION_COUNT="
OPERATION_SEED_HEX = "b2510025d002d003"


class CollectionError(RuntimeError):
    pass


def git(directory: Path, *args: str) -> str:
    result = subprocess.run(("git", *args), cwd=directory, capture_output=True, text=True, check=True)
    return result.stdout.strip()


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def compare(left: dict[str, str], right: dict[str, str]) -> None:
    mismatches = [field for field in ("seed", "failure", "blob") if left[field] != right[field]]
    if mismatches:
        raise CollectionError("D02 operation identity mismatch: " + ", ".join(mismatches))


def run_operation(directory: Path, adapter: Path) -> dict[str, str]:
    destination = directory / OPERATION
    destination.parent.mkdir(parents=True, exist_ok=True)
    if adapter.resolve() != destination.resolve():
        shutil.copyfile(adapter, destination)
    command = (
        "cargo", "test", "--manifest-path", str(directory / "Cargo.toml"),
        "-p", "corelink-billing", "--test", "b251_identity_operation",
        "operation_emits_transcript", "--", "--exact", "--nocapture",
    )
    result = subprocess.run(command, cwd=directory, capture_output=True, text=True, timeout=900)
    if result.returncode:
        raise CollectionError(f"checker operation failed at {git(directory, 'rev-parse', 'HEAD')}")
    output = result.stdout + "\n" + result.stderr
    records = {key: [line[len(key):] for line in output.splitlines() if line.startswith(key)]
               for key in (COUNT, MARKER, FAILURES)}
    if any(len(values) != 1 for values in records.values()) or records[COUNT][0] != "1000":
        raise CollectionError("checker operation did not emit one complete 1,000-case transcript")
    try:
        transcript = bytes.fromhex(records[MARKER][0])
        failures = bytes.fromhex(records[FAILURES][0])
    except ValueError as exc:
        raise CollectionError("checker operation transcript was malformed") from exc
    return {
        "seed": digest(bytes.fromhex(OPERATION_SEED_HEX)),
        "failure": digest(failures),
        "blob": digest(transcript),
    }


def collect(d02: Path, d03: Path, output: Path, run_id: str, run_attempt: str) -> dict[str, object]:
    d02_commit = git(d02, "rev-parse", "HEAD")
    if d02_commit != D02_COMMIT:
        raise CollectionError("D02 checkout is not the pinned immutable producer revision")
    d02_test = git(d02, "rev-parse", f"HEAD:{'crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs'}")
    d03_commit = git(d03, "rev-parse", "HEAD")
    d03_test = git(d03, "rev-parse", "HEAD:crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs")
    adapter_blob = git(d03, "hash-object", OPERATION)
    committed_adapter_blob = git(d03, "rev-parse", f"HEAD:{OPERATION}")
    if adapter_blob != committed_adapter_blob:
        raise CollectionError("operation adapter differs from the recorded D03 revision")
    left = run_operation(d02, d03 / OPERATION)
    right = run_operation(d03, d03 / OPERATION)

    # Negative mutation: identity comparison must reject even one altered digest.
    mutated = dict(right)
    mutated["blob"] = "0" * 64 if right["blob"] != "0" * 64 else "1" * 64
    try:
        compare(left, mutated)
    except CollectionError as error:
        if "blob" not in str(error):
            raise
        mutation = "PASS: altered blob identity rejected"
    else:
        raise CollectionError("negative mutation was not rejected")
    compare(left, right)

    output.parent.mkdir(parents=True, exist_ok=True)
    for name, source, identity in (("d02-identity.json", "D02", left),
                                   ("d03-identity.json", "D03-observed", right)):
        (output.parent / name).write_text(
            json.dumps({"source": source, **identity}, sort_keys=True) + "\n", encoding="utf-8"
        )
    receipt: dict[str, object] = {
        "schema": "corelink.b251.provenance.v1",
        "actions_run": {"id": run_id, "attempt": run_attempt},
        "d02": {"revision": d02_commit, "producer_test_blob": d02_test, "identity": left},
        "d03_observed": {"revision": d03_commit, "producer_test_blob": d03_test, "identity": right},
        "operation": {"adapter_blob": committed_adapter_blob, "cases": 1000,
                      "inputs": "D02 property ranges: used [0,900), quota [1000,2000), request_bytes [1,50)",
                      "fixture": "InMemoryAtomicQuotaChecker", "mutation": mutation},
        "comparison": {"fields": ["seed", "failure", "blob"], "match": True},
        "limitations": {"production_backend_exercised": False,
                         "production_latency_measured": False,
                         "backend_seam": "No production AtomicCasState/AtomicQuotaChecker adapter exists; D1ByteStore is a separate async path."},
    }
    output.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--d02", type=Path, required=True)
    parser.add_argument("--d03", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--run-attempt", required=True)
    args = parser.parse_args()
    receipt = collect(args.d02, args.d03, args.output, args.run_id, args.run_attempt)
    print("B-251 provenance collection: PASS (D02 and D03 operation digests match; mutation rejected)")
    print(f"D02 revision: {receipt['d02']['revision']}")
    print(f"D03 revision: {receipt['d03_observed']['revision']}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (CollectionError, OSError, subprocess.SubprocessError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
