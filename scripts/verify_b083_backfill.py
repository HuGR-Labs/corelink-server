#!/usr/bin/env python3
"""Hermetic executable checks for the B-083 generation backfill protocol.

This proves checked-in migration/runtime invariants only. It does not claim a
live KMS, D1, or R2 execution.
"""

from pathlib import Path
import sqlite3
import time


ROOT = Path(__file__).resolve().parents[1]


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle}")


def expect_integrity_error(
    connection: sqlite3.Connection, sql: str, params: tuple = ()
) -> None:
    try:
        connection.execute(sql, params)
    except sqlite3.IntegrityError:
        return
    raise AssertionError(f"statement unexpectedly succeeded: {sql}")


def require_retired_target_history(connection: sqlite3.Connection) -> None:
    row = connection.execute(
        """SELECT tcs_wrapped,retired_at_ms
        FROM tenant_byok_secret_history
        WHERE tenant_id='tenant-e' AND tcs_version=2
          AND cmk_provider='aws' AND cmk_key_id='new-key' AND cmk_region='us-east-1'"""
    ).fetchone()
    if row is None or row[0] is not None or row[1] is None:
        raise AssertionError(f"aborted target TCS history is not retired: {row}")


def exercise_source_degrade_guard(connection: sqlite3.Connection, suffix: str) -> None:
    """Execute the real 0119 guard and 0121 postcondition for source CMK A."""
    fence = f"source-control-{suffix}"
    version = connection.execute(
        "SELECT state_version FROM byok_activation_intent WHERE intent_id='activation-e'"
    ).fetchone()[0]
    gate, generation = connection.execute(
        "SELECT gate_epoch,current_generation FROM byok_tenant_gate WHERE tenant_id='tenant-e'"
    ).fetchone()
    config_version, config_state = connection.execute(
        "SELECT config_version,state FROM tenant_byok_config WHERE tenant_id='tenant-e'"
    ).fetchone()
    epoch = connection.execute(
        "SELECT COALESCE(MAX(epoch),0)+1 FROM byok_transition_fence WHERE tenant_id='tenant-e'"
    ).fetchone()[0]
    connection.execute(
        """INSERT OR IGNORE INTO byok_activation_key_health
        VALUES ('activation-e','tenant-e','target','aws','new-key','us-east-1','healthy',1),
               ('activation-e','tenant-e','source','aws','old-key','us-east-1','healthy',1)"""
    )
    connection.execute(
        "UPDATE byok_activation_key_health SET access_state='unavailable' "
        "WHERE intent_id='activation-e' AND dependency_role='source'"
    )
    connection.execute(
        "UPDATE byok_activation_guard SET outcome='expired',completed_at_ms=2 WHERE guard_id='guard-e'"
    )
    connection.execute(
        "UPDATE byok_transition_fence SET outcome='aborted',completed_at_ms=2 WHERE token='token-e'"
    )
    connection.execute(
        """INSERT INTO byok_transition_fence VALUES
        (?,'tenant-e',?,'active',?,?,?,?,'active',1,9999999999999,NULL)""",
        (fence, epoch, gate, generation, config_version, config_state),
    )
    connection.execute(
        """INSERT INTO byok_transition_commit_guard
        VALUES (?,'tenant-e',?,'degrade','aws','old-key','us-east-1')""",
        (fence, epoch),
    )
    connection.execute(
        """UPDATE tenant SET byok_status='degraded_read_only',
        byok_revoked_provider='aws',byok_revoked_kms_key_id='old-key'
        WHERE tenant_id='tenant-e'"""
    )
    connection.execute(
        """UPDATE byok_activation_intent SET suspension_state='degraded_read_only',
        suspended_at_ms=2,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL,
        claim_epoch=claim_epoch+1,state_version=state_version+1
        WHERE intent_id='activation-e'"""
    )
    connection.execute("UPDATE byok_tenant_gate SET gate_epoch=gate_epoch+1 WHERE tenant_id='tenant-e'")
    connection.execute(
        "UPDATE byok_transition_fence SET outcome='committed',completed_at_ms=2 WHERE token=?",
        (fence,),
    )
    connection.execute(
        """INSERT INTO byok_control_outcome VALUES
        (?,'tenant-e',?,'degrade','aws','old-key','us-east-1','completed',
         'tenant-e','customer_activity_recorded',2)""",
        (fence, epoch),
    )
    connection.execute("DELETE FROM byok_transition_commit_guard WHERE token=?", (fence,))
    connection.execute(
        """INSERT INTO byok_activation_suspension_postcondition VALUES
        (?,'activation-e',?,'degrade',2)""",
        (f"source-post-{suffix}", version + 1),
    )


def verify_runtime_contract() -> None:
    engine = (ROOT / "crates/corelink-container/src/storage/byok_backfill.rs").read_text()
    activation = (ROOT / "crates/corelink-container/src/storage/byok_activation.rs").read_text()
    control = (ROOT / "crates/corelink-container/src/byok_control_transition.rs").read_text()
    activation_d1 = (
        ROOT / "crates/corelink-container/src/storage/byok_activation_d1.rs"
    ).read_text()
    tests = (
        ROOT / "crates/corelink-container/src/storage/byok_backfill/tests.rs"
    ).read_text()
    for needle, label in (
        ("trait ByokBackfillStore", "typed store boundary"),
        ("async fn enumerate", "CAS/AC enumeration"),
        ("async fn stage", "generation staging"),
        ("async fn checkpoint", "atomic catalog checkpoint"),
        ("async fn commit_generation", "guarded generation commit"),
        ("MAX_BYOK_FENCE_LEASE", "bounded fence lease"),
        ("BackfillAllocation", "ciphertext-free D1 receipt"),
        ("allocation_id: &str", "Mode-B idempotent encryption identity"),
        ("target_physical_key", "tenant-scoped R2 identity boundary"),
        ("generation_qualified_suffix", "generation-qualified R2 suffix"),
    ):
        require(engine, needle, label)
    if "pub ciphertext: Vec<u8>" in engine.split("pub struct BackfillCheckpoint", 1)[1].split(
        "pub struct BeginBackfill", 1
    )[0]:
        raise AssertionError("checkpoint must not carry ciphertext into D1")
    require(tests, "tokio::sync::Barrier::new(2)", "stale PUT barrier")
    require(tests, "crash_after_r2_stage_resumes", "crash recovery test")
    require(tests, "cas_and_ac_stay_invisible", "atomic visibility test")
    require(activation, "require_0121_capability", "fail-closed rollout probe")
    for needle, label in (
        ("INSERT INTO byok_activation_intent", "atomic admin activation intent"),
        ("INSERT INTO byok_activation_postcondition", "admin rollback assertion"),
        ("INSERT INTO byok_activation_operation_guard", "authorized destructive preempt"),
        ("tenant_byok_secret_history SET tcs_wrapped=NULL", "historical secret shred"),
        ("phase='preempted'", "activation terminal preemption"),
        ("a.phase='committed' AND c.state='active'", "committed request idempotency"),
        ("Aborted work is intentionally retryable", "explicit aborted retry policy"),
        ("preempted work", "explicit preempted retry policy"),
        ("outcome='committed'", "0118 committed transition outcome"),
        ("corelink.byok.activation-request.v1", "domain-separated request hash"),
        ("corelink.byok.activation-policy.v1", "domain-separated policy hash"),
        ("(CAST(strftime('%s','now') AS INTEGER)*1000)+?7", "D1 activation deadline"),
        ("INSERT INTO byok_activation_source_capture", "pre-overwrite source capture"),
        ("JOIN byok_activation_source_capture src", "intent copies exact source capture"),
        ("DELETE FROM byok_activation_source_capture", "postcondition source-capture cleanup"),
    ):
        require(control, needle, label)
    for text, label in (
        (control, "control preempt target cause"),
        (activation_d1, "activation abort/preempt target cause"),
    ):
        require(
            text,
            "SELECT p.purge_id,'publish_loser',p.allocation_id",
            label,
        )
    prepare_body = control.split("async fn commit_activation_preparation", 1)[1].split(
        "async fn activation_request", 1
    )[0]
    if "deadline_at_ms" not in prepare_body or "json!(now_ms)" in prepare_body.split(
        "INSERT INTO byok_activation_intent", 1
    )[1].split("));", 1)[0]:
        raise AssertionError("activation intent timestamps must use SQLite NOW")

    adapter = (
        ROOT / "crates/corelink-container/src/storage/byok_backfill_d1.rs"
    ).read_text()
    crypto = (
        ROOT / "crates/corelink-container/src/storage/byok_backfill_crypto.rs"
    ).read_text()
    mode_b = (
        ROOT / "crates/corelink-container/src/storage/byok_cas/part-01.rs"
    ).read_text()
    mode_a = (
        ROOT / "crates/corelink-container/src/storage/byok_cas/part-00.rs"
    ).read_text()
    config_reader = (
        ROOT / "crates/corelink-container/src/customer_d1_byok_config.rs"
    ).read_text()
    ac_handler = (
        ROOT / "crates/corelink-container/src/storage/r2_s3_parts/ac_handler.rs"
    ).read_text()
    ac_update = (
        ROOT / "crates/corelink-container/src/storage/r2_s3_parts/ac_update.rs"
    ).read_text()
    ac_ops = (
        ROOT / "crates/corelink-container/src/storage/r2_s3_parts/ac_ops.rs"
    ).read_text()
    for text, needle, label in (
        (crypto, '"blake3" => DigestAlgo::Blake3', "BLAKE3 identity parser"),
        (crypto, '"sha256" => DigestAlgo::Sha256', "SHA-256 identity parser"),
        (crypto, "encrypt_for_allocation", "allocation-qualified Mode B"),
        (mode_b, "decrypt_for_allocation", "allocation-qualified Mode-B read"),
        (adapter, "gate_epoch,size_bytes,outcome", "generation plaintext size"),
        (adapter, "generation,allocation_id,physical_key,gate_epoch,size_bytes", "publication identity"),
        (adapter, "p.allocation_id<>x.allocation_id", "allocation publication proof"),
        (adapter, "p.size_bytes<>x.size_bytes", "size publication proof"),
        (adapter, "outcome='committed'", "valid committed fence outcome"),
        (adapter, "outcome='aborted'", "valid aborted fence outcome"),
        (adapter, "SET state='inactive'", "aborted activation fail-closed reset"),
        (activation_d1, "abort_config_state(intent.source_generation, preempt)",
         "source-aware abort state selection"),
        (activation_d1, "tenant_byok_secret_history", "rotated abort TCS restoration"),
        (activation_d1, "current_generation=?12", "rotated abort generation CAS"),
        (activation_d1, "abort_target_secret_history_statement", "aborted target TCS cleanup"),
        (activation_d1, "AND ?6=0", "normal-only target TCS cleanup"),
        (activation_d1, "a.intent_id=?7", "target TCS intended identity"),
        (ac_handler, "fn generation_r2_key", "component-safe AC generation key"),
        (ac_update, ".generation_r2_key(", "live AC generation key callsite"),
        (ac_ops, "published.allocation_id", "AC allocation-aware decrypt"),
        (config_reader, "tenant_byok_config_history", "versioned source config history"),
        (config_reader, "WHERE tenant_id = ?1 AND config_version = ?2", "exact config version predicate"),
        (mode_a, "tenant_byok_secret_history", "versioned source TCS history"),
    ):
        require(text, needle, label)
    if ".map(|published| published.physical_key)" in ac_ops:
        raise AssertionError("AC lookup must preserve the full PublishedObject")


def verify_schema() -> None:
    migration_118 = (ROOT / "migrations/d1/0118_byok_transition_fence.sql").read_text()
    migration_119 = (ROOT / "migrations/d1/0119_byok_control_transition_guard.sql").read_text()
    migration_120 = (ROOT / "migrations/d1/0120_byok_backfill_run.sql").read_text()
    migration_121 = (ROOT / "migrations/d1/0121_byok_activation_pipeline.sql").read_text()
    for needle, label in (
        ("idx_byok_activation_live_tenant", "one live activation per tenant"),
        ("byok_activation_operation_guard", "claim/config/clock assertion row"),
        ("publication_gate_epoch", "post-publication gate snapshot"),
        ("source_blake3", "exact retry source fingerprint"),
        ("source_tcs_version", "rotation crypto identity"),
        ("byok_object_purge_cause", "deduplicated multi-cause purge ledger"),
        ("trg_byok_activation_suspension_guard", "0119 degrade/restore pause guard"),
        ("trg_byok_activation_suspension_postcondition", "atomic suspension assertion"),
        ("OLD.phase = 'copy'", "copy restore gate resnapshot"),
        ("OLD.phase IN ('published_partial','purging','ready_finalize')", "partial/purge restore gate resnapshot"),
        ("byok_0121_capability", "0121 completion marker"),
        ("tenant_byok_secret_history", "versioned wrapped-secret history"),
        ("tenant_byok_config_history", "versioned policy history"),
    ):
        require(migration_121, needle, label)
    require(
        migration_119,
        "c.state IN ('pending', 'active', 'partial')",
        "pending shred authorization",
    )
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    connection.executescript(
        """
        CREATE TABLE tenant (
          tenant_id TEXT PRIMARY KEY,
          byok_revoked_provider TEXT,
          byok_revoked_kms_key_id TEXT,
          byok_status TEXT NOT NULL DEFAULT 'active'
            CHECK (byok_status IN ('active','degraded_read_only','revoked'))
        );
        CREATE TABLE tenant_byok_config (
          tenant_id TEXT PRIMARY KEY,
          mode TEXT NOT NULL DEFAULT 'byok',
          crypto_mode TEXT NOT NULL DEFAULT 'convergent',
          cmk_provider TEXT,
          cmk_key_id TEXT,
          cmk_region TEXT,
          state TEXT NOT NULL CHECK (state IN ('inactive','pending','active','partial','shredded'))
        );
        CREATE TABLE tenant_byok_secret (
          tenant_id TEXT PRIMARY KEY, tcs_wrapped BLOB, cmk_key_id TEXT,
          tcs_version INTEGER NOT NULL DEFAULT 1, wrapped_at_ms INTEGER
        );
        CREATE TABLE blob_meta (
          tenant_id TEXT NOT NULL, digest TEXT NOT NULL, deleted_at INTEGER,
          PRIMARY KEY (tenant_id,digest)
        );
        CREATE TABLE ac_meta (
          tenant_id TEXT NOT NULL, action_digest TEXT NOT NULL, expires_at INTEGER,
          PRIMARY KEY (tenant_id,action_digest)
        );
        INSERT INTO tenant (tenant_id,byok_status) VALUES ('tenant-a', 'active');
        INSERT INTO tenant_byok_config
          (tenant_id,cmk_provider,cmk_key_id,cmk_region,state)
          VALUES ('tenant-a','aws','key-a','us-east-1','pending');
        INSERT INTO tenant_byok_secret VALUES ('tenant-a',x'01','key-a',1,1);
        """
    )
    connection.executescript(migration_118)
    connection.executescript(migration_119)
    connection.executescript(migration_120)
    connection.executescript(migration_121)
    marker = connection.execute(
        "SELECT singleton,schema_version FROM byok_0121_capability"
    ).fetchone()
    if marker != (1, 2):
        raise AssertionError("0121 completion marker is absent or malformed")
    connection.execute(
        """INSERT INTO byok_object_purge_item
        (purge_id,tenant_id,object_kind,logical_key,generation,allocation_id,
         physical_key,crypto_mode,reason,next_attempt_at_ms,created_at_ms)
        VALUES ('target-loser','tenant-a','cas',?1,2,'allocation-exact',
                'iad/prefix/generation/2/allocation-exact/digest','random',
                'publish_loser',1,1)""",
        (f"blake3:{'a' * 64}",),
    )
    connection.execute(
        """INSERT INTO byok_object_purge_cause
        (purge_id,cause_kind,cause_id,created_at_ms)
        SELECT purge_id,'publish_loser',allocation_id,1
        FROM byok_object_purge_item WHERE purge_id='target-loser'"""
    )
    cause = connection.execute(
        "SELECT cause_id FROM byok_object_purge_cause WHERE purge_id='target-loser'"
    ).fetchone()
    if cause != ("allocation-exact",):
        raise AssertionError("activation target publish_loser cause is not allocation-qualified")
    partial = sqlite3.connect(":memory:")
    partial.execute("CREATE TABLE byok_activation_intent(intent_id TEXT PRIMARY KEY)")
    try:
        partial.execute(
            "SELECT singleton FROM byok_0121_capability WHERE singleton=1 AND schema_version=2"
        )
    except sqlite3.OperationalError:
        pass
    else:
        raise AssertionError("partial 0121 schema unexpectedly passed completion probe")
    connection.execute(
        "UPDATE tenant_byok_config SET config_version=2 WHERE tenant_id='tenant-a'"
    )
    connection.execute(
        "UPDATE tenant_byok_config SET config_version=1 WHERE tenant_id='tenant-a'"
    )
    connection.execute(
        """UPDATE tenant_byok_secret SET tcs_wrapped=x'02',tcs_version=2
        WHERE tenant_id='tenant-a'"""
    )
    archived = connection.execute(
        """SELECT tcs_version,cmk_provider,cmk_key_id,cmk_region,tcs_wrapped
        FROM tenant_byok_secret_history WHERE tenant_id='tenant-a' AND tcs_version=1"""
    ).fetchone()
    if archived != (1, "aws", "key-a", "us-east-1", b"\x01"):
        raise AssertionError("source wrapped-secret identity was not archived exactly")
    archived_config = connection.execute(
        """SELECT config_version,mode,crypto_mode,cmk_provider,cmk_key_id,cmk_region,state
        FROM tenant_byok_config_history WHERE tenant_id='tenant-a' AND config_version=1"""
    ).fetchone()
    if archived_config != (1, "byok", "convergent", "aws", "key-a", "us-east-1", "pending"):
        raise AssertionError("source policy identity was not archived exactly")
    connection.execute(
        "UPDATE tenant_byok_secret SET tcs_wrapped=x'01',tcs_version=1 WHERE tenant_id='tenant-a'"
    )
    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('token-a','tenant-a',1,1,0,1,'pending','active',1,9999999999999)"""
    )
    connection.execute(
        """INSERT INTO byok_backfill_run
        (tenant_id,run_id,intent_token,transition_epoch,gate_epoch,
         source_generation,target_generation,started_at_ms,checkpointed_at_ms)
        VALUES ('tenant-a','run-a','token-a',1,1,0,1,1,1)"""
    )
    expect_integrity_error(
        connection,
        "UPDATE byok_backfill_run SET phase='ready_to_commit' WHERE tenant_id='tenant-a'",
    )
    expect_integrity_error(
        connection,
        "UPDATE byok_backfill_run SET gate_epoch=2 WHERE tenant_id='tenant-a'",
    )
    connection.execute(
        """UPDATE byok_backfill_run
        SET cas_complete=1, ac_complete=1, phase='ready_to_commit'
        WHERE tenant_id='tenant-a'"""
    )
    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('token-new','tenant-a',2,1,0,1,'pending','active',2,9999999999999)"""
    )
    expect_integrity_error(
        connection,
        """UPDATE byok_backfill_run
        SET intent_token='token-new', transition_epoch=2
        WHERE tenant_id='tenant-a'""",
    )
    connection.execute(
        "UPDATE byok_transition_fence SET outcome='expired' WHERE token='token-a'"
    )
    expect_integrity_error(
        connection,
        "UPDATE byok_transition_fence SET outcome='completed' WHERE token='token-new'",
    )
    connection.execute(
        """UPDATE byok_backfill_run
        SET intent_token='token-new', transition_epoch=2
        WHERE tenant_id='tenant-a'"""
    )
    expect_integrity_error(
        connection,
        """INSERT INTO byok_transition_commit_guard
        (token,tenant_id,epoch,action) VALUES ('stale-token','tenant-a',2,'shred')""",
    )
    connection.execute(
        """INSERT INTO byok_transition_commit_guard
        (token,tenant_id,epoch,action) VALUES ('token-new','tenant-a',2,'shred')"""
    )
    expect_integrity_error(
        connection,
        """INSERT INTO byok_backfill_run
        (tenant_id,run_id,intent_token,transition_epoch,gate_epoch,
         source_generation,target_generation,started_at_ms,checkpointed_at_ms)
        VALUES ('tenant-a','run-b','token-new',2,1,1,2,1,1)""",
    )
    phase, run_id, token = connection.execute(
        "SELECT phase,run_id,intent_token FROM byok_backfill_run WHERE tenant_id='tenant-a'"
    ).fetchone()
    if phase != "ready_to_commit":
        raise AssertionError(f"unexpected durable phase: {phase}")
    if (run_id, token) != ("run-a", "token-new"):
        raise AssertionError("expired takeover changed run identity or failed to rotate token")

    connection.execute(
        """INSERT INTO byok_activation_guard
        (guard_id,tenant_id,transition_token,transition_epoch,capability_token,
         epoch,action,priority,acquired_at_ms,expires_at_ms)
        VALUES ('guard-a','tenant-a','token-new',2,'cap-a',1,'activate',10,1,9999999999999)"""
    )
    expect_integrity_error(
        connection,
        """INSERT INTO byok_data_intent
        (token,tenant_id,operation,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('blocked-read','tenant-a','read',1,0,1,'pending','active',1,9999999999999)""",
    )
    activation_insert = """INSERT INTO byok_activation_intent
        (intent_id,tenant_id,guard_id,request_blake3,config_version,mode,crypto_mode,
         cmk_provider,cmk_key_id,cmk_region,policy_blake3,tcs_version,wrapped_tcs_blake3,
         source_generation,target_generation,observed_gate_epoch,claim_owner,claim_token,
         claim_epoch,claim_expires_at_ms,deadline_at_ms,created_at_ms,checkpointed_at_ms)
        VALUES (?,?,?,?,1,'byok','convergent','aws','key-a','us-east-1',?,1,?,
                0,1,1,'worker-a',?,1,9999999999999,9999999999999,1,1)"""
    h = "a" * 64
    connection.execute(activation_insert, ("activation-a", "tenant-a", "guard-a", h, h, h, "claim-a"))
    connection.execute(
        """INSERT INTO byok_activation_guard
        (guard_id,tenant_id,transition_token,transition_epoch,capability_token,epoch,action,priority,outcome,
         acquired_at_ms,expires_at_ms,completed_at_ms)
        VALUES ('guard-b','tenant-a','token-new',2,'cap-b',2,'activate',10,'aborted',1,2,2)"""
    )
    expect_integrity_error(
        connection,
        """INSERT INTO byok_activation_intent
        (intent_id,tenant_id,guard_id,request_blake3,config_version,mode,crypto_mode,
         cmk_provider,cmk_key_id,cmk_region,policy_blake3,tcs_version,wrapped_tcs_blake3,
         source_generation,target_generation,observed_gate_epoch,claim_owner,claim_token,
         claim_epoch,claim_expires_at_ms,deadline_at_ms,created_at_ms,checkpointed_at_ms)
        VALUES ('activation-b','tenant-a','guard-b','bbbb',1,'byok','convergent',
                'aws','key-a','us-east-1','bbbb',1,'bbbb',0,1,1,
                'worker-b','claim-b',1,9999999999999,9999999999999,1,1)""",
    )
    connection.execute(
        """UPDATE byok_activation_intent SET phase='aborted',failure_reason='deadline',
        completed_at_ms=2,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL,
        state_version=state_version+1 WHERE intent_id='activation-a'"""
    )
    outcome = connection.execute(
        "SELECT outcome FROM byok_activation_guard WHERE guard_id='guard-a'"
    ).fetchone()[0]
    if outcome != "aborted":
        raise AssertionError("terminal activation did not classify its guard")
    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('token-c','tenant-a',3,1,0,1,'pending','active',3,9999999999999)"""
    )
    connection.execute(
        """INSERT INTO byok_activation_guard
        (guard_id,tenant_id,transition_token,transition_epoch,capability_token,
         epoch,action,priority,acquired_at_ms,expires_at_ms)
        VALUES ('guard-c','tenant-a','token-c',3,'cap-c',3,'activate',10,1,9999999999999)"""
    )
    expect_integrity_error(
        connection,
        activation_insert,
        ("activation-upper", "tenant-a", "guard-c", "A" * 64, h, h, "claim-upper"),
    )
    connection.execute(
        activation_insert,
        ("activation-b", "tenant-a", "guard-c", "b" * 64, h, h, "claim-b"),
    )
    expect_integrity_error(
        connection,
        """UPDATE byok_activation_intent SET suspension_state='degraded_read_only',
        suspended_at_ms=2,state_version=state_version+1 WHERE intent_id='activation-b'""",
    )
    expect_integrity_error(
        connection,
        """INSERT INTO byok_activation_source_object
        (intent_id,tenant_id,object_kind,logical_key,source_generation,
         source_physical_key,source_crypto_mode,plaintext_size,source_stored_size,
         source_blake3,discovered_at_ms)
        VALUES ('activation-b','tenant-a','ac','bbbb',0,'raw-key','plaintext',
                1,1,?,1)""",
        ("A" * 64,),
    )
    connection.execute(
        """UPDATE byok_activation_intent SET cas_complete=1,ac_complete=1,
        phase='published_partial',publication_gate_epoch=2,published_config_version=2,
        state_version=state_version+1
        WHERE intent_id='activation-b'"""
    )
    connection.execute(
        "UPDATE byok_tenant_gate SET gate_epoch=2,current_generation=1 WHERE tenant_id='tenant-a'"
    )
    connection.execute(
        "UPDATE tenant_byok_config SET state='partial',config_version=2 WHERE tenant_id='tenant-a'"
    )
    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('token-post','tenant-a',4,2,1,2,'partial','active',
                (CAST(strftime('%s','now') AS INTEGER)*1000),9999999999999)"""
    )
    connection.execute(
        """UPDATE byok_activation_guard SET transition_token='token-post',transition_epoch=4,
        capability_token='cap-post',epoch=4,
        acquired_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000),expires_at_ms=9999999999999
        WHERE guard_id='guard-c'"""
    )
    connection.execute(
        """INSERT INTO byok_activation_operation_guard
        (operation_token,intent_id,claim_token,expected_state_version,action,checked_at_ms)
        VALUES ('op-post','activation-b','claim-b',2,'begin_purge',1)"""
    )
    expect_integrity_error(
        connection,
        """UPDATE byok_activation_intent SET phase='aborted',failure_reason='late',
        completed_at_ms=3,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL,
        state_version=state_version+1 WHERE intent_id='activation-b'""",
    )
    expect_integrity_error(
        connection,
        """INSERT INTO byok_activation_source_object
        (intent_id,tenant_id,object_kind,logical_key,source_generation,
         source_allocation_id,source_physical_key,source_crypto_mode,
         plaintext_size,source_stored_size,source_blake3,discovered_at_ms)
        VALUES ('activation-b','tenant-a','cas','blake3:bad',1,'alloc','key',
                'random',1,2,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',1)""",
    )
    connection.execute(
        """INSERT INTO byok_object_purge_item
        (purge_id,tenant_id,object_kind,logical_key,generation,allocation_id,
         physical_key,object_size,object_blake3,crypto_mode,reason,next_attempt_at_ms,created_at_ms)
        VALUES ('purge-a','tenant-a','cas','blake3:x',0,NULL,'raw-key',
                1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                'plaintext','activation_source',1,1)"""
    )
    connection.execute(
        """UPDATE byok_object_purge_item SET state='deleting',claim_owner='w',
        claim_token='pc',claim_epoch=1,claim_expires_at_ms=9999999999999,
        attempts=1 WHERE purge_id='purge-a'"""
    )
    expect_integrity_error(
        connection,
        "UPDATE byok_object_purge_item SET attempts=0 WHERE purge_id='purge-a'",
    )

    # A destructive preempt owns a distinct control fence. If the activation's
    # exact predecessor already expired, terminalization still preempts its
    # guard while preserving the truthful `expired` fence outcome.
    connection.execute(
        "UPDATE byok_transition_fence SET outcome='expired',completed_at_ms=expires_at_ms "
        "WHERE token='token-post'"
    )
    connection.execute(
        "UPDATE byok_activation_guard SET outcome='expired',completed_at_ms=expires_at_ms "
        "WHERE guard_id='guard-c'"
    )
    connection.execute(
        """UPDATE byok_activation_intent SET phase='preempted',
        failure_reason='operator_crypto_shred',cmk_provider=NULL,cmk_key_id=NULL,cmk_region=NULL,
        completed_at_ms=4,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL,
        state_version=state_version+1,checkpointed_at_ms=4 WHERE intent_id='activation-b'"""
    )
    terminal_guard, terminal_fence = connection.execute(
        """SELECT g.outcome,f.outcome FROM byok_activation_guard g
        JOIN byok_transition_fence f ON f.token=g.transition_token
        WHERE g.guard_id='guard-c'"""
    ).fetchone()
    if (terminal_guard, terminal_fence) != ("preempted", "expired"):
        raise AssertionError("expired activation fence was rebound or misclassified by preempt")

    # Copy remains a tenant-wide barrier even after both leases expire. No CAS
    # or AC operation may race discovery before an exact cancel/takeover.
    connection.execute("INSERT INTO tenant (tenant_id,byok_status) VALUES ('tenant-d','active')")
    connection.execute(
        """INSERT INTO tenant_byok_config
        (tenant_id,mode,crypto_mode,cmk_provider,cmk_key_id,cmk_region,state,config_version)
        VALUES ('tenant-d','byok','convergent','aws','key-d','us-east-1','pending',1)"""
    )
    connection.execute(
        "INSERT INTO tenant_byok_secret VALUES ('tenant-d',x'01','key-d',1,1)"
    )
    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('token-d','tenant-d',1,1,0,1,'pending','active',1,9999999999999)"""
    )
    connection.execute(
        """INSERT INTO byok_activation_guard
        (guard_id,tenant_id,transition_token,transition_epoch,capability_token,
         epoch,action,priority,acquired_at_ms,expires_at_ms)
        VALUES ('guard-d','tenant-d','token-d',1,'cap-d',1,'activate',10,1,9999999999999)"""
    )
    connection.execute(
        """INSERT INTO byok_activation_intent
        (intent_id,tenant_id,guard_id,request_blake3,config_version,mode,crypto_mode,
         cmk_provider,cmk_key_id,cmk_region,policy_blake3,tcs_version,wrapped_tcs_blake3,
         source_generation,target_generation,observed_gate_epoch,claim_owner,claim_token,
         claim_epoch,claim_expires_at_ms,deadline_at_ms,created_at_ms,checkpointed_at_ms)
        VALUES ('activation-d','tenant-d','guard-d',?,1,'byok','convergent','aws','key-d',
                'us-east-1',?,1,?,0,1,1,'worker-d','claim-d',1,9999999999999,
                9999999999999,1,1)""",
        ("d" * 64, h, h),
    )
    connection.execute(
        "UPDATE byok_transition_fence SET outcome='expired',completed_at_ms=2 WHERE token='token-d'"
    )
    connection.execute(
        "UPDATE byok_activation_guard SET outcome='expired',completed_at_ms=2 WHERE guard_id='guard-d'"
    )
    expect_integrity_error(
        connection,
        """INSERT INTO byok_data_intent
        (token,tenant_id,operation,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('blocked-expired-copy','tenant-d','read',1,0,1,'pending','active',1,9999999999999)""",
    )

    # Rotation cancel restores the exact source custody from immutable history,
    # advances both live versions, and truthfully aborts without shredding gen1.
    connection.execute("INSERT INTO tenant (tenant_id,byok_status) VALUES ('tenant-e','active')")
    connection.execute(
        """INSERT INTO tenant_byok_config
        (tenant_id,mode,crypto_mode,cmk_provider,cmk_key_id,cmk_region,state,config_version)
        VALUES ('tenant-e','byok','convergent','aws','old-key','us-east-1','active',1)"""
    )
    connection.execute(
        "INSERT INTO tenant_byok_secret VALUES ('tenant-e',x'11','old-key',1,1)"
    )
    connection.execute(
        "UPDATE byok_tenant_gate SET current_generation=1 WHERE tenant_id='tenant-e'"
    )
    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('token-e','tenant-e',1,1,1,1,'active','active',1,9999999999999)"""
    )
    connection.execute(
        """INSERT INTO byok_activation_source_capture
        (transition_token,tenant_id,source_generation,source_config_version,
         source_cmk_provider,source_cmk_key_id,source_cmk_region,source_tcs_version,captured_at_ms)
        VALUES ('token-e','tenant-e',1,1,'aws','old-key','us-east-1',1,1)"""
    )
    expect_integrity_error(
        connection,
        """UPDATE byok_activation_source_capture SET source_cmk_key_id='wrong-key'
        WHERE transition_token='token-e'""",
    )
    connection.execute(
        """UPDATE tenant_byok_config SET cmk_key_id='new-key',state='pending',config_version=2
        WHERE tenant_id='tenant-e'"""
    )
    connection.execute(
        """UPDATE tenant_byok_secret SET tcs_wrapped=x'22',cmk_key_id='new-key',tcs_version=2
        WHERE tenant_id='tenant-e'"""
    )
    connection.execute(
        """INSERT INTO byok_activation_guard
        (guard_id,tenant_id,transition_token,transition_epoch,capability_token,
         epoch,action,priority,acquired_at_ms,expires_at_ms)
        VALUES ('guard-e','tenant-e','token-e',1,'cap-e',1,'activate',10,1,9999999999999)"""
    )
    connection.execute(
        """INSERT INTO byok_activation_intent
        (intent_id,tenant_id,guard_id,request_blake3,config_version,mode,crypto_mode,
         cmk_provider,cmk_key_id,cmk_region,policy_blake3,tcs_version,wrapped_tcs_blake3,
         source_generation,source_config_version,source_cmk_provider,source_cmk_key_id,
         source_cmk_region,source_tcs_version,target_generation,observed_gate_epoch,
         claim_owner,claim_token,claim_epoch,claim_expires_at_ms,deadline_at_ms,
         created_at_ms,checkpointed_at_ms)
        VALUES ('activation-e','tenant-e','guard-e',?,2,'byok','convergent','aws','new-key',
                'us-east-1',?,2,?,1,1,'aws','old-key','us-east-1',1,2,1,
                'worker-e','claim-e',1,9999999999999,9999999999999,1,1)""",
        ("e" * 64, h, h),
    )
    pinned = connection.execute(
        """SELECT source_config_version,source_cmk_provider,source_cmk_key_id,
                  source_cmk_region,source_tcs_version
             FROM byok_activation_intent WHERE intent_id='activation-e'"""
    ).fetchone()
    if pinned != (1, "aws", "old-key", "us-east-1", 1):
        raise AssertionError("activation intent did not retain exact source custody pins")

    # An expired rotated activation is still an unpublished copy.  The old
    # generic abort mutation forced `inactive` (and cleared the source TCS),
    # which made the real 0121 abort postcondition reject the batch and left
    # the intent stuck in `copy`.  Prove the migration catches that mutant,
    # then execute the exact source restore + CAS predicates used by the
    # adapter and prove the terminal postcondition commits.
    connection.execute("SAVEPOINT rotated_deadline_abort_mutant")
    connection.execute(
        """INSERT INTO byok_activation_operation_guard
        (operation_token,intent_id,claim_token,expected_state_version,action,checked_at_ms)
        VALUES ('abort-e-mutant','activation-e','claim-e',1,'abort',1)"""
    )
    connection.execute(
        """UPDATE tenant_byok_config SET state='inactive',config_version=config_version+1,
        cmk_provider=NULL,cmk_key_id=NULL,cmk_region=NULL
        WHERE tenant_id='tenant-e' AND state='pending' AND config_version=2"""
    )
    connection.execute(
        """UPDATE tenant_byok_secret SET tcs_wrapped=NULL,cmk_key_id=NULL,
        wrapped_at_ms=NULL,tcs_version=tcs_version+1
        WHERE tenant_id='tenant-e' AND tcs_version=2 AND cmk_key_id='new-key'"""
    )
    connection.execute(
        """UPDATE byok_activation_intent SET phase='aborted',failure_reason='deadline',
        completed_at_ms=2,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL,
        state_version=state_version+1 WHERE intent_id='activation-e'"""
    )
    expect_integrity_error(
        connection,
        """INSERT INTO byok_activation_postcondition
        (operation_token,intent_id,expected_state_version,expected_phase,checked_at_ms)
        VALUES ('abort-e-mutant-post','activation-e',2,'aborted',2)""",
    )
    connection.execute("ROLLBACK TO rotated_deadline_abort_mutant")
    connection.execute("RELEASE rotated_deadline_abort_mutant")

    connection.execute("SAVEPOINT rotated_deadline_abort")
    connection.execute(
        """INSERT INTO byok_activation_operation_guard
        (operation_token,intent_id,claim_token,expected_state_version,action,checked_at_ms)
        VALUES ('abort-e','activation-e','claim-e',1,'abort',1)"""
    )
    connection.execute(
        """UPDATE tenant_byok_secret SET
        tcs_wrapped=(SELECT h.tcs_wrapped FROM tenant_byok_secret_history h
                     WHERE h.tenant_id='tenant-e' AND h.tcs_version=1
                       AND h.cmk_provider='aws' AND h.cmk_key_id='old-key'
                       AND h.cmk_region='us-east-1' AND h.tcs_wrapped IS NOT NULL),
        cmk_key_id='old-key',tcs_version=tcs_version+1
        WHERE tenant_id='tenant-e' AND tcs_version=2 AND cmk_key_id='new-key'
          AND tcs_wrapped IS NOT NULL
          AND EXISTS (SELECT 1 FROM tenant_byok_secret_history h
              WHERE h.tenant_id='tenant-e' AND h.tcs_version=1
                AND h.cmk_provider='aws' AND h.cmk_key_id='old-key'
                AND h.cmk_region='us-east-1' AND h.tcs_wrapped IS NOT NULL)
              AND EXISTS (SELECT 1 FROM tenant_byok_config c
              WHERE c.tenant_id='tenant-e' AND c.state='pending'
                AND c.config_version=2 AND c.cmk_key_id='new-key')"""
    )
    # Mutation contract: if the target-history retirement statement is omitted,
    # the archive trigger leaves the unpublished new-key TCS wrapped. The
    # invariant below must reject that mutation before the real cleanup path.
    connection.execute("SAVEPOINT rotated_deadline_abort_missing_cleanup")
    try:
        require_retired_target_history(connection)
    except AssertionError:
        pass
    else:
        raise AssertionError("cleanup omission mutation was not detected")
    connection.execute("ROLLBACK TO rotated_deadline_abort_missing_cleanup")
    connection.execute("RELEASE rotated_deadline_abort_missing_cleanup")

    connection.execute(
        """UPDATE tenant_byok_secret_history SET tcs_wrapped=NULL,retired_at_ms=2
        WHERE tenant_id='tenant-e' AND tcs_version=2
          AND cmk_provider='aws' AND cmk_key_id='new-key' AND cmk_region='us-east-1'
          AND tcs_wrapped IS NOT NULL AND EXISTS (
            SELECT 1 FROM byok_activation_intent a
            WHERE a.intent_id='activation-e'
              AND a.tenant_id=tenant_byok_secret_history.tenant_id
              AND a.tcs_version=2 AND a.cmk_provider='aws'
              AND a.cmk_key_id='new-key' AND a.cmk_region='us-east-1'
              AND a.phase='copy')"""
    )
    connection.execute(
        """UPDATE tenant_byok_config SET
        mode=(SELECT h.mode FROM tenant_byok_config_history h
              WHERE h.tenant_id='tenant-e' AND h.config_version=1
                AND h.state='active' AND h.cmk_provider='aws'
                AND h.cmk_key_id='old-key' AND h.cmk_region='us-east-1'),
        crypto_mode=(SELECT h.crypto_mode FROM tenant_byok_config_history h
              WHERE h.tenant_id='tenant-e' AND h.config_version=1
                AND h.state='active' AND h.cmk_provider='aws'
                AND h.cmk_key_id='old-key' AND h.cmk_region='us-east-1'),
        cmk_provider='aws',cmk_key_id='old-key',cmk_region='us-east-1',
        state='active',config_version=config_version+1
        WHERE tenant_id='tenant-e' AND state='pending' AND config_version=2
          AND mode='byok' AND crypto_mode='convergent'
          AND cmk_provider='aws' AND cmk_key_id='new-key' AND cmk_region='us-east-1'
          AND EXISTS (SELECT 1 FROM tenant_byok_config_history h
              WHERE h.tenant_id='tenant-e' AND h.config_version=1
                AND h.state='active' AND h.cmk_provider='aws'
                AND h.cmk_key_id='old-key' AND h.cmk_region='us-east-1')
          AND EXISTS (SELECT 1 FROM byok_tenant_gate g
              WHERE g.tenant_id='tenant-e' AND g.current_generation=1)
          AND NOT EXISTS (SELECT 1 FROM byok_logical_object_publication p
              WHERE p.tenant_id='tenant-e' AND p.generation<>1)"""
    )
    connection.execute(
        """UPDATE byok_activation_intent SET phase='aborted',failure_reason='deadline',
        completed_at_ms=2,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL,
        state_version=state_version+1 WHERE intent_id='activation-e'"""
    )
    connection.execute(
        """INSERT INTO byok_activation_postcondition
        (operation_token,intent_id,expected_state_version,expected_phase,checked_at_ms)
        VALUES ('abort-e-post','activation-e',2,'aborted',2)"""
    )
    restored = connection.execute(
        """SELECT c.state,c.config_version,c.cmk_provider,c.cmk_key_id,
                         s.cmk_key_id,s.tcs_wrapped,g.current_generation,
                         a.phase
        FROM tenant_byok_config c
        JOIN tenant_byok_secret s ON s.tenant_id=c.tenant_id
        JOIN byok_tenant_gate g ON g.tenant_id=c.tenant_id
        JOIN byok_activation_intent a ON a.tenant_id=c.tenant_id
        WHERE c.tenant_id='tenant-e' AND a.intent_id='activation-e'"""
    ).fetchone()
    if restored != ("active", 3, "aws", "old-key", "old-key", b"\x11", 1, "aborted"):
        raise AssertionError(f"expired rotated activation did not restore source custody: {restored}")
    require_retired_target_history(connection)
    source_history = connection.execute(
        """SELECT tcs_wrapped,retired_at_ms
        FROM tenant_byok_secret_history
        WHERE tenant_id='tenant-e' AND tcs_version=1
          AND cmk_provider='aws' AND cmk_key_id='old-key' AND cmk_region='us-east-1'"""
    ).fetchone()
    if source_history != (b"\x11", None):
        raise AssertionError(f"normal abort damaged source TCS history: {source_history}")
    connection.execute("ROLLBACK TO rotated_deadline_abort")
    connection.execute("RELEASE rotated_deadline_abort")

    # Generation-zero aborts have no source history to restore, but the same
    # trigger still archives the target TCS while clearing the live secret.
    # Prove that normal abort retires that exact target row before returning to
    # inactive, without invoking the preempt-wide historical shred.
    connection.execute("SAVEPOINT generation_zero_deadline_abort")
    connection.execute("INSERT INTO tenant (tenant_id,byok_status) VALUES ('tenant-f','active')")
    connection.execute(
        """INSERT INTO tenant_byok_config
        (tenant_id,mode,crypto_mode,cmk_provider,cmk_key_id,cmk_region,state,config_version)
        VALUES ('tenant-f','byok','convergent','aws','f-new','us-east-1','pending',1)"""
    )
    connection.execute(
        "INSERT INTO tenant_byok_secret VALUES ('tenant-f',x'33','f-new',1,1)"
    )
    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('token-f','tenant-f',1,1,0,1,'pending','active',1,9999999999999)"""
    )
    connection.execute(
        """INSERT INTO byok_activation_guard
        (guard_id,tenant_id,transition_token,transition_epoch,capability_token,
         epoch,action,priority,acquired_at_ms,expires_at_ms)
        VALUES ('guard-f','tenant-f','token-f',1,'cap-f',1,'activate',10,1,9999999999999)"""
    )
    connection.execute(
        """INSERT INTO byok_activation_intent
        (intent_id,tenant_id,guard_id,request_blake3,config_version,mode,crypto_mode,
         cmk_provider,cmk_key_id,cmk_region,policy_blake3,tcs_version,wrapped_tcs_blake3,
         source_generation,target_generation,observed_gate_epoch,claim_owner,claim_token,
         claim_epoch,claim_expires_at_ms,deadline_at_ms,created_at_ms,checkpointed_at_ms)
        VALUES ('activation-f','tenant-f','guard-f',?,1,'byok','convergent','aws','f-new',
                'us-east-1',?,1,?,0,1,1,'worker-f','claim-f',1,9999999999999,
                9999999999999,1,1)""",
        ("f" * 64, h, h),
    )
    connection.execute(
        """INSERT INTO byok_activation_operation_guard
        (operation_token,intent_id,claim_token,expected_state_version,action,checked_at_ms)
        VALUES ('abort-f','activation-f','claim-f',1,'abort',1)"""
    )
    connection.execute(
        """UPDATE tenant_byok_secret SET tcs_wrapped=NULL,cmk_key_id=NULL,
        wrapped_at_ms=NULL,tcs_version=tcs_version+1
        WHERE tenant_id='tenant-f' AND tcs_version=1 AND cmk_key_id='f-new'
          AND tcs_wrapped IS NOT NULL"""
    )
    connection.execute(
        """UPDATE tenant_byok_secret_history SET tcs_wrapped=NULL,retired_at_ms=2
        WHERE tenant_id='tenant-f' AND tcs_version=1
          AND cmk_provider='aws' AND cmk_key_id='f-new' AND cmk_region='us-east-1'
          AND tcs_wrapped IS NOT NULL AND EXISTS (
            SELECT 1 FROM byok_activation_intent a
            WHERE a.intent_id='activation-f'
              AND a.tenant_id=tenant_byok_secret_history.tenant_id
              AND a.tcs_version=1 AND a.cmk_provider='aws'
              AND a.cmk_key_id='f-new' AND a.cmk_region='us-east-1'
              AND a.phase='copy')"""
    )
    connection.execute(
        """UPDATE tenant_byok_config SET state='inactive',config_version=config_version+1,
        cmk_provider=NULL,cmk_key_id=NULL,cmk_region=NULL
        WHERE tenant_id='tenant-f' AND state='pending' AND config_version=1
          AND mode='byok' AND crypto_mode='convergent' AND cmk_provider='aws'
          AND cmk_key_id='f-new' AND cmk_region='us-east-1'"""
    )
    connection.execute(
        """UPDATE byok_activation_intent SET phase='aborted',failure_reason='deadline',
        completed_at_ms=2,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL,
        state_version=state_version+1 WHERE intent_id='activation-f'"""
    )
    connection.execute(
        """INSERT INTO byok_activation_postcondition
        (operation_token,intent_id,expected_state_version,expected_phase,checked_at_ms)
        VALUES ('abort-f-post','activation-f',2,'aborted',2)"""
    )
    generation_zero = connection.execute(
        """SELECT c.state,c.config_version,c.cmk_provider,c.cmk_key_id,
                         s.tcs_wrapped,a.phase
        FROM tenant_byok_config c
        JOIN tenant_byok_secret s ON s.tenant_id=c.tenant_id
        JOIN byok_activation_intent a ON a.tenant_id=c.tenant_id
        WHERE c.tenant_id='tenant-f' AND a.intent_id='activation-f'"""
    ).fetchone()
    if generation_zero != ("inactive", 2, None, None, None, "aborted"):
        raise AssertionError(f"generation-zero abort did not clear target custody: {generation_zero}")
    target_history = connection.execute(
        """SELECT tcs_wrapped,retired_at_ms
        FROM tenant_byok_secret_history
        WHERE tenant_id='tenant-f' AND tcs_version=1
          AND cmk_provider='aws' AND cmk_key_id='f-new' AND cmk_region='us-east-1'"""
    ).fetchone()
    if target_history != (None, 2):
        raise AssertionError(f"generation-zero target TCS history is not retired: {target_history}")
    connection.execute("ROLLBACK TO generation_zero_deadline_abort")
    connection.execute("RELEASE generation_zero_deadline_abort")

    connection.execute("SAVEPOINT source_copy_guard")
    exercise_source_degrade_guard(connection, "copy")
    connection.execute("ROLLBACK TO source_copy_guard")
    connection.execute("RELEASE source_copy_guard")

    connection.execute("SAVEPOINT source_purge_guard")
    connection.execute(
        """UPDATE byok_activation_intent SET cas_complete=1,ac_complete=1,
        phase='published_partial',publication_gate_epoch=2,published_config_version=3,
        state_version=state_version+1 WHERE intent_id='activation-e'"""
    )
    connection.execute(
        "UPDATE byok_tenant_gate SET gate_epoch=2,current_generation=2 WHERE tenant_id='tenant-e'"
    )
    connection.execute(
        "UPDATE tenant_byok_config SET state='partial',config_version=3 WHERE tenant_id='tenant-e'"
    )
    connection.execute(
        "UPDATE byok_activation_intent SET phase='purging',state_version=state_version+1 "
        "WHERE intent_id='activation-e'"
    )
    exercise_source_degrade_guard(connection, "purging")
    connection.execute("ROLLBACK TO source_purge_guard")
    connection.execute("RELEASE source_purge_guard")

    connection.execute(
        "DELETE FROM byok_activation_source_capture WHERE transition_token='token-e'"
    )
    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_version,observed_config_state,observed_byok_status,
         acquired_at_ms,expires_at_ms)
        VALUES ('control-e','tenant-e',2,1,1,2,'pending','active',1,9999999999999)"""
    )
    connection.execute(
        """INSERT INTO byok_transition_commit_guard
        (token,tenant_id,epoch,action,cmk_provider,cmk_key_id,cmk_region)
        VALUES ('control-e','tenant-e',2,'deactivate','aws','new-key','us-east-1')"""
    )
    connection.execute(
        """INSERT INTO byok_activation_operation_guard
        (operation_token,intent_id,control_token,expected_state_version,action,checked_at_ms)
        VALUES ('cancel-e','activation-e','control-e',1,'cancel',1)"""
    )
    connection.execute(
        """UPDATE tenant_byok_config SET
        mode=(SELECT mode FROM tenant_byok_config_history WHERE tenant_id='tenant-e' AND config_version=1),
        crypto_mode=(SELECT crypto_mode FROM tenant_byok_config_history WHERE tenant_id='tenant-e' AND config_version=1),
        cmk_provider=(SELECT cmk_provider FROM tenant_byok_config_history WHERE tenant_id='tenant-e' AND config_version=1),
        cmk_key_id=(SELECT cmk_key_id FROM tenant_byok_config_history WHERE tenant_id='tenant-e' AND config_version=1),
        cmk_region=(SELECT cmk_region FROM tenant_byok_config_history WHERE tenant_id='tenant-e' AND config_version=1),
        state='active',config_version=3 WHERE tenant_id='tenant-e'"""
    )
    connection.execute(
        """UPDATE tenant_byok_secret SET
        tcs_wrapped=(SELECT tcs_wrapped FROM tenant_byok_secret_history WHERE tenant_id='tenant-e' AND tcs_version=1),
        cmk_key_id='old-key',tcs_version=3 WHERE tenant_id='tenant-e'"""
    )
    connection.execute(
        "UPDATE tenant_byok_secret_history SET tcs_wrapped=NULL,retired_at_ms=3 WHERE tenant_id='tenant-e'"
    )
    connection.execute(
        """UPDATE byok_activation_intent SET phase='aborted',failure_reason='operator_deactivate',
        completed_at_ms=3,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL,
        state_version=2 WHERE intent_id='activation-e'"""
    )
    connection.execute(
        "UPDATE byok_transition_fence SET outcome='committed',completed_at_ms=3 WHERE token='control-e'"
    )
    connection.execute(
        """INSERT INTO byok_activation_postcondition
        (operation_token,intent_id,expected_state_version,expected_phase,checked_at_ms)
        VALUES ('cancel-e-post','activation-e',2,'aborted',3)"""
    )
    restored = connection.execute(
        """SELECT c.state,c.config_version,c.cmk_key_id,s.tcs_version,s.tcs_wrapped
        FROM tenant_byok_config c JOIN tenant_byok_secret s ON s.tenant_id=c.tenant_id
        WHERE c.tenant_id='tenant-e'"""
    ).fetchone()
    if restored != ("active", 3, "old-key", 3, b"\x11"):
        raise AssertionError("rotation cancel did not restore exact source custody")

    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_state,observed_byok_status,acquired_at_ms,expires_at_ms)
        VALUES ('token-expired','tenant-c',1,1,0,'absent','active',
                (CAST(strftime('%s','now') AS INTEGER)*1000),
                (CAST(strftime('%s','now') AS INTEGER)*1000)+100)"""
    )
    connection.execute(
        """INSERT INTO byok_activation_guard
        (guard_id,tenant_id,transition_token,transition_epoch,capability_token,
         epoch,action,priority,acquired_at_ms,expires_at_ms)
        VALUES ('guard-expired','tenant-c','token-expired',1,'cap-old',1,'activate',10,
                (CAST(strftime('%s','now') AS INTEGER)*1000),
                (CAST(strftime('%s','now') AS INTEGER)*1000)+100)"""
    )
    time.sleep(1.1)
    connection.execute(
        """INSERT INTO byok_transition_fence
        (token,tenant_id,epoch,observed_gate_epoch,observed_generation,
         observed_config_state,observed_byok_status,acquired_at_ms,expires_at_ms)
        VALUES ('token-takeover','tenant-c',2,1,0,'absent','active',
                (CAST(strftime('%s','now') AS INTEGER)*1000),
                (CAST(strftime('%s','now') AS INTEGER)*1000)+10000)"""
    )
    connection.execute(
        """UPDATE byok_activation_guard SET transition_token='token-takeover',transition_epoch=2,
        capability_token='cap-new',epoch=2,
        acquired_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000),
        expires_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000)+10000
        WHERE guard_id='guard-expired'"""
    )
    epoch, capability = connection.execute(
        "SELECT epoch,capability_token FROM byok_activation_guard WHERE guard_id='guard-expired'"
    ).fetchone()
    if (epoch, capability) != (2, "cap-new"):
        raise AssertionError("expired guard takeover did not rotate exact capability")
    old_outcome = connection.execute(
        "SELECT outcome FROM byok_transition_fence WHERE token='token-expired'"
    ).fetchone()[0]
    if old_outcome != "expired":
        raise AssertionError("expired takeover misclassified its prior transition fence")


if __name__ == "__main__":
    verify_runtime_contract()
    verify_schema()
    print("B-083 backfill hermetic checks: PASS")
