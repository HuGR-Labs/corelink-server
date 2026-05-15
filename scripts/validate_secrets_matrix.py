#!/usr/bin/env python3
"""
validate_secrets_matrix.py — Secrets-matrix drift validator (Python).

Cross-references `docs/internal/secrets-checklist.md` (the canonical 89-row
production-secrets matrix) against the codebase. Emits a structured JSON
report (matrix-only / code-only / in-both) and fails on real drift.

Complements `scripts/secrets-checklist-verify.sh` (deploy-gate, bash):
  * The bash verifier is wired into `cf-deploy-prod.yml` (fail-closed on every
    production deploy).
  * This Python validator is wired into a daily cron + PR gate
    (`.github/workflows/secrets-drift.yml`) and produces a machine-readable
    JSON artifact suitable for dashboards, runbook triage, and SOC 2 CC6.1
    evidence sampling.

Exit codes:
  0 — no `code-only` drift (matrix is sound; `matrix-only` rows soft-warn).
  1 — at least one `code-only` env var found (real drift; PR must add a row).
  2 — invocation error (matrix file missing, unparsable, etc.).

Usage:
  python3 scripts/validate_secrets_matrix.py
  python3 scripts/validate_secrets_matrix.py --json-out report.json
  python3 scripts/validate_secrets_matrix.py --dry-run        # never fail
  python3 scripts/validate_secrets_matrix.py --quiet          # JSON-only

Outputs JSON shape::

  {
    "schema_version": "1",
    "matrix_file": "docs/internal/secrets-checklist.md",
    "summary": {
      "matrix_total": <int>,
      "code_total":   <int>,
      "in_both":      <int>,
      "matrix_only":  <int>,
      "code_only":    <int>,
      "drift":        <int>          // == code_only after allowlist
    },
    "matrix_only": [...],            // dead/forward-looking rows (warn)
    "code_only":   [...],            // real drift (fail)
    "in_both":     [...],            // clean intersection
    "allowlist_skipped": [...]       // env vars filtered as not-a-secret
  }

Cross-references:
  * docs/internal/secrets-checklist.md (canonical matrix)
  * ROADMAP-TO-GA.md §9 (Human Track — credentials)
  * specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md (CC6.1)
  * specs/_runbooks/RB-SECRETS-DRIFT.md (triage)
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parent.parent
MATRIX_FILE_REL = "docs/internal/secrets-checklist.md"

# ---------------------------------------------------------------------------
# Allowlist — env vars intentionally not in the matrix.
# Kept in sync with scripts/secrets-checklist-verify.sh (single source of
# truth is the bash regex; we mirror it here for the Python pipeline).
# ---------------------------------------------------------------------------
ALLOWLIST_REGEX = re.compile(
    r"^("
    r"PROPTEST_"
    r"|HOME$|USERPROFILE$"
    r"|CARGO_"
    r"|GITHUB_(TOKEN|OUTPUT|ENV|PATH|STEP_SUMMARY|ACTIONS|REPOSITORY|SHA|REF"
    r"|WORKFLOW|RUN_ID|RUN_NUMBER|ACTOR|EVENT_NAME|EVENT_PATH|JOB|API_URL"
    r"|SERVER_URL|GRAPHQL_URL|WORKSPACE)$"
    r"|RUNNER_"
    r"|GH_TOKEN$"
    r"|GNUPGHOME$"
    r"|SOURCE_DATE_EPOCH$"
    r"|PATH$|PWD$|USER$|SHELL$|TERM$|CI$|TZ$|LANG$|LC_"
    r"|NODE_ENV$|ENVIRONMENT$"
    r"|RUST_"
    r"|TODO_"
    r"|DOCS_BASE_URL$"
    r"|LH_BASE_URL$|LH_START_COMMAND$"
    r"|E2E_BASE_URL$|NEXT_PUBLIC_E2E_TEST_MODE$"
    r"|PROJECTS$|SKIP_WEBSERVER$"
    r"|GCP_TEST_KEY_RESOURCE$|GCP_TEST_REGION$"
    r"|DT_API_KEY_TEST_|DT_MOCK_INJECTION_ENABLED$"
    r")"
)

EXCLUDE_DIR_PARTS = {
    "target",
    "node_modules",
    ".next",
    "dist",
    "build",
    ".git",
    "_archive",
    ".turbo",
    ".pnpm-store",
}

# Matrix row regex: `| 1 | <secret> | `ENV_VAR` | ...`
MATRIX_ROW_RE = re.compile(r"^\|\s*\d+\s*\|")
MATRIX_ENV_VAR_RE = re.compile(r"`([A-Z][A-Z0-9_]*)`")

# Rust env::var("X") / std::env::var("X") / env::var_os("X") / env::set_var("X"
RUST_ENV_RE = re.compile(
    r"(?:std::)?env::(?:var|var_os|set_var)\(\s*\"([A-Z][A-Z0-9_]*)\""
)

# TS/JS process.env.X and process.env["X"]
TS_ENV_DOT_RE = re.compile(r"process\.env\.([A-Z][A-Z0-9_]*)")
TS_ENV_BRACKET_RE = re.compile(r"process\.env\[\s*\"([A-Z][A-Z0-9_]*)\"\s*\]")

# GHA `${{ secrets.X }}` and `${{ env.X }}` (env. only if uppercase-style)
GHA_SECRETS_RE = re.compile(r"\$\{\{\s*secrets\.([A-Z][A-Z0-9_]*)\s*\}\}")

# Wrangler bindings under [vars] or [env.X.vars] — `KEY = "value"` shape.
WRANGLER_VAR_RE = re.compile(r"^([A-Z][A-Z0-9_]*)\s*=")


# ---------------------------------------------------------------------------
# Matrix parsing
# ---------------------------------------------------------------------------
def parse_matrix(path: Path) -> set[str]:
    """Extract env-var names from the matrix's 4th column (backtick-quoted)."""
    if not path.is_file():
        print(f"ERROR: matrix file not found: {path}", file=sys.stderr)
        sys.exit(2)

    names: set[str] = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        if not MATRIX_ROW_RE.match(line):
            continue
        cols = line.split("|")
        # cols indices: 0 (leading empty), 1 (#), 2 (name), 3 (env var), 4..
        if len(cols) < 4:
            continue
        env_col = cols[3]
        for m in MATRIX_ENV_VAR_RE.finditer(env_col):
            names.add(m.group(1))
    return names


# ---------------------------------------------------------------------------
# Code scanning
# ---------------------------------------------------------------------------
def _iter_files(root: Path, suffixes: Iterable[str]) -> Iterable[Path]:
    suffixes = tuple(suffixes)
    for dirpath, dirnames, filenames in os.walk(root):
        # prune excluded dirs in-place
        dirnames[:] = [d for d in dirnames if d not in EXCLUDE_DIR_PARTS]
        for fn in filenames:
            if fn.endswith(suffixes):
                yield Path(dirpath) / fn


def scan_rust(root: Path) -> set[str]:
    hits: set[str] = set()
    for f in _iter_files(root, (".rs",)):
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for m in RUST_ENV_RE.finditer(txt):
            hits.add(m.group(1))
    return hits


def scan_ts(root: Path) -> set[str]:
    hits: set[str] = set()
    for f in _iter_files(root, (".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs")):
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for m in TS_ENV_DOT_RE.finditer(txt):
            hits.add(m.group(1))
        for m in TS_ENV_BRACKET_RE.finditer(txt):
            hits.add(m.group(1))
    return hits


def scan_workflows(root: Path) -> set[str]:
    hits: set[str] = set()
    wf_dir = root / ".github" / "workflows"
    if not wf_dir.is_dir():
        return hits
    for f in wf_dir.rglob("*.yml"):
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for m in GHA_SECRETS_RE.finditer(txt):
            hits.add(m.group(1))
    for f in wf_dir.rglob("*.yaml"):
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for m in GHA_SECRETS_RE.finditer(txt):
            hits.add(m.group(1))
    return hits


def scan_wrangler(root: Path) -> set[str]:
    """Extract var/binding names from wrangler.toml files (uppercase tokens)."""
    hits: set[str] = set()
    in_vars_section = False
    for f in root.rglob("wrangler.toml"):
        # Skip excluded paths
        if any(part in EXCLUDE_DIR_PARTS for part in f.parts):
            continue
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        in_vars_section = False
        for raw in txt.splitlines():
            line = raw.strip()
            if line.startswith("[") and line.endswith("]"):
                # Heuristic: only collect under [vars], [env.*.vars], or
                # binding name = "..." patterns. Skip everything else
                # to avoid collecting bucket names / class names.
                in_vars_section = (
                    line == "[vars]"
                    or line.endswith(".vars]")
                )
                continue
            if not in_vars_section or not line or line.startswith("#"):
                continue
            m = WRANGLER_VAR_RE.match(line)
            if m:
                hits.add(m.group(1))
    return hits


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[1])
    ap.add_argument(
        "--json-out",
        type=Path,
        default=None,
        help="Write the JSON report to this path (in addition to stdout summary).",
    )
    ap.add_argument(
        "--dry-run",
        action="store_true",
        help="Compute and print the report but always exit 0.",
    )
    ap.add_argument(
        "--quiet",
        action="store_true",
        help="Suppress human summary; print only JSON to stdout.",
    )
    ap.add_argument(
        "--repo-root",
        type=Path,
        default=REPO_ROOT,
        help="Repository root (default: parent of scripts/).",
    )
    args = ap.parse_args()

    root: Path = args.repo_root.resolve()
    matrix_path = root / MATRIX_FILE_REL

    matrix = parse_matrix(matrix_path)
    code = scan_rust(root) | scan_ts(root) | scan_workflows(root) | scan_wrangler(root)

    # Partition with allowlist applied to `code` side.
    allowlist_skipped = sorted(v for v in code if ALLOWLIST_REGEX.match(v))
    code_filtered = {v for v in code if not ALLOWLIST_REGEX.match(v)}

    in_both = sorted(matrix & code_filtered)
    matrix_only = sorted(matrix - code_filtered)
    code_only = sorted(code_filtered - matrix)

    report = {
        "schema_version": "1",
        "matrix_file": MATRIX_FILE_REL,
        "summary": {
            "matrix_total": len(matrix),
            "code_total": len(code_filtered),
            "in_both": len(in_both),
            "matrix_only": len(matrix_only),
            "code_only": len(code_only),
            "drift": len(code_only),
        },
        "matrix_only": matrix_only,
        "code_only": code_only,
        "in_both": in_both,
        "allowlist_skipped": allowlist_skipped,
    }

    if args.json_out:
        args.json_out.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

    if args.quiet:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        s = report["summary"]
        print(
            "validate_secrets_matrix: "
            f"matrix={s['matrix_total']} code={s['code_total']} "
            f"in_both={s['in_both']} matrix_only={s['matrix_only']} "
            f"code_only={s['code_only']}"
        )
        if matrix_only:
            print("WARN matrix_only (forward-looking or stale rows; soft-warn):")
            for v in matrix_only:
                print(f"  - {v}")
        if code_only:
            print("ERROR code_only (real drift; add to matrix or allowlist):")
            for v in code_only:
                print(f"  - {v}")
        if not code_only and not matrix_only:
            print("validate_secrets_matrix: OK (no drift).")

    if args.dry_run:
        return 0
    return 1 if code_only else 0


if __name__ == "__main__":
    sys.exit(main())
