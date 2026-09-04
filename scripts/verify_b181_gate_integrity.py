#!/usr/bin/env python3
"""Verify the monotonic, two-stage introduction of the B181 verifier.

S0 (this file) is intentionally inert: no workflow is changed and no existing
policy is activated.  It is a trusted, Git-object-only verifier for the one
permitted bootstrap delta.  Run it from a checkout whose verifier is already
trusted (the S0 checkout after review) as::

    python3 scripts/verify_b181_gate_integrity.py --bootstrap BASE --git-ref CANDIDATE

The bootstrap contract is deliberately stricter than an ordinary code review:
the candidate may add exactly this verifier and may not add, remove, or mutate
any workflow, trigger, baseline, or existing policy script.  The next (S2)
activation change can then use the byte-exact trusted-base gate checks from the
reviewed 953c14d024ad35bf7f69746d5e09c4d8f1589985 lineage.  This provenance is
kept here so the S0/S2 boundary cannot be mistaken for self-protection.

Only Git blobs are read for BASE and CANDIDATE.  Candidate source is never
imported or executed.  Full object IDs and blob object types are required.
"""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
CHECKER_PATH = "scripts/verify_b181_gate_integrity.py"
PREEXISTING_POLICY_PATHS = (
    "scripts/concurrency_scope_scan.py",
    "scripts/verify-action-sha-pinning.py",
)
FORBIDDEN_BOOTSTRAP_PREFIXES = (
    ".github/workflows/",
    ".github/actionlint.yaml",
    ".actionlint.yaml",
    "config/concurrency-scope-baseline.txt",
    "reports/refactor/god-files-2026-08-31.tsv",
)


class IntegrityError(ValueError):
    """A bootstrap candidate failed the trusted S0 contract."""


def _require_commit(revision: str) -> None:
    if len(revision) != 40 or any(c not in "0123456789abcdefABCDEF" for c in revision):
        raise IntegrityError(f"revision is not a full 40-character SHA: {revision!r}")
    result = subprocess.run(
        ["git", "cat-file", "-t", revision],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False,
    )
    if result.returncode or result.stdout.strip() != b"commit":
        raise IntegrityError(f"revision is not a commit object: {revision}")


def _git_blob(revision: str, path: str) -> bytes:
    """Read exactly one blob from a commit; never fall back to the worktree."""
    _require_commit(revision)
    spec = f"{revision}:{path}"
    kind = subprocess.run(
        ["git", "cat-file", "-t", spec],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False,
    )
    if kind.returncode or kind.stdout.strip() != b"blob":
        raise IntegrityError(f"{revision} is missing blob {path}")
    blob = subprocess.run(
        ["git", "cat-file", "blob", spec],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False,
    )
    if blob.returncode:
        raise IntegrityError(f"cannot read Git blob {path} from {revision}")
    return blob.stdout


def _head_revision() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise IntegrityError("trusted checkout has no readable HEAD commit")
    revision = result.stdout.decode("ascii", errors="replace").strip()
    _require_commit(revision)
    return revision


def _changed_paths(base: str, candidate: str) -> tuple[str, ...]:
    """Return raw Git path names; NUL framing preserves newline filenames."""
    result = subprocess.run(
        ["git", "diff", "--no-renames", "--name-only", "-z", base, candidate, "--"],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise IntegrityError(f"cannot enumerate candidate diff: {detail}")
    raw_names = result.stdout.split(b"\0")
    if raw_names and raw_names[-1] == b"":
        raw_names.pop()
    try:
        return tuple(name.decode("utf-8") for name in raw_names)
    except UnicodeDecodeError as error:
        raise IntegrityError(f"candidate contains a non-UTF-8 path: {error}") from error


def _verify_checker_addition_mode(base: str, candidate: str) -> None:
    """Require the sole permitted path to be a regular, non-executable add."""
    result = subprocess.run(
        ["git", "diff", "--no-renames", "--raw", "-z", base, candidate, "--", CHECKER_PATH],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise IntegrityError("cannot inspect bootstrap verifier mode")
    records = [record for record in result.stdout.split(b"\0") if record]
    if len(records) != 2:
        raise IntegrityError("bootstrap verifier must have exactly one Git change record")
    header, path = records
    fields = header.decode("ascii", errors="replace").split()
    if path.decode("utf-8", errors="replace") != CHECKER_PATH:
        raise IntegrityError("bootstrap verifier diff path is unexpected")
    if len(fields) != 5 or fields[-1] != "A":
        raise IntegrityError("bootstrap verifier must be an added regular file")
    if fields[0].lstrip(":") != "000000" or fields[1] != "100644":
        raise IntegrityError("bootstrap verifier permission/type changed")


def _verify_bootstrap_blobs(
    candidate: dict[str, bytes],
    trusted: dict[str, bytes],
    changed: tuple[str, ...],
    trusted_checker: bytes | None = None,
) -> None:
    allowed = {CHECKER_PATH}
    unexpected = sorted(set(changed) - allowed)
    if unexpected:
        raise IntegrityError(
            "bootstrap is not verifier-only; changed path(s): " + ", ".join(unexpected)
        )
    if CHECKER_PATH not in changed:
        raise IntegrityError("bootstrap did not add the verifier")
    for path, expected in trusted.items():
        actual = candidate.get(path)
        if actual is None:
            raise IntegrityError(f"bootstrap removed pre-existing policy {path}")
        if actual != expected:
            expected_sha = hashlib.sha256(expected).hexdigest()
            actual_sha = hashlib.sha256(actual).hexdigest()
            raise IntegrityError(
                f"bootstrap mutated pre-existing policy {path} "
                f"(sha256 {actual_sha}, expected {expected_sha})"
            )
    if not candidate.get(CHECKER_PATH):
        raise IntegrityError("bootstrap verifier blob is empty")
    if trusted_checker is not None and candidate[CHECKER_PATH] != trusted_checker:
        raise IntegrityError("bootstrap replaced the trusted verifier blob")


def verify_bootstrap(base: str, candidate_revision: str) -> int:
    """Check S0 using trusted base blobs and an exact candidate Git diff."""
    _require_commit(base)
    _require_commit(candidate_revision)
    trusted = {path: _git_blob(base, path) for path in PREEXISTING_POLICY_PATHS}
    candidate = {
        path: _git_blob(candidate_revision, path)
        for path in (*PREEXISTING_POLICY_PATHS, CHECKER_PATH)
    }
    changed = _changed_paths(base, candidate_revision)
    _verify_checker_addition_mode(base, candidate_revision)
    _verify_bootstrap_blobs(
        candidate,
        trusted,
        changed,
        trusted_checker=_git_blob(_head_revision(), CHECKER_PATH),
    )
    forbidden = sorted(
        path
        for path in changed
        if path.startswith(FORBIDDEN_BOOTSTRAP_PREFIXES)
    )
    if forbidden:  # defensive duplicate of the allowlist, with a useful reason
        raise IntegrityError("bootstrap touched forbidden policy path(s): " + ", ".join(forbidden))
    print(
        "B181 bootstrap OK: exactly one verifier added; existing policy/workflow/"
        "trigger/baseline blobs unchanged"
    )
    return 0


def self_test() -> int:
    """Exercise removal, mutation, no-op, and activation-shaped bootstrap deltas."""
    trusted = {
        path: b"trusted:" + path.encode("utf-8") for path in PREEXISTING_POLICY_PATHS
    }
    checker = b"#!/usr/bin/env python3\n# trusted B181 bootstrap verifier\n"
    accepted = {**trusted, CHECKER_PATH: checker}
    _verify_bootstrap_blobs(accepted, trusted, (CHECKER_PATH,), trusted_checker=checker)
    mutations: list[tuple[str, dict[str, bytes], tuple[str, ...]]] = [
        (
            "policy mutation",
            {**accepted, PREEXISTING_POLICY_PATHS[0]: b"changed"},
            (CHECKER_PATH, PREEXISTING_POLICY_PATHS[0]),
        ),
        (
            "policy removal",
            {**accepted, PREEXISTING_POLICY_PATHS[1]: None},  # type: ignore[dict-item]
            (CHECKER_PATH, PREEXISTING_POLICY_PATHS[1]),
        ),
        (
            "workflow activation",
            accepted,
            (CHECKER_PATH, ".github/workflows/concurrency-scope-gate.yml"),
        ),
        (
            "newline path",
            accepted,
            (CHECKER_PATH, "notes/unsafe\nworkflow.yml"),
        ),
        (
            "no-op verifier-only claim",
            accepted,
            tuple(),
        ),
        (
            "verifier replacement",
            {**trusted, CHECKER_PATH: b"mutated verifier"},
            (CHECKER_PATH,),
        ),
    ]
    for label, candidate, changed in mutations:
        try:
            _verify_bootstrap_blobs(candidate, trusted, changed, trusted_checker=checker)
        except IntegrityError:
            continue
        raise IntegrityError(f"self-test accepted {label}")
    print(f"B181 bootstrap self-test OK: {len(mutations)} non-additive mutations rejected")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Verify the additive B181 bootstrap foundation.")
    parser.add_argument("--bootstrap", metavar="BASE", help="trusted base commit SHA")
    parser.add_argument("--git-ref", metavar="CANDIDATE", help="candidate commit SHA")
    parser.add_argument("--self-test", action="store_true", help="run adversarial bootstrap tests")
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            if args.bootstrap or args.git_ref:
                parser.error("--self-test cannot be combined with --bootstrap/--git-ref")
            return self_test()
        if not args.bootstrap or not args.git_ref:
            parser.error("--bootstrap BASE --git-ref CANDIDATE is required")
        return verify_bootstrap(args.bootstrap, args.git_ref)
    except IntegrityError as error:
        print(f"B181 BOOTSTRAP FAILED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
