#!/usr/bin/env python3
"""Verify B-245's closed perf-credential scope and bounded mutations.

The two variables are genuine owner-provisioned credentials for one reviewed
measurement workflow, so their matrix rows are required. This focal verifier
also exercises the negative path: a missing, moved, or extra consumer must
fail rather than being hidden by a global allowlist.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
VALIDATOR_PATH = ROOT / "scripts" / "validate_secrets_matrix.py"

# The drift gates intentionally allowlist exactly these two GC safety flags.
# Active GC inputs remain visible in the matrix; no GC_* prefix is safe.
GC_SAFETY_FLAGS = frozenset({
    "GC_LIVE_DELETE",
    "GC_OBSERVATION_ONLY",
})
PUBLIC_NON_SECRET_CONFIG = frozenset({
    "PAGERDUTY_EVENTS_URL",
    "PAGERDUTY_SERVICE",
    "SLA_CREDITS_ENABLED",
    "SLA_OBSERVATIONS_ENABLED",
    "SYNTHETIC_DRILL_ENABLED",
})
GC_MATRIX_NAMES = frozenset({
    "GC_R2_BUCKET",
    "GC_RUN_ID",
    "GC_VALIDATE_ONLY",
})
GC_SENSITIVE_NAMES = frozenset({
    "GC_LIVE_DELETE_CONFIRM",
})
GC_UNCLASSIFIED_NAMES = frozenset({
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
    """Keep the two-name GC allowlist exact and fail closed elsewhere."""
    for name in GC_SAFETY_FLAGS | PUBLIC_NON_SECRET_CONFIG:
        if not validator.ALLOWLIST_REGEX.match(name):
            raise AssertionError(f"non-secret config is not allowlisted: {name}")
    for name in GC_MATRIX_NAMES | GC_SENSITIVE_NAMES | GC_UNCLASSIFIED_NAMES:
        if validator.ALLOWLIST_REGEX.match(name):
            raise AssertionError(f"sensitive/unclassified GC name was allowlisted: {name}")


def main() -> int:
    validator = _load_validator()
    manifest = validator.B245_PERF_SECRET_MANIFEST

    validate_gc_allowlist_contract(validator)

    # Confirm the real checkout's exact population and both canonical rows.
    validator.validate_b245_perf_scope(ROOT)
    matrix = validator.parse_matrix(ROOT / validator.MATRIX_FILE_REL)
    for name in GC_MATRIX_NAMES | GC_SENSITIVE_NAMES:
        if name not in matrix:
            raise AssertionError(f"sensitive GC matrix row missing: {name}")
        if validator.ALLOWLIST_REGEX.match(name):
            raise AssertionError(f"sensitive GC name is globally allowlisted: {name}")
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

        # This exact mock fixture is non-secret test data, but an additional
        # reference in the same test file must remain a closed-scope failure.
        fixture = root / "tests/test_b105_isolated_lane_behavior.py"
        fixture.parent.mkdir(parents=True, exist_ok=True)
        fixture.write_text(
            'with mock.patch.dict(collector.os.environ, {\n'
            '    "CORELINK_PERF_BASE": "https://cache.example.invalid",\n'
            '    "CORELINK_PERF_PAT": "redacted-test-token",\n'
            '    "GITHUB_RUN_ID": "4242",\n'
            '}, clear=False):\n'
            '    pass\n',
            encoding="utf-8",
        )
        validator.validate_b245_perf_scope(root, manifest)
        fixture.write_text(
            fixture.read_text(encoding="utf-8")
            + 'token = os.environ["CORELINK_PERF_PAT"]\n',
            encoding="utf-8",
        )
        _must_reject(
            validator,
            root,
            manifest,
            "second unclassified B-245 reference in fixture test file",
        )

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

    # Exercise matrix deletion through the normal gate, not just the parser:
    # deleting the live secret row must produce code-only drift and exit 1.
    with tempfile.TemporaryDirectory(prefix="corelink-b245-matrix-") as temp:
        root = Path(temp)
        (root / "Cargo.toml").write_text(
            "[package]\nname = 'matrix-mutation'\n", encoding="utf-8"
        )
        matrix = root / validator.MATRIX_FILE_REL
        matrix.parent.mkdir(parents=True, exist_ok=True)
        matrix_lines = (ROOT / validator.MATRIX_FILE_REL).read_text(
            encoding="utf-8"
        ).splitlines(keepends=True)
        fresh_rows = [
            line
            for line in matrix_lines
            if validator.MATRIX_ROW_RE.match(line) and "CORELINK_FRESH_SESSION" in line
        ]
        if len(fresh_rows) != 1:
            raise AssertionError(
                "expected exactly one CORELINK_FRESH_SESSION matrix row for deletion mutation"
            )
        matrix.write_text(
            "".join(line for line in matrix_lines if line != fresh_rows[0]),
            encoding="utf-8",
        )

        for relative in (
            ".github/workflows/perf-production-evidence.yml",
            "scripts/collect_b102_b107_measurements.py",
            "scripts/collect_b105_same_lane.py",
            "tests/test_b105_isolated_lane_behavior.py",
        ):
            source = ROOT / relative
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(source.read_bytes())

        prior_argv = sys.argv
        report = io.StringIO()
        diagnostics = io.StringIO()
        try:
            sys.argv = [
                "validate_secrets_matrix.py",
                "--repo-root",
                str(root),
                "--quiet",
            ]
            with contextlib.redirect_stdout(report), contextlib.redirect_stderr(diagnostics):
                exit_code = validator.main()
        finally:
            sys.argv = prior_argv
        parsed_report = json.loads(report.getvalue())
        if exit_code != 1 or parsed_report.get("code_only") != ["CORELINK_FRESH_SESSION"]:
            raise AssertionError(
                "matrix-row deletion did not fail closed through the normal gate: "
                f"exit={exit_code}, code_only={parsed_report.get('code_only')!r}, "
                f"stderr={diagnostics.getvalue()!r}"
            )

    print("verify_b245_secrets_matrix: PASS (exact scope + 6 mutations rejected; no-op unchanged)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
