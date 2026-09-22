"""Adversarial fixtures for the issue #1669 aggregate reconciliation."""

from __future__ import annotations

import importlib.util
import sqlite3
import sys
from pathlib import Path


ROOT = Path(__file__).parents[1]
SPEC = importlib.util.spec_from_file_location(
    "probe_i1669_readonly", ROOT / "scripts" / "probe_i1669_readonly.py"
)
assert SPEC and SPEC.loader
probe = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = probe
SPEC.loader.exec_module(probe)


def aggregate_fixture(*, invalid_public_region: bool = False) -> tuple[dict, dict]:
    """Return both aggregates over a population containing every row class."""
    connection = sqlite3.connect(":memory:")
    connection.executescript(
        """
        CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY, primary_region TEXT);
        CREATE TABLE audit_outbox (tenant_id TEXT NOT NULL, region TEXT NOT NULL);
        CREATE TABLE dsr_erasure_log (tenant_id TEXT NOT NULL);
        INSERT INTO tenant VALUES ('tenant-e', 'enam');
        INSERT INTO tenant VALUES ('tenant-w', 'wnam');
        INSERT INTO audit_outbox VALUES ('tenant-e', 'enam');
        INSERT INTO audit_outbox VALUES ('tenant-w', 'enam');
        INSERT INTO audit_outbox VALUES ('erased-tenant', 'weur');
        INSERT INTO audit_outbox VALUES ('unexplained-tenant', 'apac');
        INSERT INTO audit_outbox VALUES ('_public', 'wnam');
        INSERT INTO dsr_erasure_log VALUES ('erased-tenant');
        """
    )
    if invalid_public_region:
        connection.execute(
            "UPDATE audit_outbox SET region = 'enam' WHERE tenant_id = '_public'"
        )

    residency_cursor = connection.execute(probe.RESIDENCY.RESIDENCY_SQL)
    residency = dict(zip((column[0] for column in residency_cursor.description), residency_cursor.fetchone(), strict=True))
    backfill_cursor = connection.execute(probe.BACKFILL_COMPLETENESS_SQL)
    backfill = dict(zip((column[0] for column in backfill_cursor.description), backfill_cursor.fetchone(), strict=True))
    return residency, backfill


def test_public_namespace_is_a_separate_backfill_partition() -> None:
    residency, backfill = aggregate_fixture()

    assert residency["total_rows"] == 5
    assert residency["orphan_rows"] == 2
    assert residency["system_scope_rows"] == 1
    assert residency["violated_rows"] == 1
    assert backfill == {
        "audit_rows": 5,
        "orphan_rows": 2,
        "joinable_rows": 2,
        "system_scope_rows": 1,
        "invalid_system_scope_rows": 0,
        "erased_orphan_rows": 1,
    }
    assert backfill["orphan_rows"] == residency["orphan_rows"]
    assert backfill["system_scope_rows"] == residency["system_scope_rows"]
    assert backfill["invalid_system_scope_rows"] == residency["invalid_system_scope_rows"]
    assert backfill["erased_orphan_rows"] == residency["erased_orphan_rows"]
    assert (
        backfill["orphan_rows"]
        + backfill["joinable_rows"]
        + backfill["system_scope_rows"]
        == residency["total_rows"]
    )


def test_invalid_public_region_still_reconciles_as_system_scope() -> None:
    residency, backfill = aggregate_fixture(invalid_public_region=True)

    assert residency["invalid_system_scope_rows"] == 1
    assert residency["violated_rows"] == 2
    assert backfill["orphan_rows"] == residency["orphan_rows"]
    assert backfill["system_scope_rows"] == residency["system_scope_rows"]
    assert backfill["erased_orphan_rows"] == residency["erased_orphan_rows"]


def test_backfill_allowlist_requires_explicit_system_scope_bucket() -> None:
    assert "a.tenant_id = '_public'" in probe.BACKFILL_COMPLETENESS_SQL
    assert "system_scope_rows" in probe.BACKFILL_FIELDS
