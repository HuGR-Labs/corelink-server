#!/usr/bin/env python3
"""Fail-closed B-149 verifier pinned to approved candidate 627ec21.

`done` means these complete files are byte-for-byte the reviewed repair. A
legitimate edit is deliberately review-required: re-review it, then update the
corresponding SHA-256 checkpoint in this file and its mutation tests.
"""
from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path


AUDIT = "crates/corelink-container/src/storage/d1_audit_sink/tests_batch_limits.rs"
AUTH = "crates/corelink-container/src/routes/billing_ingest/tests_auth.rs"
VALIDATE = "crates/corelink-container/src/routes/billing_ingest/tests_validate_record.rs"
SKIP = "crates/corelink-container/src/routes/billing_ingest/tests_record_skip.rs"
INGEST = "crates/corelink-container/src/routes/billing_ingest.rs"

# SHA-256 of complete source files at approved B-149 repair 627ec21.
CHECKPOINTS = {
    AUDIT: "450a24475c926d91dac2c89ab50647b3ee9cb68f8d167022ffe6d93441417824",
    AUTH: "db769f5120c69e362f0fcacf9c8a205e74097d81af009393646bd78226d99481",
    VALIDATE: "69d17ce8bffd1726f18dbaae7cb65550eadd2ef7740564c71ce80dce20f136b5",
    SKIP: "af709a075ac78c71b39611fb028b9c30c6cbc548ad561229428e9e8f11d1a6f8",
    INGEST: "b7d9016ce4a12fb73d14ea78f593ffbe0eda2cc52631063ae182d5a793b4f04e",
}
GAPS = (
    "empty_batch-no-empty-statement-assertion",
    "secret-absent-is-environment-conditional",
    "record-error-fixtures-or-reason-codes-not-exhaustive",
    "record-skip-delegation-does-not-reference-exhaustive-proof",
)


class InstrumentError(RuntimeError):
    """A required checkpoint cannot be read, so no verdict is trustworthy."""


def checkpoint_digests(root: Path) -> dict[str, str]:
    """Return all protected-file digests or fail instead of making a vacuous call."""
    digests: dict[str, str] = {}
    for relative in CHECKPOINTS:
        source = root / relative
        if not source.is_file():
            raise InstrumentError(f"required source is missing: {relative}")
        try:
            digests[relative] = hashlib.sha256(source.read_bytes()).hexdigest()
        except OSError as error:
            raise InstrumentError(f"cannot read {relative}: {error}") from error
    return digests


def assess(root: Path) -> list[str]:
    """Return B-149 gaps. Zero gaps is possible only at every checkpoint."""
    actual = checkpoint_digests(root.resolve())
    drifted = {path for path, digest in actual.items() if digest != CHECKPOINTS[path]}
    gaps: list[str] = []
    if AUDIT in drifted:
        gaps.append(GAPS[0])
    if AUTH in drifted:
        gaps.append(GAPS[1])
    if VALIDATE in drifted or INGEST in drifted:
        gaps.append(GAPS[2])
    if SKIP in drifted:
        gaps.append(GAPS[3])
    return gaps


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--expect", choices=("open", "done"), required=True)
    args = parser.parse_args(argv)
    try:
        gaps = assess(args.root)
    except (OSError, InstrumentError) as error:
        print(f"instrument error: {error}", file=sys.stderr)
        return 2
    actual = "open" if gaps else "done"
    print(f"B-149 {actual}: {len(gaps)} gap(s)")
    for gap in gaps:
        print(f"- {gap} (source checkpoint drift; review required)")
    if actual != args.expect:
        print(f"expected {args.expect}, found {actual}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
