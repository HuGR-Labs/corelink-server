"""Source-level regression guards for every durable audit-outbox writer."""

from pathlib import Path
import sqlite3

import pytest


ROOT = Path(__file__).parents[1]
TENANT_WRITERS = (
    ROOT / "crates/corelink-container/src/storage/d1_audit_sink.rs",
    ROOT / "crates/corelink-container/src/routes/dsr/audit.rs",
    ROOT / "crates/corelink-container/src/routes/dsr/access.rs",
)


def test_customer_writers_do_not_default_missing_tenants_to_wnam() -> None:
    for path in TENANT_WRITERS:
        text = path.read_text(encoding="utf-8")
        assert "INSERT OR IGNORE INTO audit_outbox" in text
        assert "COALESCE((SELECT primary_region" not in text
        assert "(SELECT primary_region FROM tenant" in text


def test_public_writer_uses_explicit_namespace_exception() -> None:
    text = (ROOT / "crates/corelink-container/src/routes/public_revoke.rs").read_text(
        encoding="utf-8"
    )
    assert 'PUBLIC_NAMESPACE' in text
    assert "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 'wnam')" in text


def test_migration_rejects_missing_or_mismatched_tenant_without_rewriting_history() -> None:
    text = (ROOT / "migrations/d1/0107_audit_outbox_tenant_residency_guard.sql").read_text(
        encoding="utf-8"
    )
    assert "CREATE TRIGGER IF NOT EXISTS" in text
    assert "NOT EXISTS (" in text
    assert "primary_region = NEW.region" in text
    assert "existing.id = NEW.id" in text
    assert "UPDATE audit_outbox" not in text
    assert "DELETE FROM audit_outbox" not in text


def test_migration_trigger_rejects_unknown_tenant_and_allows_public_namespace() -> None:
    connection = sqlite3.connect(":memory:")
    connection.executescript(
        """
        CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY, primary_region TEXT);
        CREATE TABLE audit_outbox (
          id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, region TEXT NOT NULL,
          UNIQUE (id)
        );
        INSERT INTO tenant VALUES ('tenant-e', 'enam');
        """
    )
    migration = (ROOT / "migrations/d1/0107_audit_outbox_tenant_residency_guard.sql").read_text(
        encoding="utf-8"
    )
    connection.executescript(migration)

    with pytest.raises(sqlite3.IntegrityError, match="residency_unprovable"):
        connection.execute("INSERT INTO audit_outbox VALUES ('bad', 'missing', 'wnam')")
    connection.execute("INSERT INTO audit_outbox VALUES ('ok', '_public', 'wnam')")
    connection.execute("INSERT INTO audit_outbox VALUES ('good', 'tenant-e', 'enam')")
