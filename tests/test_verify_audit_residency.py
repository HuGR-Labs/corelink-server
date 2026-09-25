"""Behavior and mutation controls for scripts/verify_audit_residency.py."""

from __future__ import annotations

import importlib.util
import json
import sqlite3
import sys
from pathlib import Path

import pytest


SPEC = importlib.util.spec_from_file_location(
    "verify_audit_residency",
    Path(__file__).parents[1] / "scripts" / "verify_audit_residency.py",
)
assert SPEC and SPEC.loader
verifier = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = verifier
SPEC.loader.exec_module(verifier)

# Independently frozen from the producer slugs below, not derived from the
# verifier's allowlist (otherwise removing an event from both SQL and the test
# parameter population would silently pass).
EXPECTED_PUBLIC_EVENTS = (
    "corelink.cas.read.attempted",
    "corelink.cas.read.served",
    "corelink.cas.write.attempted",
    "corelink.cas.write.committed",
    "public.revoke",
    "corelink.signup.pilot_reserved.v1",
    "corelink.signup.pilot_token_rejected.v1",
    "corelink.signup.pilot_rate_limited.v1",
)


def test_public_allowlist_matches_independent_writer_slugs() -> None:
    assert set(verifier.PUBLIC_EVENTS) == set(EXPECTED_PUBLIC_EVENTS)
    source_root = Path(__file__).parents[1] / "crates" / "corelink-container" / "src"
    writer_sources = {
        source_root.parents[1] / "corelink-handler-cas" / "src" / "audit.rs": EXPECTED_PUBLIC_EVENTS[:4],
        source_root / "routes" / "public_revoke.rs": (EXPECTED_PUBLIC_EVENTS[4],),
        source_root / "routes" / "signup.rs": EXPECTED_PUBLIC_EVENTS[5:],
    }
    for path, slugs in writer_sources.items():
        source = path.read_text(encoding="utf-8")
        for slug in slugs:
            assert f'"{slug}"' in source, f"producer slug {slug} missing from {path}"


def response(**overrides: int) -> dict:
    row = {
        "total_rows": 10,
        "customer_rows": 10,
        "public_rows": 0,
        "reserved_public_rows": 0,
        "invalid_public_rows": 0,
        "satisfied_rows": 10,
        "violated_rows": 0,
        "unevaluable_rows": 0,
        "customer_unevaluable_rows": 0,
        "orphan_rows": 0,
        "orphan_tenants": 0,
        "erased_orphan_rows": 0,
        "erased_orphan_tenants": 0,
        "unexplained_orphan_rows": 0,
        "unexplained_orphan_tenants": 0,
        "weur_audit_rows": 0,
        "weur_orphan_rows": 0,
        "weur_tenants": 0,
        "erasure_log_rows": 1,
    }
    row.update(overrides)
    return {"success": True, "result": [{"success": True, "results": [row]}]}


def assess(payload: dict, environment: str = "production") -> tuple[str, str]:
    return verifier.assess(verifier.parse_d1_response(payload), environment=environment)


def sql_counts(
    *,
    include_mismatch: bool = False,
    include_orphan: bool = False,
    malformed_region: bool = False,
    public_region: str | None = None,
    public_event: str = "public.revoke",
) -> dict:
    """Execute the actual aggregate against a minimal D1-compatible SQLite schema."""
    connection = sqlite3.connect(":memory:")
    connection.executescript(
        """
        CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY, primary_region TEXT);
        CREATE TABLE audit_outbox (
            tenant_id TEXT NOT NULL, region TEXT,
            event_type TEXT NOT NULL DEFAULT 'corelink.cas.read.served'
        );
        CREATE TABLE dsr_erasure_log (tenant_id TEXT NOT NULL);
        INSERT INTO tenant VALUES ('tenant-e', 'enam');
        INSERT INTO audit_outbox (tenant_id, region) VALUES ('tenant-e', 'enam');
        INSERT INTO dsr_erasure_log VALUES ('erased-tenant');
        """
    )
    if include_mismatch:
        connection.execute("INSERT INTO tenant VALUES ('tenant-w', 'wnam')")
        connection.execute("INSERT INTO audit_outbox (tenant_id, region) VALUES ('tenant-w', 'enam')")
    if include_orphan:
        connection.execute("INSERT INTO audit_outbox (tenant_id, region) VALUES ('erased-tenant', 'weur')")
    if malformed_region:
        connection.execute("INSERT INTO audit_outbox (tenant_id, region) VALUES ('tenant-e', 'unknown-region')")
    if public_region is not None:
        connection.execute(
            "INSERT INTO audit_outbox (tenant_id, region, event_type) VALUES ('_public', ?, ?)",
            (public_region, public_event),
        )
    row = connection.execute(verifier.RESIDENCY_SQL).fetchone()
    assert row is not None
    return dict(zip((field[0] for field in connection.execute(verifier.RESIDENCY_SQL).description), row, strict=True))


def test_complete_population_with_only_satisfied_rows_is_compliant() -> None:
    assert assess(response())[0] == "COMPLIANT"


def test_sql_classifies_a_matching_tenant_as_satisfied() -> None:
    # Mutation control: changing equality in RESIDENCY_SQL makes this red.
    counts = sql_counts()
    assert counts["total_rows"] == 1
    assert counts["satisfied_rows"] == 1
    assert counts["violated_rows"] == 0
    assert counts["unevaluable_rows"] == 0


def test_known_cross_region_mismatch_fails() -> None:
    # Mutation control: changing only this count to zero makes this test fail.
    assert assess(response(satisfied_rows=9, violated_rows=1))[0] == "FAILED"


def test_sql_keeps_mismatch_and_erased_orphan_in_distinct_failing_buckets() -> None:
    counts = sql_counts(include_mismatch=True, include_orphan=True)
    assert counts["total_rows"] == 3
    assert counts["satisfied_rows"] == 1
    assert counts["violated_rows"] == 1
    assert counts["unevaluable_rows"] == 1
    assert counts["erased_orphan_rows"] == 1
    assert counts["unexplained_orphan_rows"] == 0


def test_sql_accounts_for_reserved_public_without_laundering_customer_orphans() -> None:
    counts = sql_counts(include_orphan=True, public_region="wnam")
    assert counts["total_rows"] == 3
    assert counts["customer_rows"] == 2
    assert counts["public_rows"] == counts["reserved_public_rows"] == 1
    assert counts["invalid_public_rows"] == 0
    assert counts["orphan_rows"] == counts["erased_orphan_rows"] == 1
    assert counts["customer_unevaluable_rows"] == counts["unevaluable_rows"] == 1


@pytest.mark.parametrize("event", EXPECTED_PUBLIC_EVENTS)
def test_canonical_public_event_in_wnam_is_reserved(event: str) -> None:
    counts = sql_counts(public_region="wnam", public_event=event)
    assert counts["total_rows"] == 2
    assert counts["reserved_public_rows"] == 1
    assert counts["unevaluable_rows"] == 0
    assert counts["orphan_rows"] == 0


@pytest.mark.parametrize(
    ("region", "event"),
    [("weur", "public.revoke"), ("wnam", "corelink.dsr.access"),
     ("weur", "corelink.dsr.access")],
)
def test_public_wrong_region_or_event_remains_visible_and_fails(region: str, event: str) -> None:
    counts = sql_counts(public_region=region, public_event=event)
    assert counts["public_rows"] == counts["invalid_public_rows"] == 1
    assert counts["reserved_public_rows"] == 0
    assert counts["unevaluable_rows"] == 1
    assert counts["orphan_rows"] == 0
    assert verifier.assess(verifier.Counts(**counts), environment="production")[0] == "FAILED"


def test_public_rows_do_not_make_customer_residual_compliant() -> None:
    payload = response(
        total_rows=4, customer_rows=3, public_rows=1,
        reserved_public_rows=1, satisfied_rows=2,
        unevaluable_rows=1, customer_unevaluable_rows=1,
        orphan_rows=1, orphan_tenants=1,
        unexplained_orphan_rows=1, unexplained_orphan_tenants=1,
    )
    assert assess(payload)[0] == "FAILED"


def test_sql_classifies_unknown_region_as_unevaluable_not_satisfied() -> None:
    counts = sql_counts(malformed_region=True)
    assert counts["total_rows"] == 2
    assert counts["satisfied_rows"] == 1
    assert counts["unevaluable_rows"] == 1


def test_retained_dsr_orphan_is_unevaluable_and_fails() -> None:
    # The erased row remains RETAIN evidence, but must not be collapsed to pass.
    payload = response(
        satisfied_rows=9,
        unevaluable_rows=1,
        customer_unevaluable_rows=1,
        orphan_rows=1,
        orphan_tenants=1,
        erased_orphan_rows=1,
        erased_orphan_tenants=1,
    )
    assert assess(payload)[0] == "FAILED"


def test_unexplained_orphan_and_weur_orphan_remain_in_the_denominator() -> None:
    payload = response(
        satisfied_rows=9,
        unevaluable_rows=1,
        customer_unevaluable_rows=1,
        orphan_rows=1,
        orphan_tenants=1,
        unexplained_orphan_rows=1,
        unexplained_orphan_tenants=1,
        weur_audit_rows=1,
        weur_orphan_rows=1,
    )
    state, _ = assess(payload)
    assert state == "FAILED"


def test_staging_without_orphans_can_use_an_empty_dsr_control() -> None:
    assert assess(response(erasure_log_rows=0), environment="staging")[0] == "COMPLIANT"


def test_missing_evidence_source_is_indeterminate() -> None:
    with pytest.raises(SystemExit) as exited:
        verifier.main(["--environment", "test"])
    assert exited.value.code == 2


@pytest.mark.parametrize("result_success", [False, None, 0, 1, "true"])
def test_result_set_success_must_be_exact_boolean_true(result_success: object) -> None:
    payload = response()
    payload["result"][0]["success"] = result_success
    with pytest.raises(verifier.Indeterminate):
        verifier.parse_d1_response(payload)


def test_result_set_success_is_required() -> None:
    payload = response()
    del payload["result"][0]["success"]
    with pytest.raises(verifier.Indeterminate):
        verifier.parse_d1_response(payload)


def test_aggregate_row_must_contain_only_the_allowlisted_counts() -> None:
    payload = response()
    payload["result"][0]["results"][0]["tenant_id"] = "row-level-identity-must-not-be-accepted"

    with pytest.raises(verifier.Indeterminate, match="fields are missing or unexpected"):
        verifier.parse_d1_response(payload)


@pytest.mark.parametrize("result_success", [False, None, 0, 1, "true"])
def test_incomplete_result_set_evidence_exits_2(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], result_success: object
) -> None:
    payload = response()
    payload["result"][0]["success"] = result_success
    evidence = tmp_path / "d1-response.json"
    evidence.write_text(json.dumps(payload), encoding="utf-8")

    with pytest.raises(SystemExit) as exited:
        verifier.main(["--environment", "production", "--input", str(evidence)])

    assert exited.value.code == 2
    assert "status=INDETERMINATE" in capsys.readouterr().err


@pytest.mark.parametrize(
    "payload",
    [
        response(total_rows=0, satisfied_rows=0),
        response(total_rows=10, customer_rows=9),
        response(public_rows=1),
        response(customer_rows=9, public_rows=1),
        response(total_rows=10, customer_rows=9, public_rows=1, reserved_public_rows=1),
        response(total_rows=10, satisfied_rows=9),
        response(orphan_rows=1),
        response(orphan_tenants=1),
        response(erasure_log_rows=0),
        {"success": True, "result": [{"success": True, "results": [{}]}]},
    ],
)
def test_empty_partial_or_malformed_evidence_is_indeterminate(payload: dict) -> None:
    with pytest.raises(verifier.Indeterminate):
        assess(payload)


def test_query_uses_left_join_exists_customer_states_and_reserved_public() -> None:
    # Source-level negative control for the old INNER JOIN / multiplicative DSR join.
    sql = verifier.RESIDENCY_SQL.lower()
    assert "left join tenant" in sql
    assert "exists (" in sql
    assert "'satisfied'" in sql and "'violated'" in sql and "'unevaluable'" in sql
    assert "'reserved_public'" in sql
    assert "join dsr_erasure_log" not in sql
