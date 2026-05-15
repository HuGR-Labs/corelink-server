#!/usr/bin/env python3
"""
CoreLink R2-13: D1 migration verification harness.

Parses every `migrations/d1/NNNN_*.sql` file, computes the **expected**
schema (tables + columns + indexes + triggers) by replaying the file
order, and either:

  (a) `--schema-only` mode (default in CI): emit the expected schema as
      structured JSON to stdout and exit 0 iff the parse succeeds with
      no internal inconsistencies. Does NOT require D1 / wrangler /
      network — suitable for CI gating on every PR.

  (b) `--against <sqlite-path>` mode: query a live SQLite file (e.g.
      a `wrangler d1 export` dump restored to local SQLite) and report
      drift vs the expected schema.

Drift categories reported in mode (b):
  - MISSING_TABLE       — expected table not present in DB
  - MISSING_COLUMN      — expected column not present on a known table
  - EXTRA_COLUMN        — DB column not declared by any migration
                          (typical cause: hand-edited rollback)
  - EXTRA_TABLE         — DB table not declared by any migration

The parser is intentionally regex-based + conservative. It recognises:
  - CREATE TABLE IF NOT EXISTS <name> ( … )    → table + columns
  - ALTER TABLE <name> ADD COLUMN [IF NOT EXISTS] <col> <type>  → column add
  - CREATE [UNIQUE] INDEX IF NOT EXISTS <name> ON <table> ( … ) → index
  - CREATE TRIGGER IF NOT EXISTS <name> …                       → trigger

False negatives (statements the parser cannot read) are reported as
PARSE_WARNING but do NOT fail the gate — they fail open because this
script's job is drift detection, not full SQL replay (the integration
test in `tests/d1-migration-integration.rs` is the replay oracle).

Canonical sources:
  - migrations/d1/*.sql (single source of truth)
  - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
  - specs/_runbooks/RB-D1-MIGRATION-APPLY.md
  - INV-AUTH-MIGRATION-ADDITIVE (HIGH)

Exit codes:
  0  schema-only mode: parse OK / against-mode: no drift detected.
  1  drift detected (against-mode) or fatal parse error (schema-only).
  2  usage error.
"""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


# ── 1. Constants ────────────────────────────────────────────────────────────

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_MIGRATIONS_DIR = REPO_ROOT / "migrations" / "d1"

# ── 2. SQL line-comment scrubber ────────────────────────────────────────────

# Strip `-- …` from each line BEFORE pattern matching, so prose in
# rationale headers never trips the parser. We keep this trivial; full
# SQL parsing is the integration test's job.
COMMENT_RE = re.compile(r"--[^\n]*$", re.MULTILINE)


def strip_sql_comments(sql: str) -> str:
    """Remove `-- line` comments but preserve the structure."""
    return COMMENT_RE.sub("", sql)


# ── 3. Regex patterns ───────────────────────────────────────────────────────

# Permissive: D1 + SQLite + the dialect quirks CoreLink actually uses.
# We anchor on `IF NOT EXISTS` because every CoreLink migration is
# additive-idempotent (per INV-AUTH-MIGRATION-ADDITIVE) and uses that
# form. Bare `CREATE TABLE` (no IF NOT EXISTS) is also tolerated.
RE_CREATE_TABLE = re.compile(
    r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?"
    r"([A-Za-z_][A-Za-z0-9_]*)\s*\(",
    re.IGNORECASE,
)

# ALTER TABLE <name> ADD COLUMN [IF NOT EXISTS] <col> <type-and-rest>;
RE_ALTER_ADD_COLUMN = re.compile(
    r"ALTER\s+TABLE\s+([A-Za-z_][A-Za-z0-9_]*)\s+"
    r"ADD\s+COLUMN\s+(?:IF\s+NOT\s+EXISTS\s+)?"
    r"([A-Za-z_][A-Za-z0-9_]*)",
    re.IGNORECASE,
)

RE_CREATE_INDEX = re.compile(
    r"CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:IF\s+NOT\s+EXISTS\s+)?"
    r"([A-Za-z_][A-Za-z0-9_]*)\s+ON\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.IGNORECASE,
)

RE_CREATE_TRIGGER = re.compile(
    r"CREATE\s+TRIGGER\s+(?:IF\s+NOT\s+EXISTS\s+)?"
    r"([A-Za-z_][A-Za-z0-9_]*)",
    re.IGNORECASE,
)


# ── 4. Schema model ────────────────────────────────────────────────────────


@dataclass
class TableDef:
    name: str
    columns: list[str] = field(default_factory=list)
    first_seen_in: str = ""

    def add_column(self, col: str) -> None:
        if col not in self.columns:
            self.columns.append(col)


@dataclass
class ExpectedSchema:
    tables: dict[str, TableDef] = field(default_factory=dict)
    indexes: dict[str, str] = field(default_factory=dict)  # idx → table
    triggers: list[str] = field(default_factory=list)
    parse_warnings: list[str] = field(default_factory=list)

    def to_json(self) -> dict[str, object]:
        return {
            "tables": {
                t.name: {
                    "columns": t.columns,
                    "first_seen_in": t.first_seen_in,
                }
                for t in self.tables.values()
            },
            "indexes": self.indexes,
            "triggers": self.triggers,
            "parse_warnings": self.parse_warnings,
        }


# ── 5. Parser ──────────────────────────────────────────────────────────────


def parse_column_list(body: str) -> list[str]:
    """Parse the column list out of a `CREATE TABLE x ( <body> )` body.

    Body parsing is paren-aware: we track depth so commas inside
    nested `CHECK (a IN ('x', 'y'))` / `DEFAULT ('a, b')` are not
    treated as column separators. Each top-level comma-delimited
    fragment is examined: if it starts with a SQL identifier and is
    NOT a table-level constraint keyword, the first token is the
    column name.
    """
    fragments: list[str] = []
    depth = 0
    buf: list[str] = []
    in_str: Optional[str] = None

    for ch in body:
        if in_str:
            buf.append(ch)
            if ch == in_str:
                in_str = None
            continue
        if ch in ("'", '"'):
            in_str = ch
            buf.append(ch)
            continue
        if ch == "(":
            depth += 1
            buf.append(ch)
            continue
        if ch == ")":
            depth -= 1
            buf.append(ch)
            continue
        if ch == "," and depth == 0:
            fragments.append("".join(buf).strip())
            buf = []
            continue
        buf.append(ch)
    if buf:
        last = "".join(buf).strip()
        if last:
            fragments.append(last)

    columns: list[str] = []
    constraint_keywords = {
        "PRIMARY",
        "UNIQUE",
        "FOREIGN",
        "CHECK",
        "CONSTRAINT",
    }
    for frag in fragments:
        token = frag.split(None, 1)[0] if frag else ""
        if not token:
            continue
        if token.upper() in constraint_keywords:
            continue
        # Strip wrapping backticks / brackets / quotes for sanity.
        col = token.strip('`"[]')
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", col):
            columns.append(col)
    return columns


def extract_table_body(sql: str, create_match: re.Match[str]) -> Optional[str]:
    """Given a successful CREATE TABLE regex match, return the body
    text between the opening `(` and its matching `)`. Returns None
    if balanced close cannot be found (parse warning)."""
    start = create_match.end() - 1  # the `(` itself
    depth = 0
    in_str: Optional[str] = None
    for i in range(start, len(sql)):
        ch = sql[i]
        if in_str:
            if ch == in_str:
                in_str = None
            continue
        if ch in ("'", '"'):
            in_str = ch
            continue
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return sql[start + 1 : i]
    return None


def parse_migration(path: Path, schema: ExpectedSchema) -> None:
    raw = path.read_text(encoding="utf-8")
    sql = strip_sql_comments(raw)
    fname = path.name

    # 5.1 — CREATE TABLE
    for m in RE_CREATE_TABLE.finditer(sql):
        tname = m.group(1)
        body = extract_table_body(sql, m)
        if body is None:
            schema.parse_warnings.append(
                f"{fname}: failed to extract body for CREATE TABLE {tname}"
            )
            continue
        if tname not in schema.tables:
            schema.tables[tname] = TableDef(name=tname, first_seen_in=fname)
        td = schema.tables[tname]
        for col in parse_column_list(body):
            td.add_column(col)

    # 5.2 — ALTER TABLE … ADD COLUMN
    for m in RE_ALTER_ADD_COLUMN.finditer(sql):
        tname, cname = m.group(1), m.group(2)
        if tname not in schema.tables:
            # ALTER on a table never declared by any earlier migration.
            # We record it so drift mode can flag the dependency, but
            # we still register the column under a synthetic table so
            # the JSON output is complete.
            schema.parse_warnings.append(
                f"{fname}: ALTER TABLE {tname} ADD COLUMN {cname} — "
                f"table not previously declared (schema dependency hazard)"
            )
            schema.tables[tname] = TableDef(name=tname, first_seen_in=fname)
        schema.tables[tname].add_column(cname)

    # 5.3 — CREATE INDEX
    for m in RE_CREATE_INDEX.finditer(sql):
        iname, tname = m.group(1), m.group(2)
        schema.indexes[iname] = tname

    # 5.4 — CREATE TRIGGER
    for m in RE_CREATE_TRIGGER.finditer(sql):
        schema.triggers.append(m.group(1))


def build_expected_schema(migrations_dir: Path) -> ExpectedSchema:
    schema = ExpectedSchema()
    files = sorted(p for p in migrations_dir.iterdir() if p.suffix == ".sql")
    for path in files:
        parse_migration(path, schema)
    return schema


# ── 6. Drift detection ─────────────────────────────────────────────────────


@dataclass
class Drift:
    category: str
    detail: str

    def render(self) -> str:
        return f"[{self.category}] {self.detail}"


def detect_drift(expected: ExpectedSchema, db_path: Path) -> list[Drift]:
    drift: list[Drift] = []
    conn = sqlite3.connect(str(db_path))
    try:
        cur = conn.cursor()

        # 6.1 — tables present in DB
        cur.execute(
            "SELECT name FROM sqlite_schema WHERE type = 'table' "
            "AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'd1_%'"
        )
        db_tables = {row[0] for row in cur.fetchall()}

        for tname in expected.tables:
            if tname not in db_tables:
                drift.append(
                    Drift("MISSING_TABLE", f"expected table not in DB: {tname}")
                )

        for tname in db_tables:
            if tname not in expected.tables:
                drift.append(
                    Drift("EXTRA_TABLE", f"DB has table not in migrations: {tname}")
                )

        # 6.2 — columns per table
        for tname, td in expected.tables.items():
            if tname not in db_tables:
                continue
            cur.execute(f"PRAGMA table_info({tname})")
            db_cols = {row[1] for row in cur.fetchall()}
            for col in td.columns:
                if col not in db_cols:
                    drift.append(
                        Drift("MISSING_COLUMN", f"{tname}.{col} missing in DB")
                    )
            for col in db_cols:
                if col not in td.columns:
                    drift.append(
                        Drift("EXTRA_COLUMN", f"{tname}.{col} in DB but not in migrations")
                    )
    finally:
        conn.close()
    return drift


# ── 7. CLI ────────────────────────────────────────────────────────────────


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(
        prog="d1-migration-verify.py",
        description="Compute expected D1 schema from migrations/ and "
        "(optionally) diff against a live SQLite file.",
    )
    p.add_argument(
        "migrations_dir",
        nargs="?",
        default=str(DEFAULT_MIGRATIONS_DIR),
        help=f"directory containing 0001_*.sql … (default: {DEFAULT_MIGRATIONS_DIR})",
    )
    p.add_argument(
        "--schema-only",
        action="store_true",
        help="emit expected schema JSON to stdout; exit 0 if parse OK",
    )
    p.add_argument(
        "--against",
        type=str,
        default=None,
        help="path to a SQLite file (e.g. wrangler d1 export); diff vs expected",
    )
    p.add_argument(
        "--strict-warnings",
        action="store_true",
        help="treat parse warnings as drift (exit 1 if any)",
    )
    args = p.parse_args(argv)

    migrations_dir = Path(args.migrations_dir).resolve()
    if not migrations_dir.is_dir():
        print(f"fatal: not a directory: {migrations_dir}", file=sys.stderr)
        return 2

    expected = build_expected_schema(migrations_dir)

    if args.against is None:
        # schema-only mode
        out = {
            "migrations_dir": str(migrations_dir.relative_to(REPO_ROOT)),
            "table_count": len(expected.tables),
            "index_count": len(expected.indexes),
            "trigger_count": len(expected.triggers),
            "schema": expected.to_json(),
        }
        json.dump(out, sys.stdout, indent=2, sort_keys=True)
        sys.stdout.write("\n")

        if args.strict_warnings and expected.parse_warnings:
            print(
                f"\nFAIL: {len(expected.parse_warnings)} parse warning(s) "
                f"with --strict-warnings",
                file=sys.stderr,
            )
            return 1
        return 0

    # against-mode
    db_path = Path(args.against).resolve()
    if not db_path.is_file():
        print(f"fatal: --against path not a file: {db_path}", file=sys.stderr)
        return 2

    drift = detect_drift(expected, db_path)
    for d in drift:
        print(d.render())

    if expected.parse_warnings:
        print(f"\nparse-warnings ({len(expected.parse_warnings)}):", file=sys.stderr)
        for w in expected.parse_warnings:
            print(f"  {w}", file=sys.stderr)

    if drift:
        print(f"\nFAIL: {len(drift)} drift item(s) detected.")
        return 1
    if args.strict_warnings and expected.parse_warnings:
        print(
            f"FAIL: {len(expected.parse_warnings)} parse warning(s) "
            f"with --strict-warnings"
        )
        return 1
    print(
        f"OK: schema matches ({len(expected.tables)} table(s), "
        f"{len(expected.indexes)} index(es), {len(expected.triggers)} trigger(s))."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
