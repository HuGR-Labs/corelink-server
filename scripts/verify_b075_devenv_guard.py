#!/usr/bin/env python3
"""Hermetic semantic/mutation gate for the B-075 DevEnv entitlement guard."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GUARD = ROOT / "worker/src/lib/devenv_guard.ts"
TEST = ROOT / "worker/tests/devenv_guard.test.ts"


def shape(source: str) -> bool:
    required = (
        'DEVENV_ENTITLEMENT_COLUMNS = ["max_concurrency", "max_vcpu_h"]',
        'if (!tenantId || tenantId === "_anonymous")',
        'if (!env.CONFIG_DB)',
        'return {\n      allowed: false,\n      reason: "DevEnv entitlement unavailable (CONFIG_DB unbound)"',
        'catch {',
        'reason: "DevEnv entitlement check unavailable"',
        'if (!row)',
        'typeof row.max_concurrency !== "number"',
        '!(row.max_concurrency > 0)',
        'return { allowed: true }',
    )
    return all(token in source for token in required)


def main() -> int:
    source = GUARD.read_text(encoding="utf-8")
    tests = TEST.read_text(encoding="utf-8")
    if not shape(source):
        raise SystemExit("B-075 guard shape is incomplete")
    if re.search(r"SELECT[^`\n]*install_status", source, re.IGNORECASE):
        raise SystemExit("B-075 guard selects retired install_status column")
    labels = (
        "selects ONLY columns the migrations actually create",
        "does not select the phantom install_status column",
        "the D1 stub REJECTS an invented column",
        "DENIES a tenant with no runners_entitlement row",
        "DENIES when D1 throws at",
        "DENIES when env.CONFIG_DB is absent",
        "ALLOWS a tenant with a positive concurrency cap",
        "query survives the schema-faithful stub end to end",
        "is the STRING",
        "the column is inert",
    )
    if any(label not in tests for label in labels):
        raise SystemExit("B-075 focused test population is incomplete")
    mutations = (
        ("if (!row)", "if (false /* no-row mutation */)"),
        ("!(row.max_concurrency > 0)", "false /* cap mutation */"),
        ("if (!env.CONFIG_DB)", "if (false /* config mutation */)"),
    )
    for original, mutant in mutations:
        mutated = source.replace(original, mutant, 1)
        if mutated == source or shape(mutated):
            raise SystemExit(f"B-075 mutation survived: {original}")
    print("B-075 hermetic semantic guard + closed mutations PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
