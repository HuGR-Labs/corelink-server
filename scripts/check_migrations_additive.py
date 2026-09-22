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
ALLOW_PATTERN = re.compile(
    r"^--\s*additive-allowed\s*:\s*ADR-\d{4}\b\s+\S", re.IGNORECASE
)

# This is the sole trigger-replacement exception. It is intentionally bound to
# one forward migration and the two B-071 accounting trigger identifiers.
TRIGGER_REPLACEMENT_FILE = "migrations/d1/0143_gc_accounting_region_upgrade.sql"
TRIGGER_REPLACEMENTS = {
    "DROP TRIGGER IF EXISTS trg_gc_purge_accounting_required;":
        "-- additive-allowed: ADR-0103 replace the deployed B-071 accounting guard",
    "DROP TRIGGER IF EXISTS trg_gc_purge_finalize_accounting;":
        "-- additive-allowed: ADR-0103 replace the deployed B-071 accounting finalizer",
}

def iter_migration_files() -> list[Path]:
    files: list[Path] = []
    for d in MIGRATION_DIRS:
        if not d.exists():
            continue
        files.extend(sorted(p for p in d.iterdir() if p.suffix == ".sql"))
    return files


def lex_sql_line(
    line: str, in_block_comment: bool, quote: str | None
) -> tuple[str, int | None, bool, str | None]:
    """Return executable SQL plus a real line-comment offset and lexer state."""
    code: list[str] = []
    index = 0
    while index < len(line):
        if in_block_comment:
            if line[index : index + 2] == "*/":
                in_block_comment = False
                index += 2
            else:
                index += 1
            code.append(" ")
            continue
        if quote is not None:
            if line[index] == quote:
                if index + 1 < len(line) and line[index + 1] == quote:
                    code.extend((" ", " "))
                    index += 2
                    continue
                quote = None
            code.append(" ")
            index += 1
            continue
        if line[index : index + 2] == "/*":
            in_block_comment = True
            code.extend((" ", " "))
            index += 2
        elif line[index : index + 2] == "--":
            return "".join(code), index, in_block_comment, quote
        elif line[index] in {"'", '"'}:
            quote = line[index]
            code.append(" ")
            index += 1
        else:
            code.append(line[index])
            index += 1
    return "".join(code), None, in_block_comment, quote


def valid_line_waiver(raw_line: str, sql_code: str, comment_start: int) -> bool:
    """Accept one terminated SQL statement plus an audited, reasoned waiver."""
    comment = raw_line[comment_start:]
    return (
        sql_code.count(";") == 1
        and sql_code.rstrip().endswith(";")
        and ALLOW_PATTERN.search(comment) is not None
    )


def valid_trigger_replacement(
    raw_line: str, sql_code: str, comment_start: int, migration_path: str | None
) -> bool:
    if migration_path != TRIGGER_REPLACEMENT_FILE:
        return False
    statement = sql_code.strip().upper()
    if statement not in {item.upper() for item in TRIGGER_REPLACEMENTS}:
        return False
    expected_statement = next(
        item for item in TRIGGER_REPLACEMENTS if item.upper() == statement
    )
    expected_comment = TRIGGER_REPLACEMENTS[expected_statement]
    comment = raw_line[comment_start:].strip()
    return comment.lower() == expected_comment.lower()


def scan_sql(raw: str, migration_path: str | None = None) -> list[tuple[int, str, str]]:
    """Return destructive SQL tokens not covered by a valid audited waiver."""
    violations: list[tuple[int, str, str]] = []
    in_block_comment = False
    quote: str | None = None
    executable_lines: list[str] = []
    raw_lines = raw.splitlines()
    for line_no, raw_line in enumerate(raw_lines, start=1):
        scan_line, comment_start, in_block_comment, quote = lex_sql_line(
            raw_line, in_block_comment, quote
        )
        # Waivers are accepted only in a real SQL line comment, after exactly
        # one terminated statement. Strings, block comments, adjacent SQL, and
        # missing reasons cannot suppress the gate.
        is_target_migration = migration_path == TRIGGER_REPLACEMENT_FILE
        if comment_start is not None and (
            valid_trigger_replacement(raw_line, scan_line, comment_start, migration_path)
            or (
                not is_target_migration
                and "ADR-0103" not in raw_line.upper()
                and valid_line_waiver(raw_line, scan_line, comment_start)
            )
        ):
            executable_lines.append(" " * len(scan_line))
            continue
        executable_lines.append(scan_line)
        for label, pattern in BANNED_PATTERNS:
            if pattern.search(scan_line):
                violations.append((line_no, label, raw_line.rstrip()))

    # The line-oriented lexer above reports normal violations with precise
    # locations. Scan the joined executable SQL as well so newline-separated
    # destructive statements cannot evade the name-scoped exception.
    joined = "\n".join(executable_lines)
    already_reported = {(line, label) for line, label, _ in violations}
    for label, pattern in BANNED_PATTERNS:
        for match in pattern.finditer(joined):
            line_no = joined.count("\n", 0, match.start()) + 1
            if (line_no, label) not in already_reported:
                violations.append(
                    (line_no, label, raw_lines[line_no - 1].rstrip())
                )
                already_reported.add((line_no, label))

    if migration_path == TRIGGER_REPLACEMENT_FILE:
        observed = {
            raw_line.split("--", 1)[0].strip().upper()
            for raw_line in raw_lines
            if "DROP TRIGGER" in raw_line.upper()
        }
        expected = {statement.upper() for statement in TRIGGER_REPLACEMENTS}
        if observed != expected:
            violations.append(
                (
                    1,
                    "B-071 trigger replacement set",
                    "expected exactly the two named accounting triggers",
                )
            )
    return violations


def scan_file(path: Path) -> list[tuple[int, str, str]]:
    """Scan a migration file for non-additive statements."""
    return scan_sql(
        path.read_text(encoding="utf-8"), path.relative_to(REPO_ROOT).as_posix()
    )


def self_test() -> int:
    """Exercise waiver parsing against quoted, block, and adjacent-SQL bypasses."""
    valid = "DROP TABLE tenant; -- additive-allowed: ADR-0064 widening rebuild"
    attacks = (
        "SELECT '-- additive-allowed: ADR-0064 approved'; DROP TABLE tenant;",
        "/* -- additive-allowed: ADR-0064 approved */ DROP TABLE tenant;",
        "SELECT 1; DROP TABLE tenant; -- additive-allowed: ADR-0064 approved",
        "DROP TABLE tenant; -- additive-allowed: ADR-0064",
        "DROP TABLE tenant;\n-- additive-allowed: ADR-0064 wrong line",
    )
    if scan_sql(valid):
        print("FAIL: valid audited waiver was rejected")
        return 1
    for attack in attacks:
        if not scan_sql(attack):
            print(f"FAIL: waiver bypass was accepted: {attack}")
            return 1
    path = TRIGGER_REPLACEMENT_FILE
    valid_triggers = "\n".join(
        f"{statement} {comment}" for statement, comment in TRIGGER_REPLACEMENTS.items()
    )
    if scan_sql(valid_triggers, path):
        print("FAIL: the exact B-071 trigger replacement was rejected")
        return 1
    mutations = (
        (
            valid_triggers
            + "\nDROP TABLE tenant; -- additive-allowed: ADR-0103 narrowly scoped",
            path,
        ),
        (
            valid_triggers
            + "\nDROP TRIGGER IF EXISTS trg_other; -- additive-allowed: ADR-0103 replacement",
            path,
        ),
        (valid_triggers.replace("trg_gc_purge_accounting_required", "trg_other"), path),
        (valid_triggers, "migrations/d1/0144_unrelated.sql"),
        ("DROP\nTRIGGER IF EXISTS trg_other; -- additive-allowed: ADR-0103 replacement", path),
    )
    for mutation, migration_path in mutations:
        if not scan_sql(mutation, migration_path):
            print("FAIL: out-of-scope trigger replacement was accepted")
            return 1
    print("OK: migration waiver lexer self-test passed")
    return 0


# A migration filename must start with a zero-padded ordinal that is UNIQUE
# within its directory. Two files sharing an ordinal is not cosmetic: the
# apply order between them degrades to a lexicographic tiebreak on the rest
# of the name, and the D1 ledger — which keys on the full filename — records
# both as applied, so the duplicate never surfaces as an error. It shows up
# later as "0094 did what?" ambiguity in every incident and every replay.
# Two sessions branching off the same `main` will pick the same next number
# independently; only a gate catches that at merge time.
ORDINAL_PATTERN = re.compile(r"^(\d{4})_")

# Ordinals that were ALREADY duplicated when this check landed, and that are
# already applied in production. Renaming an applied migration desyncs the D1
# ledger — it keys on the filename, so the renamed file reads as never-applied
# and gets replayed. These are grandfathered deliberately; the cost of the
# rename is strictly worse than the ambiguity. Never add to this set to make a
# NEW collision pass: renumber the new file instead.
GRANDFATHERED_ORDINALS: dict[str, frozenset[str]] = {
    # Both landed 2026-05-14 from two branches that each picked "next = 0044".
    "migrations/d1": frozenset(
        {"0044_drata_evidence_sent.sql", "0044_stripe_webhook_events_processed.sql"}
    ),
}


def check_ordinals() -> int:
    """Report migration ordinals reused within a single directory."""
    collisions = 0
    for d in MIGRATION_DIRS:
        if not d.exists():
            continue
        by_ordinal: dict[str, list[str]] = {}
        for path in sorted(p for p in d.iterdir() if p.suffix == ".sql"):
            m = ORDINAL_PATTERN.match(path.name)
            if m is None:
                continue
            by_ordinal.setdefault(m.group(1), []).append(path.name)
        rel = d.relative_to(REPO_ROOT)
        grandfathered = GRANDFATHERED_ORDINALS.get(rel.as_posix(), frozenset())
        for ordinal, names in sorted(by_ordinal.items()):
            if len(names) < 2:
                continue
            if all(name in grandfathered for name in names):
                continue
            collisions += 1
            print(f"\n{rel}: ordinal {ordinal} is used by {len(names)} files")
            for name in names:
                print(f"  {name}")
            print(
                "  \u2192 migration ordinals must be unique per directory. Renumber all "
                "but one to the next free ordinal; the apply order between "
                "same-ordinal files is an accident of lexicographic sort."
            )
    return collisions


def main() -> int:
    if self_test() != 0:
        return 1
    files = iter_migration_files()
    if not files:
        print("warn: no migration files found under migrations/ or migrations/d1/")
        return 0

    ordinal_collisions = check_ordinals()

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

    if total_violations or ordinal_collisions:
        parts = []
        if total_violations:
            parts.append(f"{total_violations} destructive token(s)")
        if ordinal_collisions:
            parts.append(f"{ordinal_collisions} duplicated ordinal(s)")
        print(f"\nFAIL: {' and '.join(parts)} across {len(files)} files.")
        return 1
    print(
        f"OK: {len(files)} migration file(s) scanned; all additive, "
        "all ordinals unique."
    )
    return 0


if __name__ == "__main__":
    sys.exit(self_test() if sys.argv[1:] == ["--self-test"] else main())
