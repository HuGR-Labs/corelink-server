#!/usr/bin/env python3
"""
Enforce UNIQUE 4-digit prefixes across every `migrations/d1/*.sql` file.

Why: wrangler applies D1 migrations in lexicographic order and tracks them
in its internal `d1_migrations` ledger by filename. Two files that share the
same 4-digit prefix (the duplicate-`0044` class) are an ambiguous-ordering /
ledger-desync landmine — a re-provision can apply them in an order that
differs from the order prod was first built in, or skip one entirely. This
gate FAILS fast at PR-time so a new duplicate prefix can never land.

Grandfathered exception:
    There is ONE pre-existing duplicate prefix in prod — `0044`
    (`0044_drata_evidence_sent.sql` + `0044_stripe_webhook_events_processed.sql`).
    Both are ALREADY APPLIED in production and MUST NOT be renamed (renaming
    a file changes its wrangler-ledger identity and would re-apply / desync).
    It is therefore explicitly grandfathered below. No NEW duplicate of `0044`
    (a third `0044_*.sql`) is allowed — the grandfather is exact-set, not
    prefix-blanket.

Exit codes:
    0 — every prefix is unique (modulo the grandfathered set).
    1 — at least one disallowed duplicate prefix was detected.
    2 — usage / environment error (migrations dir missing).
"""

from __future__ import annotations

import re
import sys
from collections import defaultdict
from pathlib import Path

# repo_root/scripts/check_migration_prefixes.py -> repo_root
REPO_ROOT = Path(__file__).resolve().parent.parent
MIGRATIONS_DIR = REPO_ROOT / "migrations" / "d1"

PREFIX_RE = re.compile(r"^(\d{4})_.*\.sql$")

# Grandfathered duplicate(s): prefix -> exact set of filenames allowed to
# share it. Already applied in prod; MUST NOT be renamed. Anything outside
# this exact set sharing the prefix is still a failure.
GRANDFATHERED: dict[str, frozenset[str]] = {
    "0044": frozenset(
        {
            "0044_drata_evidence_sent.sql",
            "0044_stripe_webhook_events_processed.sql",
        }
    ),
}


def main() -> int:
    if not MIGRATIONS_DIR.is_dir():
        print(f"ERROR: migrations dir not found: {MIGRATIONS_DIR}", file=sys.stderr)
        return 2

    by_prefix: dict[str, list[str]] = defaultdict(list)
    for path in sorted(MIGRATIONS_DIR.glob("*.sql")):
        m = PREFIX_RE.match(path.name)
        if not m:
            print(
                f"ERROR: {path.name} does not match the NNNN_name.sql convention",
                file=sys.stderr,
            )
            return 1
        by_prefix[m.group(1)].append(path.name)

    violations: list[str] = []
    for prefix, names in sorted(by_prefix.items()):
        if len(names) == 1:
            continue
        actual = frozenset(names)
        allowed = GRANDFATHERED.get(prefix)
        if allowed is not None and actual == allowed:
            print(
                f"OK (grandfathered): prefix {prefix} shared by "
                f"{', '.join(sorted(names))}"
            )
            continue
        violations.append(
            f"  {prefix}: {', '.join(sorted(names))}"
        )

    if violations:
        print(
            "FAIL: migrations/d1 has files sharing a 4-digit prefix.\n"
            "Each migration must have a UNIQUE 4-digit prefix (ambiguous "
            "ordering desyncs the wrangler d1_migrations ledger on re-provision).\n"
            "Renumber the NEWER file to the next free prefix:\n"
            + "\n".join(violations),
            file=sys.stderr,
        )
        return 1

    total = sum(len(v) for v in by_prefix.values())
    print(f"OK: {total} migration file(s), all prefixes unique (0044 grandfathered).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
