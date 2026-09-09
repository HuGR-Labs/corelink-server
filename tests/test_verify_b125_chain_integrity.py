from __future__ import annotations

import importlib.util
import sqlite3
import tomllib
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b125_chain_integrity", ROOT / "scripts" / "verify_b125_chain_integrity.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
WIRING_SPEC = importlib.util.spec_from_file_location(
    "verify_b126_m2_wiring", ROOT / "scripts" / "verify_b126_m2_wiring.py"
)
assert WIRING_SPEC and WIRING_SPEC.loader
WIRING_GUARD = importlib.util.module_from_spec(WIRING_SPEC)
WIRING_SPEC.loader.exec_module(WIRING_GUARD)


def test_v2_duplicate_tail_guard_is_defined_and_wired() -> None:
    report = WIRING_GUARD.audit_drain_duplicate_tail_guard_report(ROOT)
    declared_chain = WIRING_GUARD.ROOT_FRAGMENTS[WIRING_GUARD.AUDIT_DRAIN_ROOT]
    assert len(declared_chain) == len(set(declared_chain))
    assert len(report["definition_fragments"]) == 1
    assert report["definition_fragments"][0] in declared_chain
    assert report["definition_fragments"][0].startswith(
        "crates/corelink-container/src/routes/audit_drain/b126_m2_"
    )
    assert len(report["call_fragments"]) == 1
    assert report["call_fragments"][0] in declared_chain


def test_container_manifest_is_valid_and_blake3_is_unique() -> None:
    manifest_text = (
        ROOT / "crates/corelink-container/Cargo.toml"
    ).read_text(encoding="utf-8")
    manifest = tomllib.loads(manifest_text)
    assert manifest["dependencies"]["blake3"] == {"workspace": True}
    assert manifest_text.count("blake3 = { workspace = true }") == 1


def response(**overrides: int) -> dict[str, object]:
    row = {
        "heads": 367,
        "missing_tail_partitions": 0,
        "ambiguous_tail_partitions": 0,
        "sequence_mismatch_partitions": 0,
        "hash_mismatch_partitions": 0,
    }
    row.update(overrides)
    return {"success": True, "result": [{"success": True, "results": [row]}]}


def test_clean_population_is_compliant() -> None:
    counts = MODULE._parse(response())
    assert MODULE.assess(counts)[0] == "COMPLIANT"


@pytest.mark.parametrize(
    "field",
    (
        "missing_tail_partitions",
        "ambiguous_tail_partitions",
        "sequence_mismatch_partitions",
        "hash_mismatch_partitions",
    ),
)
def test_each_integrity_defect_fails(field: str) -> None:
    counts = MODULE._parse(response(**{field: 1}))
    assert MODULE.assess(counts)[0] == "FAILED"


def test_matching_one_of_two_tails_is_still_ambiguous() -> None:
    # Regression for dd35a645: the old control saw one matching hash and passed.
    counts = MODULE._parse(response(ambiguous_tail_partitions=1))
    assert counts["hash_mismatch_partitions"] == 0
    assert MODULE.assess(counts)[0] == "FAILED"
    assert "tail_rows, 0) > 1" in MODULE.HEAD_TAIL_SQL


@pytest.mark.parametrize("value", (None, -1, True, "0"))
def test_malformed_counts_are_indeterminate(value: object) -> None:
    payload = response()
    payload["result"][0]["results"][0]["ambiguous_tail_partitions"] = value
    with pytest.raises(MODULE.Indeterminate):
        MODULE._parse(payload)


def test_resolution_migration_is_inert_and_append_only() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute(
        "CREATE TABLE audit_outbox (id TEXT PRIMARY KEY, tenant_id TEXT, region TEXT, "
        "sequence_number INTEGER, archived_at INTEGER, quarantined_at INTEGER)"
    )
    migration = (
        ROOT / "migrations" / "d1" / "0123_audit_chain_legacy_tail_resolution.sql"
    ).read_text(encoding="utf-8")
    connection.executescript(migration)
    assert connection.execute(
        "SELECT COUNT(*) FROM audit_chain_legacy_tail_resolution"
    ).fetchone() == (0,)
    values = (
        "tenant-a", "enam", 1, 0, "selected-row", "a" * 64, "a" * 64,
        1, "A" * 88, 1, 2, "b" * 64, "B" * 88, 1_700_000_000_000,
    )
    connection.execute(
        "INSERT INTO audit_chain_legacy_tail_resolution VALUES "
        "(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        values,
    )
    with pytest.raises(sqlite3.IntegrityError, match="replacement is forbidden"):
        connection.execute(
            "INSERT OR REPLACE INTO audit_chain_legacy_tail_resolution VALUES "
            "(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            values,
        )
    with pytest.raises(sqlite3.IntegrityError, match="append-only"):
        connection.execute(
            "UPDATE audit_chain_legacy_tail_resolution SET created_at_ms=2"
        )
    with pytest.raises(sqlite3.IntegrityError, match="append-only"):
        connection.execute("DELETE FROM audit_chain_legacy_tail_resolution")


def test_resolution_freezes_only_committed_candidate_sequence() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute(
        "CREATE TABLE audit_outbox (id TEXT PRIMARY KEY, tenant_id TEXT, region TEXT, "
        "sequence_number INTEGER, archived_at INTEGER, quarantined_at INTEGER)"
    )
    connection.execute(
        "INSERT INTO audit_outbox VALUES ('selected-row','tenant-a','enam',0,1,NULL)"
    )
    migration = (
        ROOT / "migrations" / "d1" / "0123_audit_chain_legacy_tail_resolution.sql"
    ).read_text(encoding="utf-8")
    connection.executescript(migration)
    connection.execute(
        "INSERT INTO audit_chain_legacy_tail_resolution VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        (
            "tenant-a", "enam", 1, 0, "selected-row", "a" * 64, "a" * 64,
            1, "A" * 88, 1, 2, "b" * 64, "B" * 88, 1_700_000_000_000,
        ),
    )
    with pytest.raises(sqlite3.IntegrityError, match="candidate set is frozen"):
        connection.execute("UPDATE audit_outbox SET archived_at=2 WHERE sequence_number=0")
    with pytest.raises(sqlite3.IntegrityError, match="candidate set is frozen"):
        connection.execute("DELETE FROM audit_outbox WHERE sequence_number=0")
    with pytest.raises(sqlite3.IntegrityError, match="candidate set is frozen"):
        connection.execute(
            "INSERT INTO audit_outbox VALUES ('fork','tenant-a','enam',0,NULL,1)"
        )
    connection.execute(
        "INSERT INTO audit_outbox VALUES ('next','tenant-a','enam',1,NULL,NULL)"
    )
    connection.execute(
        "INSERT INTO audit_outbox VALUES ('outsider','tenant-b','wnam',NULL,NULL,NULL)"
    )
    with pytest.raises(sqlite3.IntegrityError, match="candidate set is frozen"):
        connection.execute(
            "UPDATE audit_outbox SET tenant_id='tenant-a',region='enam',sequence_number=0 "
            "WHERE id='outsider'"
        )
