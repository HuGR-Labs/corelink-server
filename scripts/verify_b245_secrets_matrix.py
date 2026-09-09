#!/usr/bin/env python3
"""Verify B-245's closed perf-credential scope and bounded mutations.

The two variables are genuine owner-provisioned credentials for one reviewed
measurement workflow, so their matrix rows are required. This focal verifier
also exercises the negative path: a missing, moved, or extra consumer must
fail rather than being hidden by a global allowlist.
"""

from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
VALIDATOR_PATH = ROOT / "scripts" / "validate_secrets_matrix.py"

# The drift gates intentionally allowlist exactly these two GC safety flags.
# Every other GC-shaped name remains visible as potential secret/config drift.
GC_SAFETY_FLAGS = frozenset({
    "GC_LIVE_DELETE",
    "GC_OBSERVATION_ONLY",
})
GC_UNCLASSIFIED_NAMES = frozenset({
    "GC_LIVE_DELETE_CONFIRM",
    "GC_R2_BUCKET",
    "GC_RUN_ID",
    "GC_VALIDATE_ONLY",
    "GC_ADMIN_TOKEN",
    "GC_OBSERVATION_ONLY_TOKEN",
})


def _load_validator():
    spec = importlib.util.spec_from_file_location("validate_secrets_matrix", VALIDATOR_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {VALIDATOR_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _must_reject(validator, root: Path, manifest, label: str) -> None:
    try:
        validator.validate_b245_perf_scope(root, manifest)
    except ValueError:
        return
    raise AssertionError(f"mutation was accepted: {label}")


def validate_gc_allowlist_contract(validator) -> None:
    """Keep the GC non-secret allowlist exact and fail closed."""
    for name in GC_SAFETY_FLAGS:
        if not validator.ALLOWLIST_REGEX.match(name):
            raise AssertionError(f"GC safety flag is not allowlisted: {name}")
    for name in GC_UNCLASSIFIED_NAMES:
        if validator.ALLOWLIST_REGEX.match(name):
            raise AssertionError(f"unclassified GC name was allowlisted: {name}")


def main() -> int:
    validator = _load_validator()
    manifest = validator.B245_PERF_SECRET_MANIFEST

    validate_gc_allowlist_contract(validator)

    # Confirm the real checkout's exact population and both canonical rows.
    validator.validate_b245_perf_scope(ROOT)
    matrix = validator.parse_matrix(ROOT / validator.MATRIX_FILE_REL)
    for name in manifest:
        if name not in matrix:
            raise AssertionError(f"B-245 matrix row missing: {name}")
        if validator.ALLOWLIST_REGEX.match(name):
            raise AssertionError(f"B-245 credential is globally allowlisted: {name}")

    # Build a minimal isolated source tree so each mutation reaches the real
    # scope checker without modifying the checkout.
    with tempfile.TemporaryDirectory(prefix="corelink-b245-mutation-") as temp:
        root = Path(temp)
        for name, paths in manifest.items():
            for rel in paths:
                target = root / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                existing = target.read_text(encoding="utf-8") if target.exists() else ""
                target.write_text(
                    existing + f'# B-245 consumer\nvalue = "{name}"\n',
                    encoding="utf-8",
                )
        validator.validate_b245_perf_scope(root, manifest)

        fresh_source = root / "scripts/collect_b102_b107_measurements.py"
        fresh_original = fresh_source.read_text(encoding="utf-8")
        fresh_source.write_text(
            fresh_original.replace('value = "CORELINK_FRESH_SESSION"\n', ""),
            encoding="utf-8",
        )
        _must_reject(validator, root, manifest, "missing expected consumer")
        fresh_source.write_text(fresh_original, encoding="utf-8")

        fresh_source.write_text(
            fresh_original.replace(
                'value = "CORELINK_FRESH_SESSION"\n',
                '# value = "CORELINK_FRESH_SESSION"\n',
            ),
            encoding="utf-8",
        )
        _must_reject(validator, root, manifest, "comment-only consumer mutation")
        fresh_source.write_text(fresh_original, encoding="utf-8")

        fresh_source.write_text(
            fresh_original.replace("CORELINK_FRESH_SESSION", "CORELINK_FRESH_SESSION_DRIFT"),
            encoding="utf-8",
        )
        _must_reject(validator, root, manifest, "string/name drift mutation")
        fresh_source.write_text(fresh_original, encoding="utf-8")

        forbidden = root / "scripts" / "forbidden-production-use.py"
        forbidden.parent.mkdir(parents=True, exist_ok=True)
        forbidden.write_text('token = os.environ["CORELINK_PERF_PAT"]\n', encoding="utf-8")
        _must_reject(validator, root, manifest, "new consumer outside exact scope")
        forbidden.unlink()

        perf_source = root / "scripts/collect_b105_same_lane.py"
        perf_original = perf_source.read_text(encoding="utf-8")
        fresh_source.write_text(
            fresh_original.replace('value = "CORELINK_FRESH_SESSION"\n', ""),
            encoding="utf-8",
        )
        perf_source.write_text(
            perf_original + 'value = "CORELINK_FRESH_SESSION"\n',
            encoding="utf-8",
        )
        _must_reject(validator, root, manifest, "credential moved to another collector")

    # Exercise the matrix parser's negative mutation as well: deleting a row
    # must remove the name from the parsed population, exposing code-only drift
    # to the normal validator.
    with tempfile.TemporaryDirectory(prefix="corelink-b245-matrix-") as temp:
        path = Path(temp) / "matrix.md"
        path.write_text(
            "| 242 | Perf PAT | `CORELINK_PERF_PAT` | collector |\n"
            "| 243 | Fresh session | `CORELINK_FRESH_SESSION` | collector |\n",
            encoding="utf-8",
        )
        mutated = path.read_text(encoding="utf-8").replace(
            "| 243 | Fresh session | `CORELINK_FRESH_SESSION` | collector |\n", ""
        )
        path.write_text(mutated, encoding="utf-8")
        if "CORELINK_FRESH_SESSION" in validator.parse_matrix(path):
            raise AssertionError("matrix-row deletion mutation was accepted")
        if path.read_text(encoding="utf-8").replace("B245_NONEXISTENT", "") != path.read_text(encoding="utf-8"):
            raise AssertionError("no-op mutation fixture unexpectedly changed matrix")

    print("verify_b245_secrets_matrix: PASS (exact scope + 6 mutations rejected; no-op unchanged)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
