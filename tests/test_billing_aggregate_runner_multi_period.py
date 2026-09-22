"""Regression net for the #1631 local SQLite workflow seam."""

import importlib.util
from pathlib import Path


ROOT = Path(__file__).parents[1]
SPEC = importlib.util.spec_from_file_location("aggregate_seam", ROOT / "scripts/test_billing_aggregate_runner_multi_period.py")
seam = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(seam)
T1, T2 = "tenant-a", "tenant-b"


def test_oldest_historical_and_current_are_bounded_fairly_and_ordered() -> None:
    db = seam.database()
    for period in ("2025-12", "2026-01", "2026-02"):
        seam.terms(db, T1, period)
    for i in range(2001): seam.stage(db, T1, f"old-{i:04}", "2025-12", emitted=i)
    seam.stage(db, T1, "middle", "2026-01", emitted=1)
    for i in range(8001): seam.stage(db, T1, f"current-{i:04}", "2026-02", emitted=i)
    assert seam.periods(db, "2026-02") == ["2025-12", "2026-02"]
    historical, _ = seam.extract(db, "2025-12", 2000)
    current, _ = seam.extract(db, "2026-02", 8000)
    assert [row["request_id"] for row in historical[:2]] == ["old-0000", "old-0001"]
    assert len(historical) == 2000 and len(current) == 8000
    # The unselected middle period remains pending; the selected slices are
    # checked above without turning this cap regression into a 10k-row commit.
    assert db.execute("SELECT runner_aggregated_at FROM usage_event_staging WHERE request_id='middle'").fetchone()[0] is None


def test_calendar_boundaries_ordering_and_final_day_late_arrival() -> None:
    assert seam.month_bounds("2025-12")[1] == seam.month_bounds("2026-01")[0]
    assert seam.month_bounds("2024-02")[1] - seam.month_bounds("2024-02")[0] == 29 * 86400000
    db = seam.database(); seam.terms(db, T1, "2026-01")
    seam.stage(db, T2, "z", "2026-01", emitted=99); seam.terms(db, T2, "2026-01")
    seam.stage(db, T1, "b", "2026-01", emitted=1); seam.stage(db, T1, "a", "2026-01", emitted=1)
    rows, _ = seam.extract(db, "2026-01", 8)
    assert [row["request_id"] for row in rows] == ["a", "b", "z"]
    seam.run(db, "2026-01")
    seam.stage(db, T1, "late-final-day", "2026-01", emitted=31 * 86400000 - 1)
    assert seam.run(db, "2026-01")[0] == ["2026-01"]


def test_empty_and_missing_terms_rows_are_noops_or_pending() -> None:
    db = seam.database()
    assert seam.run(db, "2026-02") == ([], [])
    seam.stage(db, T1, "blocked", "2026-02", emitted=1)
    assert seam.run(db, "2026-02") == ([], [("2026-02", "blocked")])
    assert db.execute("SELECT runner_aggregated_at FROM usage_event_staging").fetchone()[0] is None


def test_conflicts_quarantine_the_whole_period_and_roll_back_every_mutation() -> None:
    # #1635: a durable winner quarantines the complete stale extraction.
    db = seam.database(); seam.terms(db, T1, "2026-02")
    seam.stage(db, T1, "fresh", "2026-02", emitted=1); seam.stage(db, T1, "stale", "2026-02", emitted=2)
    stale, _ = seam.extract(db, "2026-02", 8); seam.commit(db, [stale[1]], "2026-02")
    before = seam.snapshot(db)
    try: seam.commit(db, stale, "2026-02", now=2000)
    except seam.Conflict: pass
    else: raise AssertionError("stale winner was accepted")
    assert seam.snapshot(db) == before
    assert db.execute("SELECT count(*) FROM runner_aggregate_event_claim WHERE request_id='fresh'").fetchone()[0] == 0


def test_failed_batch_lost_ack_and_bare_watermark_never_reapply_accounting() -> None:
    db = seam.database(); seam.terms(db, T1, "2026-02"); seam.stage(db, T1, "one", "2026-02", emitted=1)
    db.commit()
    rows, _ = seam.extract(db, "2026-02", 8); before = seam.snapshot(db)
    try: seam.commit(db, rows, "2026-02", fail=True)
    except seam.Conflict: pass
    else: raise AssertionError("failed atomic batch passed")
    assert seam.snapshot(db) == before
    assert seam.commit(db, rows, "2026-02") == "Committed"; committed = seam.snapshot(db)
    assert seam.commit(db, rows, "2026-02") == "Deduped" and seam.snapshot(db) == committed
    db2 = seam.database(); seam.terms(db2, T1, "2026-02"); seam.stage(db2, T1, "bare", "2026-02", emitted=1)
    db2.execute("UPDATE usage_event_staging SET runner_aggregated_at=1"); db2.commit(); bare, _ = seam.extract(db2, "2026-02", 8)
    assert not bare
    # Commit's discovery path receives the stale extraction a writer could retain.
    row = db2.execute("SELECT tenant_id,request_id,region,qty,event_payload_hash,emitted_at FROM usage_event_staging").fetchone()
    try: seam.commit(db2, [row], "2026-02")
    except seam.Conflict as exc: assert "watermark" in str(exc)
    else: raise AssertionError("bare watermark accepted")
