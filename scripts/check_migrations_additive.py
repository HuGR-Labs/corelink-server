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

The historic rebuild exceptions below are fixed by path, SQL statement, and
ADR.  New destructive SQL cannot be approved by copying an annotation.  The
only forward trigger replacement is likewise fixed to the two B-071 trigger
names in migration 0143 and ADR-0143.

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

# Existing table-rebuild decisions carry an ADR annotation on the same line.
# The annotation alone is deliberately insufficient: each legacy statement is
# also pinned below so a new migration cannot create a generic bypass.
ALLOW_PATTERN = re.compile(
    r"^--\s*additive-allowed\s*:\s*ADR-\d{4}\b\s+\S", re.IGNORECASE
)

LEGACY_WAIVERS: dict[str, tuple[str, frozenset[str]]] = {
    "migrations/d1/0064_tenant_tier_max.sql": (
        "ADR-0064",
        frozenset(
            {
                "DROP TABLE tenant;",
                "ALTER TABLE tenant_new RENAME TO tenant;",
            }
        ),
    ),
    "migrations/d1/0098_widen_erasure_region_check_apac.sql": (
        "ADR-0098",
        frozenset(
            {
                "DROP TABLE erasure_attestations;",
                "ALTER TABLE erasure_attestations_new RENAME TO erasure_attestations;",
                "DROP TABLE erasure_public_keys;",
                "ALTER TABLE erasure_public_keys_new RENAME TO erasure_public_keys;",
            }
        ),
    ),
    "migrations/d1/0126_b083_common_purge_r2_absent_quarantine.sql": (
        "ADR-0031",
        frozenset(
            {"DROP TRIGGER IF EXISTS trg_byok_object_purge_forward_only;"},
        ),
    ),
    "migrations/d1/0141_terraform_drift_region_contract.sql": (
        "ADR-0102",
        frozenset(
            {
                "DROP TABLE terraform_drift_findings;",
                "ALTER TABLE terraform_drift_findings_new RENAME TO terraform_drift_findings;",
            }
        ),
    ),
}

B071_TRIGGER_REPLACEMENT_PATH = "migrations/d1/0143_gc_accounting_region_upgrade.sql"
B071_TRIGGER_REPLACEMENT_NAMES = (
    "trg_gc_purge_accounting_required",
    "trg_gc_purge_finalize_accounting",
)
B071_TRIGGER_REPLACEMENT_COMMENT = (
    "-- additive-trigger-replacement: ADR-0143 B-071 historic accounting body"
)

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


def normalized_sql(sql_code: str) -> str:
    """Make whitespace irrelevant while retaining the complete statement."""
    return " ".join(sql_code.split())


def valid_legacy_waiver(
    raw_line: str, sql_code: str, comment_start: int, relative_path: str | None
) -> bool:
    """Accept only a pre-recorded rebuild statement at its original location."""
    if relative_path is None or relative_path not in LEGACY_WAIVERS:
        return False
    comment = raw_line[comment_start:]
    adr, statements = LEGACY_WAIVERS[relative_path]
    return (
        sql_code.count(";") == 1
        and sql_code.rstrip().endswith(";")
        and normalized_sql(sql_code) in statements
        and ALLOW_PATTERN.search(comment) is not None
        and re.search(rf"\b{re.escape(adr)}\b", comment, re.IGNORECASE) is not None
    )


def b071_trigger_replacement_policy(
    raw: str, relative_path: str | None
) -> tuple[frozenset[int], list[tuple[int, str]]]:
    """Authorize only the two ordered, adjacent ADR-0143 replacement pairs.

    This intentionally parses the *statement sequence*, rather than searching
    the whole migration for a matching DROP.  A matching DROP is consumed with
    the immediately following executable CREATE of the same trigger.  Each
    expected name may be consumed exactly once and in ADR order.  Therefore a
    duplicate, an unpaired CREATE, a reordering, or a renamed trigger leaves
    every DROP unauthorized and fails closed in the additive scanner.
    """
    if relative_path != B071_TRIGGER_REPLACEMENT_PATH:
        return frozenset(), []

    records: list[tuple[int, str, str, int | None]] = []
    in_block_comment = False
    quote: str | None = None
    for line_no, raw_line in enumerate(raw.splitlines(), start=1):
        code, comment_start, in_block_comment, quote = lex_sql_line(
            raw_line, in_block_comment, quote
        )
        records.append((line_no, raw_line, code, comment_start))

    def next_executable(index: int) -> tuple[int, str, str, int | None] | None:
        for record in records[index + 1 :]:
            if record[2].strip():
                return record
        return None

    authorized_drop_lines: set[int] = set()
    authorized_create_lines: set[int] = set()
    consumed_names: list[str] = []
    policy_errors: list[tuple[int, str]] = []

    for index, (line_no, raw_line, code, comment_start) in enumerate(records):
        normalized = normalized_sql(code)
        matched_name = next(
            (
                name
                for name in B071_TRIGGER_REPLACEMENT_NAMES
                if normalized == f"DROP TRIGGER IF EXISTS {name};"
            ),
            None,
        )
        if matched_name is None:
            continue
        if (
            comment_start is None
            or raw_line[comment_start:].strip() != B071_TRIGGER_REPLACEMENT_COMMENT
        ):
            policy_errors.append((line_no, "ADR-0143 trigger replacement is not pinned"))
            continue
        following = next_executable(index)
        if following is None:
            policy_errors.append((line_no, "ADR-0143 DROP has no adjacent CREATE"))
            continue
        create_line, _, create_code, _ = following
        # The name comparison remains byte-exact.  SQL keyword whitespace and
        # keyword case are immaterial; the controlled object identity is not.
        create_match = re.match(
            r"^\s*CREATE\s+TRIGGER\s+([^\s(]+)\b", create_code, re.IGNORECASE
        )
        if create_match is None or create_match.group(1) != matched_name:
            policy_errors.append(
                (line_no, "ADR-0143 DROP is not followed by its matching CREATE")
            )
            continue
        if matched_name in consumed_names:
            policy_errors.append((line_no, "ADR-0143 trigger replacement is duplicated"))
            continue
        authorized_drop_lines.add(line_no)
        authorized_create_lines.add(create_line)
        consumed_names.append(matched_name)

    if tuple(consumed_names) != B071_TRIGGER_REPLACEMENT_NAMES:
        policy_errors.append(
            (1, "ADR-0143 must contain each approved trigger pair exactly once and in order")
        )

    # A bare CREATE for either approved name is valid only when the same
    # statement was consumed by its immediately preceding approved DROP.
    for line_no, _, code, _ in records:
        create_match = re.match(
            r"^\s*CREATE\s+TRIGGER\s+([^\s(]+)\b", code, re.IGNORECASE
        )
        if (
            create_match is not None
            and create_match.group(1) in B071_TRIGGER_REPLACEMENT_NAMES
            and line_no not in authorized_create_lines
        ):
            policy_errors.append(
                (line_no, "ADR-0143 approved trigger CREATE is unpaired or duplicated")
            )

    # Do not authorize a subset after any structural-policy failure.  This is
    # what makes the policy fail closed when an otherwise valid pair appears
    # beside a duplicate or reordered pair.
    if policy_errors:
        return frozenset(), policy_errors
    return frozenset(authorized_drop_lines), []


def scan_sql(raw: str, relative_path: str | None = None) -> list[tuple[int, str, str]]:
    """Return destructive SQL tokens not covered by a valid audited waiver."""
    violations: list[tuple[int, str, str]] = []
    authorized_b071_drops, b071_policy_errors = b071_trigger_replacement_policy(
        raw, relative_path
    )
    for line_no, message in b071_policy_errors:
        violations.append((line_no, "B-071 trigger replacement policy", message))
    in_block_comment = False
    quote: str | None = None
    for line_no, raw_line in enumerate(raw.splitlines(), start=1):
        scan_line, comment_start, in_block_comment, quote = lex_sql_line(
            raw_line, in_block_comment, quote
        )
        for label, pattern in BANNED_PATTERNS:
            if not pattern.search(scan_line):
                continue
            # Waivers are accepted only in a real SQL line comment, after one
            # complete statement and only for a statement already approved by
            # an ADR. Strings, block comments, adjacent SQL, wrong paths, and
            # a different destructive operation cannot suppress the gate.
            if comment_start is not None and (
                valid_legacy_waiver(
                    raw_line, scan_line, comment_start, relative_path
                )
                or (label == "DROP TRIGGER" and line_no in authorized_b071_drops)
            ):
                continue
            violations.append((line_no, label, raw_line.rstrip()))
    return violations


def scan_file(path: Path) -> list[tuple[int, str, str]]:
    """Scan a migration file for non-additive statements."""
    return scan_sql(
        path.read_text(encoding="utf-8"), path.relative_to(REPO_ROOT).as_posix()
    )


def self_test() -> int:
    """Exercise fixed waiver identities and trigger-replacement mutations."""
    valid = "DROP TABLE tenant; -- additive-allowed: ADR-0064 widening rebuild"
    attacks = (
        "SELECT '-- additive-allowed: ADR-0064 approved'; DROP TABLE tenant;",
        "/* -- additive-allowed: ADR-0064 approved */ DROP TABLE tenant;",
        "SELECT 1; DROP TABLE tenant; -- additive-allowed: ADR-0064 approved",
        "DROP TABLE tenant; -- additive-allowed: ADR-0064",
        "DROP TABLE tenant;\n-- additive-allowed: ADR-0064 wrong line",
    )
    if scan_sql(valid, "migrations/d1/0064_tenant_tier_max.sql"):
        print("FAIL: valid audited waiver was rejected")
        return 1
    for attack in attacks:
        if not scan_sql(attack, "migrations/d1/0064_tenant_tier_max.sql"):
            print(f"FAIL: waiver bypass was accepted: {attack}")
            return 1
    b071_valid = f"""\
DROP TRIGGER IF EXISTS trg_gc_purge_accounting_required; {B071_TRIGGER_REPLACEMENT_COMMENT}
CREATE TRIGGER trg_gc_purge_accounting_required BEFORE DELETE ON blob_meta
BEGIN SELECT 1; END;
DROP TRIGGER IF EXISTS trg_gc_purge_finalize_accounting; {B071_TRIGGER_REPLACEMENT_COMMENT}
CREATE TRIGGER trg_gc_purge_finalize_accounting AFTER DELETE ON blob_meta
BEGIN SELECT 1; END;
"""
    if scan_sql(b071_valid, B071_TRIGGER_REPLACEMENT_PATH):
        print("FAIL: exact ordered B-071 trigger replacements were rejected")
        return 1
    b071_mutations = (
        (
            b071_valid.replace("accounting_required", "legal_hold_guard", 1),
            B071_TRIGGER_REPLACEMENT_PATH,
        ),
        (
            b071_valid.replace("DROP TRIGGER", "DROP TABLE", 1),
            B071_TRIGGER_REPLACEMENT_PATH,
        ),
        (b071_valid, "migrations/d1/0143_irrelevant.sql"),
        (
            b071_valid.replace(
                B071_TRIGGER_REPLACEMENT_COMMENT,
                "-- additive-allowed: ADR-0143 copied generic waiver",
                1,
            ),
            B071_TRIGGER_REPLACEMENT_PATH,
        ),
        # The rejected #2036 policy found an approved DROP anywhere in the
        # file and consequently accepted this second, unpaired CREATE.
        (
            b071_valid
            + "CREATE TRIGGER trg_gc_purge_accounting_required AFTER DELETE ON blob_meta\n"
            "BEGIN SELECT 1; END;\n",
            B071_TRIGGER_REPLACEMENT_PATH,
        ),
        (
            b071_valid.replace(
                "trg_gc_purge_accounting_required; " + B071_TRIGGER_REPLACEMENT_COMMENT,
                "trg_gc_purge_finalize_accounting; " + B071_TRIGGER_REPLACEMENT_COMMENT,
                1,
            ),
            B071_TRIGGER_REPLACEMENT_PATH,
        ),
    )
    for mutation, mutation_path in b071_mutations:
        if not scan_sql(mutation, mutation_path):
            print(f"FAIL: B-071 mutation bypass was accepted: {mutation}")
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
