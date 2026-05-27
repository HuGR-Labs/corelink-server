#!/usr/bin/env python3
"""
Enforce INV-AUTH-MIGRATION-ADDITIVE (HIGH) on every SQL migration in
`migrations/` and `migrations/d1/`.

Canonical sources:
- specs/04_sprints/_sealed/S03/work_items/WI-S03-005-neon-schema-auth-tables.md §6.1.6
- specs/03_architecture/invariant_registry.md INV-AUTH-MIGRATION-ADDITIVE
- specs/03_architecture/adrs/ADR-0031-neon-schema-pgcrypto.md §5.6

The script is **regex-based** intentionally. A real Postgres parser
would buy us better fidelity but at the cost of needing libpq + Python
bindings in CI. The patterns below are conservative — false positives
fail the CI gate; the operator must add an `-- ADR-XXXX additive-only:
allowed because …` annotation on the offending line to suppress.

Banned patterns (case-insensitive; anchored on a SQL token boundary):

- `DROP TABLE` (any form, including `DROP TABLE IF EXISTS`)
- `DROP COLUMN`
- `DROP INDEX`
- `DROP TYPE`
- `DROP CONSTRAINT`
- `DROP TRIGGER`
- `DROP FUNCTION`
- `DROP EXTENSION`
- `DROP POLICY`
- `ALTER COLUMN … TYPE` (column type narrowing / re-typing is destructive)
- `ALTER COLUMN … DROP NOT NULL` (re-allowing NULL on a previously-NN
  column changes the data model semantics; treated as destructive)
- `TRUNCATE` (irreversible bulk delete)
- `RENAME COLUMN` / `RENAME TABLE` / `RENAME TO` (forces dual-write
  windows; defer to additive copy + read shim)

Allowed exception comments (line-local):
    `-- additive-allowed: <ADR-NNNN reason>`

Exit codes:
    0 — every migration file passes the additive check.
    1 — at least one banned token was detected without an
        `additive-allowed` annotation; offending lines are printed.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
MIGRATION_DIRS = [
    REPO_ROOT / "migrations",
    REPO_ROOT / "migrations" / "d1",
    # Wave-18 — Neon analytics shadow migrations land in `migrations/neon/`.
    # The shadow tables are additive-only (INV-AUDIT-APPEND-ONLY); the
    # gate is the same additive-only discipline as `migrations/d1/`.
    REPO_ROOT / "migrations" / "neon",
]

BANNED_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    ("DROP TABLE", re.compile(r"\bDROP\s+TABLE\b", re.IGNORECASE)),
    ("DROP COLUMN", re.compile(r"\bDROP\s+COLUMN\b", re.IGNORECASE)),
    ("DROP INDEX", re.compile(r"\bDROP\s+INDEX\b", re.IGNORECASE)),
    ("DROP TYPE", re.compile(r"\bDROP\s+TYPE\b", re.IGNORECASE)),
    ("DROP CONSTRAINT", re.compile(r"\bDROP\s+CONSTRAINT\b", re.IGNORECASE)),
    ("DROP TRIGGER", re.compile(r"\bDROP\s+TRIGGER\b", re.IGNORECASE)),
    ("DROP FUNCTION", re.compile(r"\bDROP\s+FUNCTION\b", re.IGNORECASE)),
    ("DROP EXTENSION", re.compile(r"\bDROP\s+EXTENSION\b", re.IGNORECASE)),
    ("DROP POLICY", re.compile(r"\bDROP\s+POLICY\b", re.IGNORECASE)),
    (
        "ALTER COLUMN ... TYPE",
        re.compile(r"\bALTER\s+COLUMN\s+\w+\s+TYPE\b", re.IGNORECASE),
    ),
    (
        "ALTER COLUMN ... DROP NOT NULL",
        re.compile(r"\bALTER\s+COLUMN\s+\w+\s+DROP\s+NOT\s+NULL\b", re.IGNORECASE),
    ),
    ("TRUNCATE", re.compile(r"\bTRUNCATE\b", re.IGNORECASE)),
    ("RENAME COLUMN", re.compile(r"\bRENAME\s+COLUMN\b", re.IGNORECASE)),
    ("RENAME TABLE", re.compile(r"\bRENAME\s+TABLE\b", re.IGNORECASE)),
    ("RENAME TO", re.compile(r"\bRENAME\s+TO\b", re.IGNORECASE)),
]

# An `additive-allowed: ADR-NNNN` annotation on the same line suppresses
# the violation. The reason text is mandatory so reviewers can see why.
ALLOW_PATTERN = re.compile(r"--\s*additive-allowed\s*:\s*ADR-\d{4}\b", re.IGNORECASE)

# Strip line comments before scanning so `-- DROP TABLE old_thing` in a
# rationale block does not trigger.
COMMENT_PATTERN = re.compile(r"--[^\n]*$", re.MULTILINE)


def iter_migration_files() -> list[Path]:
    files: list[Path] = []
    for d in MIGRATION_DIRS:
        if not d.exists():
            continue
        files.extend(sorted(p for p in d.iterdir() if p.suffix == ".sql"))
    return files


def scan_file(path: Path) -> list[tuple[int, str, str]]:
    """Return list of (line_number, banned_pattern_label, raw_line) for
    every banned pattern that fires without an allow annotation."""
    violations: list[tuple[int, str, str]] = []
    raw = path.read_text(encoding="utf-8")
    for line_no, raw_line in enumerate(raw.splitlines(), start=1):
        # If the line already carries an additive-allowed annotation, skip.
        if ALLOW_PATTERN.search(raw_line):
            continue
        # Strip the `-- …` portion so prose comments cannot trip the scan.
        scan_line = COMMENT_PATTERN.sub("", raw_line)
        for label, pattern in BANNED_PATTERNS:
            if pattern.search(scan_line):
                violations.append((line_no, label, raw_line.rstrip()))
    return violations


def main() -> int:
    files = iter_migration_files()
    if not files:
        print("warn: no migration files found under migrations/ or migrations/d1/")
        return 0

    total_violations = 0
    for path in files:
        violations = scan_file(path)
        if not violations:
            continue
        rel = path.relative_to(REPO_ROOT)
        print(f"\n{rel}: {len(violations)} destructive token(s) detected")
        for line_no, label, raw_line in violations:
            total_violations += 1
            print(f"  L{line_no:>4}  [{label}]  {raw_line}")
        print(
            "  → INV-AUTH-MIGRATION-ADDITIVE (HIGH): destructive migrations are "
            "rejected. Either rewrite as an additive copy + read shim, or "
            "annotate the line with "
            "`-- additive-allowed: ADR-NNNN <reason>` after recording an ADR "
            "documenting the dual-write window."
        )

    if total_violations:
        print(
            f"\nFAIL: {total_violations} destructive token(s) across {len(files)} files."
        )
        return 1
    print(f"OK: {len(files)} migration file(s) scanned; all additive.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
