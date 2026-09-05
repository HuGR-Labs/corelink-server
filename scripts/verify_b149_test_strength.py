#!/usr/bin/env python3
"""Fail-closed B-149 verifier pinned to the reviewed checkpoint registry.

`done` means these complete files are byte-for-byte the reviewed repair. A
legitimate edit is deliberately review-required: re-review it, then update the
corresponding SHA-256 checkpoint in this file and its mutation tests.
"""
from __future__ import annotations

import argparse
import hashlib
import os
import stat
import sys
from pathlib import Path
from types import MappingProxyType
from typing import Callable


AUDIT = "crates/corelink-container/src/storage/d1_audit_sink/tests_batch_limits.rs"
AUTH = "crates/corelink-container/src/routes/billing_ingest/tests_auth.rs"
VALIDATE = "crates/corelink-container/src/routes/billing_ingest/tests_validate_record.rs"
SKIP = "crates/corelink-container/src/routes/billing_ingest/tests_record_skip.rs"
INGEST = "crates/corelink-container/src/routes/billing_ingest.rs"

# SHA-256 of complete source files at the approved B-149 repair.  This is
# deliberately a read-only public view, not the authority used by `assess`:
# the authority is repeated as literals in `_validated_checkpoints` so that an
# accidental reassignment, omission, or extension cannot turn this verifier
# into a vacuous green check.
CHECKPOINTS = MappingProxyType({
    AUDIT: "450a24475c926d91dac2c89ab50647b3ee9cb68f8d167022ffe6d93441417824",
    AUTH: "db769f5120c69e362f0fcacf9c8a205e74097d81af009393646bd78226d99481",
    VALIDATE: "69d17ce8bffd1726f18dbaae7cb65550eadd2ef7740564c71ce80dce20f136b5",
    SKIP: "af709a075ac78c71b39611fb028b9c30c6cbc548ad561229428e9e8f11d1a6f8",
    INGEST: "b7d9016ce4a12fb73d14ea78f593ffbe0eda2cc52631063ae182d5a793b4f04e",
})
GAPS = (
    "empty_batch-no-empty-statement-assertion",
    "secret-absent-is-environment-conditional",
    "record-error-fixtures-or-reason-codes-not-exhaustive",
    "record-skip-delegation-does-not-reference-exhaustive-proof",
)


class InstrumentError(RuntimeError):
    """A required checkpoint cannot be read, so no verdict is trustworthy."""


def _validated_checkpoints() -> dict[str, str]:
    """Return precisely the reviewed five-file certificate, or fail closed.

    `CHECKPOINTS` is intentionally checked as configuration too.  The
    canonical literals below are the reviewed B-149 certificate; a missing,
    added, or changed entry is an instrument failure, never a zero-gap result.
    Keeping the public mapping read-only prevents ordinary accidental edits,
    while this comparison also catches module-level reassignment in tests and
    future refactors.
    """
    approved = {
        "crates/corelink-container/src/storage/d1_audit_sink/tests_batch_limits.rs": "450a24475c926d91dac2c89ab50647b3ee9cb68f8d167022ffe6d93441417824",
        "crates/corelink-container/src/routes/billing_ingest/tests_auth.rs": "db769f5120c69e362f0fcacf9c8a205e74097d81af009393646bd78226d99481",
        "crates/corelink-container/src/routes/billing_ingest/tests_validate_record.rs": "69d17ce8bffd1726f18dbaae7cb65550eadd2ef7740564c71ce80dce20f136b5",
        "crates/corelink-container/src/routes/billing_ingest/tests_record_skip.rs": "af709a075ac78c71b39611fb028b9c30c6cbc548ad561229428e9e8f11d1a6f8",
        "crates/corelink-container/src/routes/billing_ingest.rs": "b7d9016ce4a12fb73d14ea78f593ffbe0eda2cc52631063ae182d5a793b4f04e",
    }
    try:
        configured = dict(CHECKPOINTS)
    except (TypeError, ValueError) as error:
        raise InstrumentError(f"invalid checkpoint registry: {error}") from error
    if configured != approved:
        raise InstrumentError(
            "checkpoint registry must contain exactly the five approved B-149 path/digest pairs"
        )
    return approved


def _open_repo_root(root: Path) -> int:
    """Open the supplied root once, rejecting a symlink before traversal starts."""
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_DIRECTORY
    try:
        root_fd = os.open(root, flags)
    except OSError as error:
        raise InstrumentError(f"unsafe repo root: {error}") from error
    try:
        root_mode = os.fstat(root_fd).st_mode
    except OSError as error:
        os.close(root_fd)
        raise InstrumentError(f"cannot inspect repo root: {error}") from error
    if not stat.S_ISDIR(root_mode):
        os.close(root_fd)
        raise InstrumentError("repo root is not a directory")
    return root_fd


def _read_checkpoint(root_fd: int, relative: str) -> bytes:
    """Read a regular, non-symlinked file below an already-open root descriptor."""
    relative_path = Path(relative)
    if relative_path.is_absolute() or ".." in relative_path.parts or not relative_path.parts:
        raise InstrumentError(f"checkpoint path escapes repo root: {relative}")
    directory_fd = file_fd = -1
    # O_NONBLOCK is essential before fstat: opening a FIFO with O_RDONLY would
    # otherwise wait forever for a writer, preventing a fail-closed verdict.
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
    try:
        directory_fd = os.dup(root_fd)
        for component in relative_path.parts[:-1]:
            next_fd = os.open(component, flags | os.O_DIRECTORY, dir_fd=directory_fd)
            os.close(directory_fd)
            directory_fd = next_fd
        file_fd = os.open(relative_path.name, flags, dir_fd=directory_fd)
        if not stat.S_ISREG(os.fstat(file_fd).st_mode):
            raise InstrumentError(f"checkpoint is not a regular file: {relative}")
        with os.fdopen(file_fd, "rb", closefd=True) as source:
            file_fd = -1
            return source.read()
    except OSError as error:
        raise InstrumentError(f"unsafe checkpoint path {relative}: {error}") from error
    finally:
        if file_fd >= 0:
            os.close(file_fd)
        if directory_fd >= 0:
            os.close(directory_fd)


def checkpoint_digests(
    root: Path, *, after_root_open: Callable[[], None] | None = None
) -> dict[str, str]:
    """Return digests from one root descriptor; never re-resolve its pathname.

    ``after_root_open`` is an internal test seam for the root-replacement race:
    it runs only after the original path is already held by ``root_fd``.
    """
    checkpoints = _validated_checkpoints()
    root_fd = _open_repo_root(root)
    try:
        if after_root_open is not None:
            after_root_open()
        return {
            relative: hashlib.sha256(_read_checkpoint(root_fd, relative)).hexdigest()
            for relative in checkpoints
        }
    finally:
        os.close(root_fd)


def assess(
    root: Path, *, after_root_open: Callable[[], None] | None = None
) -> list[str]:
    """Return B-149 gaps. Zero gaps is possible only at every checkpoint."""
    checkpoints = _validated_checkpoints()
    actual = checkpoint_digests(root, after_root_open=after_root_open)
    drifted = {path for path, digest in actual.items() if digest != checkpoints[path]}
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
