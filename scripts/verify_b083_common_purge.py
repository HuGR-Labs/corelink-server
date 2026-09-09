#!/usr/bin/env python3
"""Executable/static contract for the terminal activation-source purge lane."""

from __future__ import annotations

import re
import sqlite3
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "crates/corelink-container/src/storage/byok_generation_catalog.rs"
DRIVER = ROOT / "crates/corelink-container/src/storage/byok_purge_io.rs"
MAIN = ROOT / "crates/corelink-container/src/main.rs"
CAS_WRITE = ROOT / "crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs"
AC_WRITE = ROOT / "crates/corelink-container/src/storage/r2_s3_parts/ac_update.rs"
BACKFILL = ROOT / "crates/corelink-container/src/storage/byok_backfill_d1.rs"
MIGRATION = ROOT / "migrations/d1/0121_byok_activation_pipeline.sql"
QUARANTINE_MIGRATION = ROOT / "migrations/d1/0125_b083_common_purge_quarantine.sql"
R2_ABSENT_MIGRATION = ROOT / "migrations/d1/0126_b083_common_purge_r2_absent_quarantine.sql"


def rust_sql(source: str, needle: str) -> str:
    matches = re.findall(r'"([^"\n]*' + re.escape(needle) + r'[^"\n]*)"', source)
    if len(matches) != 1:
        raise AssertionError(f"expected one SQL literal containing {needle!r}, got {len(matches)}")
    return matches[0]


def main() -> None:
    catalog = CATALOG.read_text(encoding="utf-8")
    driver = DRIVER.read_text(encoding="utf-8")
    main = MAIN.read_text(encoding="utf-8")
    cas_write = CAS_WRITE.read_text(encoding="utf-8")
    ac_write = AC_WRITE.read_text(encoding="utf-8")
    backfill = BACKFILL.read_text(encoding="utf-8")
    migration = MIGRATION.read_text(encoding="utf-8")
    quarantine_migration = QUARANTINE_MIGRATION.read_text(encoding="utf-8")
    r2_absent_migration = R2_ABSENT_MIGRATION.read_text(encoding="utf-8")
    claim_sql = rust_sql(catalog, "p.reason='activation_source'")
    absent_sql = rust_sql(catalog, "SET state='r2_absent'")
    verified_sql = rust_sql(catalog, "SET state='verified'")

    db = sqlite3.connect(":memory:")
    db.row_factory = sqlite3.Row
    db.executescript(
        """
        CREATE TABLE byok_activation_intent (
          intent_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, phase TEXT NOT NULL
        );
        CREATE TABLE byok_object_purge_item (
          purge_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, intent_id TEXT,
          object_kind TEXT NOT NULL, logical_key TEXT NOT NULL,
          generation INTEGER NOT NULL, allocation_id TEXT, physical_key TEXT NOT NULL,
          object_size INTEGER, object_blake3 TEXT,
          crypto_mode TEXT NOT NULL, reason TEXT NOT NULL, state TEXT NOT NULL,
          claim_owner TEXT, claim_token TEXT, claim_epoch INTEGER NOT NULL DEFAULT 0,
          claim_expires_at_ms INTEGER, attempts INTEGER NOT NULL DEFAULT 0,
          next_attempt_at_ms INTEGER NOT NULL, last_error TEXT, verified_at_ms INTEGER
        );
        CREATE TABLE byok_object_purge_cause (
          purge_id TEXT, cause_kind TEXT, cause_id TEXT,
          created_at_ms INTEGER, UNIQUE(purge_id,cause_kind,cause_id)
        );
        CREATE TABLE byok_logical_object_generation (
          tenant_id TEXT, object_kind TEXT, logical_key TEXT, generation INTEGER,
          allocation_id TEXT, physical_key TEXT, allocated_at_ms INTEGER,
          intent_token TEXT, backfill_run_id TEXT, outcome TEXT
        );
        CREATE TABLE byok_data_intent (
          token TEXT, tenant_id TEXT, operation TEXT, outcome TEXT, expires_at_ms INTEGER
        );
        CREATE TABLE byok_backfill_run (
          tenant_id TEXT, run_id TEXT, intent_token TEXT, phase TEXT,
          transition_epoch INTEGER, target_generation INTEGER
        );
        CREATE TABLE byok_transition_fence (
          tenant_id TEXT, token TEXT, epoch INTEGER, outcome TEXT, expires_at_ms INTEGER
        );
        CREATE TABLE byok_activation_source_object (
          intent_id TEXT, tenant_id TEXT, object_kind TEXT, logical_key TEXT,
          target_allocation_id TEXT, target_physical_key TEXT,
          target_write_expires_at_ms INTEGER
        );
        """
    )
    tenant = "11111111-1111-4111-8111-111111111111"
    db.executemany(
        "INSERT INTO byok_activation_intent VALUES (?,?,?)",
        [("preempt", tenant, "preempted"), ("active", tenant, "purging")],
    )
    digest = "a" * 64
    rows = [
        ("source-preempt", "preempt", "activation_source", "pending"),
        ("source-active", "active", "activation_source", "pending"),
    ]
    for purge_id, intent_id, reason, state in rows:
        db.execute(
            """INSERT INTO byok_object_purge_item
               (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,
                allocation_id,physical_key,crypto_mode,reason,state,next_attempt_at_ms)
               VALUES (?,?,?,'cas',?,1,'allocation',?,'random',?,?,0)""",
            (purge_id, tenant, intent_id, f"blake3:{digest}", f"iad/prefix/key-{purge_id}", reason, state),
        )

    now_ms = db.execute("SELECT CAST(strftime('%s','now') AS INTEGER)*1000").fetchone()[0]
    db.execute(
        "INSERT INTO byok_data_intent VALUES ('late-put',?,'write','active',?)",
        (tenant, now_ms - 1),
    )
    db.execute(
        "INSERT INTO byok_logical_object_generation VALUES (?,'cas',?,2,'late-allocation','iad/prefix/late-key',?,'late-put',NULL,'allocated')",
        (tenant, f"blake3:{digest}", now_ms),
    )
    db.execute(
        """INSERT INTO byok_object_purge_item
           (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,
            allocation_id,physical_key,crypto_mode,reason,state,next_attempt_at_ms)
           VALUES ('late-loser',?,NULL,'cas',?,2,'late-allocation',
                   'iad/prefix/late-key','random','publish_loser','pending',0)""",
        (tenant, f"blake3:{digest}"),
    )
    db.execute("INSERT INTO byok_object_purge_cause VALUES ('late-loser','publish_loser','late-allocation',0)")
    trigger = re.search(
        r"CREATE TRIGGER IF NOT EXISTS trg_byok_publish_loser_requires_drained_owner.*?END;",
        migration,
        re.DOTALL,
    )
    assert trigger is not None
    db.executescript(trigger.group(0))
    db.execute(
        "UPDATE byok_data_intent SET expires_at_ms=? WHERE token='late-put'",
        (now_ms + 65_001,),
    )
    try:
        db.execute("UPDATE byok_object_purge_item SET state='deleting' WHERE purge_id='late-loser'")
    except sqlite3.IntegrityError as error:
        assert "owner lease has not drained" in str(error)
    else:
        raise AssertionError("live writer lease did not block loser deletion")
    db.execute(
        """INSERT INTO byok_object_purge_item
           (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,
            allocation_id,physical_key,crypto_mode,reason,state,next_attempt_at_ms)
           VALUES ('late-r2-absent',?,NULL,'cas',?,2,'late-allocation',
                   'iad/prefix/late-key','random','publish_loser','deleting',0)""",
        (tenant, f"blake3:{digest}"),
    )
    db.execute("INSERT INTO byok_object_purge_cause VALUES ('late-r2-absent','publish_loser','late-allocation',0)")
    try:
        db.execute(
            "UPDATE byok_object_purge_item SET state='r2_absent' WHERE purge_id='late-r2-absent'"
        )
    except sqlite3.IntegrityError as error:
        assert "owner lease has not drained" in str(error)
    else:
        raise AssertionError("live writer lease did not block r2_absent checkpoint")
    db.execute(
        "UPDATE byok_data_intent SET expires_at_ms=? WHERE token='late-put'",
        (now_ms - 1,),
    )

    claimed = db.execute(claim_sql, (120_000, 32, 5_000)).fetchall()
    assert [row["purge_id"] for row in claimed] == ["source-preempt"]
    claim = claimed[0]
    identity = (
        claim["purge_id"],
        claim["tenant_id"],
        claim["physical_key"],
        claim["claim_token"],
        claim["claim_epoch"],
    )

    # Executable exact-claim state progression after the statically asserted
    # DELETE -> HEAD-absent boundary and Mode-B allocation envelope reclaim.
    assert db.execute(absent_sql, identity).fetchone()["purge_id"] == "source-preempt"
    assert db.execute(
        "SELECT state FROM byok_object_purge_item WHERE purge_id='source-preempt'"
    ).fetchone()["state"] == "r2_absent"
    assert db.execute(verified_sql, identity).fetchone()["purge_id"] == "source-preempt"
    assert db.execute(
        "SELECT state FROM byok_object_purge_item WHERE purge_id='source-preempt'"
    ).fetchone()["state"] == "verified"
    assert db.execute(
        "SELECT state FROM byok_object_purge_item WHERE purge_id='source-active'"
    ).fetchone()["state"] == "pending"
    assert db.execute(
        "SELECT state FROM byok_object_purge_item WHERE purge_id='late-loser'"
    ).fetchone()["state"] == "pending", "expired lease alone must not race a late PUT"

    # Age/lease expiry alone still cannot race the late PUT: its durable
    # allocation remains non-terminal until the PUT return path classifies it.
    assert db.execute(claim_sql, (120_000, 32, 5_000)).fetchall() == []
    db.execute(
        "UPDATE byok_logical_object_generation SET outcome='abandoned' WHERE allocation_id='late-allocation'"
    )
    assert db.execute(claim_sql, (120_000, 32, 5_000)).fetchall() == []
    db.execute(
        "UPDATE byok_data_intent SET expires_at_ms=? WHERE token='late-put'",
        (now_ms - 5_001,),
    )
    late = db.execute(claim_sql, (120_000, 32, 5_000)).fetchall()
    assert [row["purge_id"] for row in late] == ["late-loser"]
    late_claim = late[0]
    late_identity = (
        late_claim["purge_id"],
        late_claim["tenant_id"],
        late_claim["physical_key"],
        late_claim["claim_token"],
        late_claim["claim_epoch"],
    )
    assert db.execute(absent_sql, late_identity).fetchone()["purge_id"] == "late-loser"
    # The static ordering below proves this checkpoint follows DELETE + HEAD
    # absence and precedes exact allocation-envelope reclaim.
    assert db.execute(verified_sql, late_identity).fetchone()["purge_id"] == "late-loser"

    process = driver[driver.index("async fn process_claim"):driver.index("async fn reclaim_envelope")]
    ordered = [
        "validate_physical_identity(plan)",
        "r2.delete(&plan.object.physical_key)",
        "r2.head_size(&plan.object.physical_key)",
        "finish(plan, true, None, false, None)",
        "reclaim_envelope(plan)",
        "finish(plan, true, None, true, None)",
    ]
    positions = [process.index(fragment) for fragment in ordered]
    assert positions == sorted(positions)
    run_page = driver[driver.index("pub async fn run_page"):driver.index("async fn process_claim")]
    assert run_page.index("reconcile_stale_page") < run_page.index("claim_purge_page")
    assert "STALE_CENSUS_TIMEOUT: Duration = Duration::from_millis(500)" in driver
    assert "allocation_envelope_blob_key" in driver
    assert '"plaintext" if plan.object.generation == 0 => Ok(())' in driver
    assert "MAX_PURGE_PAGE: u32 = 1" in driver
    assert "MAX_OPERATION_TIMEOUT: Duration = Duration::from_secs(15)" in driver
    assert "PURGE_DISPATCH_TAIL_MS: i64 = 5_000" in catalog
    assert "g.allocated_at_ms<=" not in claim_sql
    assert cas_write.index("validate_for_storage_dispatch()") < cas_write.index(
        "self.client.put_if_absent(&key, payload)"
    )
    assert ac_write.index("validate_for_storage_dispatch()") < ac_write.index(
        "self.client.put_if_absent(&key, payload)"
    )
    assert "R2_PUT_DISPATCH_HEADROOM_MS: i64 = 65_000" in catalog
    threshold = re.search(
        r"COMMON_PURGE_INVALID_IDENTITY_QUARANTINE_AFTER: i64 = (\d+);", catalog
    )
    assert threshold is not None and threshold.group(1) == "3"
    assert "record_invalid_identity(plan" in driver
    assert "validate_physical_identity(plan)" in driver
    assert "record_retry(plan, plan.r2_absent" not in driver
    assert "state=CASE WHEN attempts>=?6 THEN 'quarantined' ELSE CASE WHEN" in catalog
    assert "ELSE CASE WHEN state='r2_absent' THEN 'r2_absent' ELSE 'retry' END END" in catalog
    assert "p.state='r2_absent' AND p.claim_owner IS NULL AND p.claim_token IS NULL AND p.claim_expires_at_ms IS NULL" in catalog
    assert "quarantine_reason=CASE WHEN attempts>=?6 THEN ?7 ELSE NULL END" in catalog
    assert "quarantined_at_ms=CASE WHEN attempts>=?6 THEN" in catalog
    assert "pub quarantined: u32" in driver
    assert "ClaimOutcome::Quarantined => report.quarantined += 1" in driver
    assert "Ok(()) if plan.attempts >= COMMON_PURGE_INVALID_IDENTITY_QUARANTINE_AFTER" in driver
    assert "report.retryable == report.claimed" in main
    assert "ALTER TABLE byok_object_purge_item ADD COLUMN quarantine_reason TEXT" in quarantine_migration
    assert "ALTER TABLE byok_object_purge_item ADD COLUMN quarantined_at_ms INTEGER" in quarantine_migration
    assert "intentionally not presented as raw-SQL re-execution-safe" in quarantine_migration
    assert "trg_byok_purge_quarantine_evidence" in quarantine_migration
    assert "trg_byok_purge_quarantine_fields_forward_only" in quarantine_migration
    assert "DROP TRIGGER IF EXISTS trg_byok_object_purge_forward_only" in r2_absent_migration
    assert "OLD.state = 'r2_absent'" in r2_absent_migration
    assert "NEW.state NOT IN ('r2_absent', 'verified', 'quarantined')" in r2_absent_migration

    # Execute the exact invalid-identity completion statement against a small
    # D1-shaped ledger. Attempts 1 and 2 remain retryable, while attempt 3
    # becomes terminal and retains both the cause and detection timestamp.
    invalid_sql_match = re.search(
        r"async fn finish_invalid_identity_purge_attempt.*?\"(UPDATE byok_object_purge_item SET state=.*?RETURNING purge_id,state)\"",
        catalog,
        re.DOTALL,
    )
    assert invalid_sql_match is not None
    invalid_sql = invalid_sql_match.group(1)
    quarantine_db = sqlite3.connect(":memory:")
    quarantine_db.executescript(
        """
        CREATE TABLE byok_object_purge_item (
          purge_id TEXT PRIMARY KEY, tenant_id TEXT, intent_id TEXT,
          object_kind TEXT, logical_key TEXT, generation INTEGER,
          allocation_id TEXT, physical_key TEXT, object_size INTEGER,
          object_blake3 TEXT, crypto_mode TEXT, reason TEXT,
          state TEXT, claim_owner TEXT, claim_token TEXT, claim_epoch INTEGER,
          claim_expires_at_ms INTEGER, attempts INTEGER, next_attempt_at_ms INTEGER,
          last_error TEXT, verified_at_ms INTEGER
        );
        CREATE TRIGGER trg_byok_object_purge_forward_only
        BEFORE UPDATE ON byok_object_purge_item
        WHEN OLD.state = 'r2_absent' AND NEW.state NOT IN ('r2_absent', 'verified')
        BEGIN
          SELECT RAISE(ABORT, 'old trigger rejects r2_absent quarantine');
        END;
        """
        + quarantine_migration
        + r2_absent_migration
    )
    quarantine_db.execute(
        """INSERT INTO byok_object_purge_item
           (purge_id,tenant_id,physical_key,state,claim_owner,claim_token,
            claim_epoch,claim_expires_at_ms,attempts,next_attempt_at_ms)
           VALUES ('invalid','tenant','bad-key','deleting','data-plane','claim-1',
                   1,9223372036854775807,1,0)"""
    )
    params = ("invalid", "tenant", "bad-key", "claim-1", 1, 3, "identity mismatch")
    quarantine_db.execute(invalid_sql, params)
    first = quarantine_db.execute(
        "SELECT state,quarantine_reason,quarantined_at_ms FROM byok_object_purge_item"
    ).fetchone()
    assert first == ("retry", None, None)
    for attempt in (2, 3):
        quarantine_db.execute(
            """UPDATE byok_object_purge_item
               SET state='deleting', claim_owner='data-plane', claim_token=?,
                   claim_epoch=?, claim_expires_at_ms=9223372036854775807,
                   attempts=?""",
            (f"claim-{attempt}", 2 * attempt - 1, attempt),
        )
        quarantine_db.execute(
            invalid_sql,
            ("invalid", "tenant", "bad-key", f"claim-{attempt}", 2 * attempt - 1, 3, "identity mismatch"),
        )
    terminal = quarantine_db.execute(
        "SELECT state,quarantine_reason,quarantined_at_ms,verified_at_ms,claim_token "
        "FROM byok_object_purge_item"
    ).fetchone()
    assert terminal[0] == "quarantined"
    assert terminal[1] == "identity mismatch"
    assert terminal[2] is not None and terminal[3] is not None and terminal[4] is None

    # A prior HEAD-absence checkpoint is retained through attempts 1 and 2;
    # the 3rd invalid identity is the only transition to quarantine.
    quarantine_db.execute(
        """INSERT INTO byok_object_purge_item
           (purge_id,tenant_id,physical_key,state,claim_owner,claim_token,
            claim_epoch,claim_expires_at_ms,attempts,next_attempt_at_ms)
           VALUES ('absent','tenant','bad-absent','r2_absent','data-plane','absent-1',
                   1,9223372036854775807,1,0)"""
    )
    quarantine_db.execute(
        invalid_sql,
        ("absent", "tenant", "bad-absent", "absent-1", 1, 3, "identity mismatch"),
    )
    assert quarantine_db.execute(
        "SELECT state FROM byok_object_purge_item WHERE purge_id='absent'"
    ).fetchone() == ("r2_absent",)
    for attempt in (2, 3):
        quarantine_db.execute(
            """UPDATE byok_object_purge_item
               SET state='r2_absent', claim_owner='data-plane', claim_token=?,
                   claim_epoch=?, claim_expires_at_ms=9223372036854775807,
                   attempts=? WHERE purge_id='absent'""",
            (f"absent-{attempt}", 2 * attempt - 1, attempt),
        )
        quarantine_db.execute(
            invalid_sql,
            ("absent", "tenant", "bad-absent", f"absent-{attempt}", 2 * attempt - 1, 3, "identity mismatch"),
        )
    absent_terminal = quarantine_db.execute(
        "SELECT state,quarantine_reason,quarantined_at_ms FROM byok_object_purge_item WHERE purge_id='absent'"
    ).fetchone()
    assert absent_terminal[0] == "quarantined"
    assert absent_terminal[1] == "identity mismatch"
    assert absent_terminal[2] is not None
    # A transient completion remains retryable even when its attempt count is
    # already at the invalid-identity threshold; only the explicit invalid
    # identity channel may quarantine.
    quarantine_db.execute(
        """INSERT INTO byok_object_purge_item
           (purge_id,tenant_id,physical_key,state,claim_owner,claim_token,
            claim_epoch,claim_expires_at_ms,attempts,next_attempt_at_ms)
           VALUES ('transient','tenant','good-key','deleting','data-plane','transient-1',
                   1,9223372036854775807,3,0)"""
    )
    retry_sql = rust_sql(
        catalog, "state=CASE WHEN state='r2_absent' THEN 'r2_absent' ELSE 'retry' END"
    )
    quarantine_db.execute(
        retry_sql,
        ("transient", "tenant", "good-key", "transient-1", 1, "temporary R2 outage"),
    )
    assert quarantine_db.execute(
        "SELECT state,quarantine_reason,quarantined_at_ms FROM byok_object_purge_item WHERE purge_id='transient'"
    ).fetchone() == ("retry", None, None)

    # Drive the real claim scheduler, including the NULL-claim r2_absent
    # recovery path. A live r2_absent claim remains excluded until its lease
    # expires; once the invalid completion releases it, next_attempt_at_ms is
    # the fair bounded-queue gate for attempts 2 and 3.
    db.executescript(quarantine_migration + r2_absent_migration)
    db.execute(
        "UPDATE byok_object_purge_item SET next_attempt_at_ms=9223372036854775807 "
        "WHERE purge_id IN ('source-active','late-loser')"
    )
    db.execute(
        """INSERT INTO byok_object_purge_item
           (purge_id,tenant_id,object_kind,logical_key,generation,allocation_id,
            physical_key,crypto_mode,reason,state,next_attempt_at_ms)
           VALUES ('scheduler-absent',?,'cas','blake3:' || ?,1,'scheduler-allocation',
                   'bad-scheduler-key','random','live_delete','r2_absent',0)""",
        (tenant, digest),
    )
    for attempt in range(1, 4):
        claimed = db.execute(claim_sql, (120_000, 1, 5_000)).fetchall()
        assert len(claimed) == 1 and claimed[0]["purge_id"] == "scheduler-absent"
        claim = claimed[0]
        if attempt == 1:
            # The exact live claim is protected by its future lease.
            assert db.execute(claim_sql, (120_000, 1, 5_000)).fetchall() == []
        db.execute(
            invalid_sql,
            (
                claim["purge_id"],
                claim["tenant_id"],
                claim["physical_key"],
                claim["claim_token"],
                claim["claim_epoch"],
                3,
                "identity mismatch",
            ),
        )
        state = db.execute(
            "SELECT state FROM byok_object_purge_item WHERE purge_id='scheduler-absent'"
        ).fetchone()[0]
        assert state == ("quarantined" if attempt == 3 else "r2_absent")
    assert db.execute(
        "SELECT quarantine_reason FROM byok_object_purge_item WHERE purge_id='scheduler-absent'"
    ).fetchone()[0] == "identity mismatch"
    stage = backfill[backfill.index("async fn stage("):]
    assert stage.index("self.assert_put_headroom(run).await?") < stage.index(
        ".put_if_absent(&staged.target_physical_key"
    )
    assert "AND f.expires_at_ms > {NOW_MS} + ?8" in backfill
    storage = (ROOT / "crates/corelink-container/src/storage.rs").read_text(encoding="utf-8")
    retired_decl = "#[cfg(test)]\npub(crate) mod byok_backfill_d1;"
    assert retired_decl in storage
    assert "pub(crate) struct D1ByokBackfillStore" in backfill
    for source_path in (ROOT / "crates/corelink-container/src").rglob("*.rs"):
        if source_path.name == "byok_backfill_d1.rs" or source_path.name == "storage.rs":
            continue
        source = source_path.read_text(encoding="utf-8")
        assert "D1ByokBackfillStore" not in source, source_path
        assert "byok_backfill_d1" not in source, source_path

    dispatch_sql = rust_sql(catalog, "i.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000)+?3")
    dispatch_sql = dispatch_sql.replace(
        "(CAST(strftime('%s','now') AS INTEGER)*1000)", "1000000"
    )
    auth = sqlite3.connect(":memory:")
    auth.executescript(
        """
        CREATE TABLE byok_data_intent (
          tenant_id TEXT, token TEXT, outcome TEXT, expires_at_ms INTEGER,
          observed_gate_epoch INTEGER, observed_generation INTEGER,
          observed_config_version INTEGER, observed_config_state TEXT,
          observed_byok_status TEXT
        );
        CREATE TABLE byok_tenant_gate (
          tenant_id TEXT, gate_epoch INTEGER, current_generation INTEGER
        );
        CREATE TABLE tenant (tenant_id TEXT, byok_status TEXT);
        CREATE TABLE tenant_byok_config (
          tenant_id TEXT, config_version INTEGER, state TEXT
        );
        INSERT INTO byok_tenant_gate VALUES ('tenant',7,2);
        INSERT INTO tenant VALUES ('tenant','active');
        INSERT INTO tenant_byok_config VALUES ('tenant',4,'active');
        INSERT INTO byok_data_intent VALUES
          ('tenant','token','active',1065000,7,2,4,'active','active');
        """
    )
    assert auth.execute(dispatch_sql, ("tenant", "token", 65_000)).fetchall() == []
    auth.execute("UPDATE byok_data_intent SET expires_at_ms=1065001")
    assert auth.execute(dispatch_sql, ("tenant", "token", 65_000)).fetchall() == [(1,)]

    selected_match = re.search(
        r'const STALE_ALLOCATION_SELECTION: &str = "([^"]+LIMIT \?2)";', catalog
    )
    assert selected_match is not None
    selected = selected_match.group(1)
    census_start = catalog.index("let selected = STALE_ALLOCATION_SELECTION;")
    census_section = catalog[
        census_start:catalog.index("async fn claim_purge_page", census_start)
    ]
    templates = re.findall(r'format!\("([^"]*\{selected\}[^"]*)"\)', census_section)
    # The first formatted statement is the independently bounded collision
    # quarantine; the remaining statements use the exact selected page.
    assert len(templates) == 3
    statements = [template.replace("{selected}", selected) for template in templates]
    census = sqlite3.connect(":memory:")
    census.executescript(
        """
        CREATE TABLE byok_logical_object_generation (
          tenant_id TEXT, object_kind TEXT, logical_key TEXT, generation INTEGER,
          allocation_id TEXT, physical_key TEXT, allocated_at_ms INTEGER,
          intent_token TEXT, backfill_run_id TEXT, outcome TEXT, completed_at_ms INTEGER
        );
        CREATE TABLE byok_logical_object_publication (
          tenant_id TEXT, object_kind TEXT, logical_key TEXT, generation INTEGER,
          allocation_id TEXT, physical_key TEXT
        );
        CREATE TABLE tenant_byok_config (
          tenant_id TEXT PRIMARY KEY, config_version INTEGER, crypto_mode TEXT
        );
        CREATE TABLE tenant_byok_config_history (
          tenant_id TEXT, config_version INTEGER, crypto_mode TEXT,
          UNIQUE(tenant_id,config_version)
        );
        CREATE TABLE byok_data_intent (
          token TEXT, tenant_id TEXT, operation TEXT, outcome TEXT, expires_at_ms INTEGER,
          observed_config_version INTEGER
        );
        CREATE TABLE byok_backfill_run (
          tenant_id TEXT, run_id TEXT, intent_token TEXT, phase TEXT,
          transition_epoch INTEGER, target_generation INTEGER
        );
        CREATE TABLE byok_transition_fence (
          tenant_id TEXT, token TEXT, epoch INTEGER, outcome TEXT, expires_at_ms INTEGER,
          observed_config_version INTEGER
        );
        CREATE TABLE byok_object_purge_item (
          purge_id TEXT PRIMARY KEY, tenant_id TEXT, intent_id TEXT, object_kind TEXT,
          logical_key TEXT, generation INTEGER, allocation_id TEXT, physical_key TEXT,
          crypto_mode TEXT, reason TEXT, state TEXT, next_attempt_at_ms INTEGER,
          created_at_ms INTEGER, UNIQUE(tenant_id,physical_key)
        );
        CREATE TABLE byok_object_purge_cause (
          purge_id TEXT, cause_kind TEXT, cause_id TEXT, created_at_ms INTEGER,
          UNIQUE(purge_id,cause_kind,cause_id)
        );
        INSERT INTO tenant_byok_config VALUES ('tenant',8,'random');
        """
    )
    for index in range(5):
        token = f"done-{index}"
        census.execute(
            "INSERT INTO byok_data_intent VALUES (?, 'tenant','write','completed',0,8)",
            (token,),
        )
        census.execute(
            "INSERT INTO byok_logical_object_generation VALUES ('tenant','cas',?,2,?,?,0,?,NULL,'allocated',NULL)",
            (f"blake3:{index:064x}", f"allocation-{index}", f"physical-{index}", token),
        )
    for statement in statements:
        census.execute(statement, (1, 2))
    assert census.execute(
        "SELECT COUNT(*) FROM byok_logical_object_generation WHERE outcome='abandoned'"
    ).fetchone()[0] == 2
    assert census.execute("SELECT COUNT(*) FROM byok_object_purge_item").fetchone()[0] == 2
    assert census.execute("SELECT COUNT(*) FROM byok_object_purge_cause").fetchone()[0] == 2
    assert all("LIMIT ?2" in statement for statement in statements)
    assert "g.outcome IN ('allocated','abandoned')" in selected
    assert "tenant_byok_config_history" in selected
    assert "HAVING COUNT(*)=1" in selected
    assert "p.reason='publish_loser'" not in statements[1]
    assert "pc.cause_kind='publish_loser'" in catalog
    assert "STALE_CENSUS_TIMEOUT: Duration = Duration::from_millis(500)" in catalog
    gate_impl = catalog.index("impl ByokRuntimeGate for D1ByokRuntimeGate")
    acquire_start = catalog.index("async fn acquire_data(", gate_impl)
    acquire = catalog[
        acquire_start:catalog.index("async fn release_data(", acquire_start)
    ]
    assert acquire.index("tokio::time::timeout(") < acquire.index("acquire_data_intent")
    print("B-083 common purge terminal-owner and exact-I/O contract: PASS")


if __name__ == "__main__":
    main()
