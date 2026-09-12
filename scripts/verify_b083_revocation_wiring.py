#!/usr/bin/env python3
"""Focal B-083/D03 verifier for revocation scheduler wiring.

This is intentionally source-level and cheap while the repository's long
Cargo process is running.  It verifies the executable Rust focal exists, then
mutates the two load-bearing teeth (scheduler spawn and population query) in
memory and requires the same checks to reject them.  No provider, D1, network,
build, or CI command is executed.
"""

from __future__ import annotations

import sqlite3
from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "crates/corelink-container/src/main.rs"
RUNTIME = ROOT / "crates/corelink-container/src/byok_revocation_runtime.rs"
DETECTOR = ROOT / "crates/corelink-byok/src/byok_revocation/detector.rs"
FOCAL = ROOT / "crates/corelink-byok/tests/byok_revocation_wiring.rs"
MIGRATION = ROOT / "migrations/d1/0114_byok_revocation_customer_audit_atomic.sql"
CONTROL = ROOT / "crates/corelink-container/src/byok_control_transition.rs"
ADMIN = ROOT / "crates/corelink-container/src/routes/byok_admin.rs"
TEST_INCLUDE_MARKER = 'include!("byok_revocation_runtime/part-01.rs");'


def require(haystack: str, needle: str, label: str) -> None:
    if needle not in haystack:
        raise AssertionError(f"{label}: missing {needle!r}")


def _mask_rust_comments_and_strings(source: str) -> str:
    """Blank Rust comments/strings while retaining code and line positions.

    The focal guard must not be satisfied by copying a required token into a
    doc comment, a string literal, or a raw-string fixture.  This small lexer
    handles nested block comments, escaped strings, byte strings, and raw
    strings; it intentionally preserves newlines for useful diagnostics.
    """
    out = list(source)
    i = 0
    block_depth = 0
    mode = "code"
    raw_hashes = 0
    while i < len(source):
        if mode == "line":
            if source[i] == "\n":
                mode = "code"
            else:
                out[i] = " "
            i += 1
            continue
        if mode == "block":
            if source.startswith("/*", i):
                out[i:i + 2] = "  "
                block_depth += 1
                i += 2
            elif source.startswith("*/", i):
                out[i:i + 2] = "  "
                block_depth -= 1
                i += 2
                if block_depth == 0:
                    mode = "code"
            else:
                if source[i] != "\n":
                    out[i] = " "
                i += 1
            continue
        if mode == "string":
            if source[i] == "\\":
                out[i] = " "
                if i + 1 < len(source):
                    if source[i + 1] != "\n":
                        out[i + 1] = " "
                    i += 2
                else:
                    i += 1
            elif source[i] == '"':
                out[i] = " "
                mode = "code"
                i += 1
            else:
                if source[i] != "\n":
                    out[i] = " "
                i += 1
            continue
        if mode == "raw":
            terminator = '"' + ("#" * raw_hashes)
            if source.startswith(terminator, i):
                out[i:i + len(terminator)] = " " * len(terminator)
                i += len(terminator)
                mode = "code"
            else:
                if source[i] != "\n":
                    out[i] = " "
                i += 1
            continue

        if source.startswith("//", i):
            out[i:i + 2] = "  "
            mode = "line"
            i += 2
        elif source.startswith("/*", i):
            out[i:i + 2] = "  "
            mode = "block"
            block_depth = 1
            i += 2
        else:
            raw = re.match(r"(?:b)?r(#+)?\"", source[i:])
            if raw:
                raw_hashes = len(raw.group(1) or "")
                width = len(raw.group(0))
                out[i:i + width] = " " * width
                i += width
                mode = "raw"
            elif source[i] == '"' or (source[i] == "b" and i + 1 < len(source) and source[i + 1] == '"'):
                if source[i] == "b":
                    out[i] = " "
                    i += 1
                out[i] = " "
                mode = "string"
                i += 1
            else:
                i += 1
    return "".join(out)


def _active_test_include_count(source: str) -> int:
    """Count executable ``include!`` calls for the revocation test module.

    The comment/string masker removes comments and string bodies, leaving the
    macro token visible only when it is executable.  The original source is
    then consulted at that token to validate the macro's actual string path;
    this prevents both ``// include!(...)`` and string-literal bait from being
    accepted as the include boundary.
    """
    masked = _mask_rust_comments_and_strings(source)
    call = re.compile(r"\binclude!\s*\(\s*")
    literal = re.compile(
        r'include!\s*\(\s*"([^"\\]*(?:\\.[^"\\]*)*)"\s*\)\s*;'
    )
    count = 0
    for match in call.finditer(masked):
        candidate = literal.match(source[match.start() :])
        if not candidate or candidate.group(1) != TEST_INCLUDE_MARKER.split('"', 2)[1]:
            continue
        if _has_cfg_attribute_on_include(masked, match.start()):
            raise AssertionError(
                "required revocation test include must not be cfg-gated"
            )
        count += 1
    return count


def _has_cfg_attribute_on_include(masked: str, include_start: int) -> bool:
    """Return whether an active ``cfg``/``cfg_attr`` directly gates an include.

    ``masked`` has comments and string bodies replaced with spaces, so only
    executable attributes remain visible.  We reject *any* cfg attribute on
    this required include: a condition that is currently true can become
    false under another build profile, while ``cfg(any())`` and ``cfg(test)``
    are unconditionally compile-disabled for the production adapter.
    """
    attribute = re.compile(r"#\s*\[\s*(?:cfg|cfg_attr)\b")

    def matching_bracket(start: int) -> int | None:
        depth = 0
        for index in range(start, include_start):
            if masked[index] == "[":
                depth += 1
            elif masked[index] == "]":
                depth -= 1
                if depth == 0:
                    return index + 1
        return None

    def only_attributes(start: int) -> bool:
        index = start
        while index < include_start:
            while index < include_start and masked[index].isspace():
                index += 1
            if index == include_start:
                return True
            if masked.startswith("#[", index):
                end = matching_bracket(index + 1)
                if end is None:
                    return False
                index = end
                continue
            return False
        return True

    for match in attribute.finditer(masked, 0, include_start):
        open_bracket = masked.find("[", match.start(), match.end())
        end = matching_bracket(open_bracket) if open_bracket >= 0 else None
        if end is not None and only_attributes(end):
            return True
    return False


def _require_active_test_include(source: str) -> None:
    """Require exactly one executable runtime/test include, not comment bait."""
    count = _active_test_include_count(source)
    if count != 1:
        raise AssertionError(
            "production/test include boundary must contain exactly one active include "
            f"(found {count})"
        )


def _rust_string_literals(source: str) -> str:
    """Return only non-comment Rust string bodies for SQL-shape checks."""
    # First mask comments but leave strings intact.  A regex over that result
    # is sufficient for Rust literals and, importantly, cannot see comment
    # bait.  (The code-shape verifier above uses the stricter full mask.)
    uncommented = list(source)
    i = 0
    mode = "code"
    depth = 0
    raw_hashes = 0
    while i < len(source):
        if mode == "line":
            if source[i] == "\n":
                mode = "code"
            else:
                uncommented[i] = " "
            i += 1
            continue
        if mode == "block":
            if source.startswith("/*", i):
                uncommented[i:i + 2] = "  "
                depth += 1
                i += 2
            elif source.startswith("*/", i):
                uncommented[i:i + 2] = "  "
                depth -= 1
                i += 2
                if depth == 0:
                    mode = "code"
            else:
                if source[i] != "\n":
                    uncommented[i] = " "
                i += 1
            continue
        if mode == "raw":
            marker = '"' + ("#" * raw_hashes)
            if source.startswith(marker, i):
                i += len(marker)
                mode = "code"
            else:
                i += 1
            continue
        raw = re.match(r"(?:b)?r(#+)?\"", source[i:])
        if raw:
            raw_hashes = len(raw.group(1) or "")
            i += len(raw.group(0))
            mode = "raw"
            continue
        if source.startswith("//", i):
            uncommented[i:i + 2] = "  "
            mode = "line"
            i += 2
        elif source.startswith("/*", i):
            uncommented[i:i + 2] = "  "
            mode = "block"
            depth = 1
            i += 2
        elif source[i] == '"':
            i += 1
            while i < len(source):
                if source[i] == "\\":
                    i += 2
                elif source[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
        else:
            i += 1
    uncommented_text = "".join(uncommented)
    literals: list[str] = []
    i = 0
    while i < len(uncommented_text):
        raw = re.match(r"(?:b)?r(#+)?\"", uncommented_text[i:])
        if raw:
            hashes = len(raw.group(1) or "")
            start = i + len(raw.group(0))
            end_marker = '"' + ("#" * hashes)
            end = uncommented_text.find(end_marker, start)
            if end >= 0:
                literals.append(uncommented_text[start:end])
                i = end + len(end_marker)
                continue
        if uncommented_text[i] == '"' or (uncommented_text[i] == "b" and i + 1 < len(uncommented_text) and uncommented_text[i + 1] == '"'):
            start = i + (2 if uncommented_text[i] == "b" else 1)
            end = start
            escaped = False
            while end < len(uncommented_text):
                char = uncommented_text[end]
                if char == '"' and not escaped:
                    break
                escaped = char == "\\" and not escaped
                if char != "\\":
                    escaped = False
                end += 1
            if end < len(uncommented_text):
                literals.append(uncommented_text[start:end])
                i = end + 1
                continue
        i += 1
    return "\n".join(literals)


def _function_source(source: str, signature: str) -> str:
    """Return one Rust function using masked code for structural balancing."""
    masked = _mask_rust_comments_and_strings(source)
    start = masked.find(signature)
    if start < 0:
        raise AssertionError(f"function missing: {signature}")
    brace = masked.find("{", start)
    if brace < 0:
        raise AssertionError(f"function body missing: {signature}")
    depth = 0
    for index in range(brace, len(masked)):
        if masked[index] == "{":
            depth += 1
        elif masked[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start:index + 1]
    raise AssertionError(f"unterminated function: {signature}")


def _first_query_argument(source: str, signature: str) -> str:
    """Extract the expression actually passed as the first ``.query`` arg."""
    function = _function_source(source, signature)
    masked = _mask_rust_comments_and_strings(function)
    matches = list(re.finditer(r"\.query\s*\(", masked))
    if len(matches) != 1:
        raise AssertionError(
            f"{signature}: expected exactly one executable .query call, found {len(matches)}"
        )
    opening = matches[0].end() - 1
    argument_start = opening + 1
    depth = 0
    for index in range(argument_start, len(masked)):
        char = masked[index]
        if char == "(":
            depth += 1
        elif char == ")":
            if depth == 0:
                raise AssertionError(f"{signature}: query has no first argument")
            depth -= 1
        elif char == "," and depth == 0:
            argument = function[argument_start:index].strip()
            if not argument:
                raise AssertionError(f"{signature}: query has an empty first argument")
            return argument
    raise AssertionError(f"{signature}: unterminated query call")


def _mask_sql_comments(source: str) -> str:
    """Mask SQL comments while preserving quoted SQL and its tokens."""
    out = list(source)
    i = 0
    mode = "code"
    while i < len(source):
        if mode == "line":
            if source[i] == "\n":
                mode = "code"
            else:
                out[i] = " "
            i += 1
            continue
        if mode == "block":
            if source.startswith("*/", i):
                out[i:i + 2] = "  "
                i += 2
                mode = "code"
            else:
                if source[i] != "\n":
                    out[i] = " "
                i += 1
            continue
        if source[i] == "'":
            i += 1
            while i < len(source):
                if source[i] == "'":
                    if i + 1 < len(source) and source[i + 1] == "'":
                        i += 2
                    else:
                        i += 1
                        break
                else:
                    i += 1
        elif source.startswith("--", i):
            out[i:i + 2] = "  "
            mode = "line"
            i += 2
        elif source.startswith("/*", i):
            out[i:i + 2] = "  "
            mode = "block"
            i += 2
        else:
            i += 1
    return "".join(out)


def _verify_sqlite_trigger_semantics(migration: str) -> None:
    """Exercise 0114's trigger with the adapter's status SQL shape."""
    connection: sqlite3.Connection | None = None
    try:
        connection = sqlite3.connect(":memory:")
        connection.executescript(
            """CREATE TABLE tenant (
                tenant_id TEXT PRIMARY KEY, byok_status TEXT NOT NULL,
                byok_revoked_at_ms INTEGER, byok_revoked_provider TEXT,
                byok_revoked_kms_key_id TEXT
            );
            CREATE TABLE tenant_byok_config (
                tenant_id TEXT PRIMARY KEY, cmk_provider TEXT NOT NULL,
                cmk_key_id TEXT NOT NULL
            );
            CREATE TABLE customer_audit_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT, tenant_id TEXT NOT NULL,
                event_type TEXT NOT NULL, actor TEXT, target TEXT,
                ts_ms INTEGER NOT NULL, detail TEXT
            );"""
        )
        connection.executescript(migration)
        connection.execute(
            "INSERT INTO tenant VALUES (?, ?, NULL, NULL, NULL)",
            ("t-b083", "active"),
        )
        connection.execute(
            "INSERT INTO tenant_byok_config VALUES (?, ?, ?)",
            ("t-b083", "aws", "arn:aws:kms:test:key/b083"),
        )
        connection.commit()
        connection.execute("BEGIN")
        connection.execute(
            """UPDATE tenant SET byok_status = 'degraded_read_only',
               byok_revoked_at_ms = ?, byok_revoked_provider = ?,
               byok_revoked_kms_key_id = ?
               WHERE tenant_id IN (SELECT tenant_id FROM tenant_byok_config
                 WHERE cmk_provider = ? AND cmk_key_id = ?)""",
            (1, "aws", "arn:aws:kms:test:key/b083", "aws", "arn:aws:kms:test:key/b083"),
        )
        if connection.execute("SELECT count(*) FROM customer_audit_events").fetchone()[0] != 1:
            raise AssertionError("0114 did not atomically emit the revoke event")
        connection.rollback()
        if connection.execute("SELECT byok_status FROM tenant").fetchone()[0] != "active":
            raise AssertionError("0114 rollback left the tenant degraded")
        if connection.execute("SELECT count(*) FROM customer_audit_events").fetchone()[0] != 0:
            raise AssertionError("0114 rollback left an audit half-transition")
    except (sqlite3.Error, TypeError) as exc:
        raise AssertionError(f"0114 SQLite semantic check failed: {exc}") from exc
    finally:
        if connection is not None:
            connection.close()


def verify(main: str, runtime: str, detector: str, focal: str, migration: str) -> None:
    _require_active_test_include(runtime)
    runtime_production = runtime
    main_code = _mask_rust_comments_and_strings(main)
    runtime_code = _mask_rust_comments_and_strings(runtime_production)
    detector_code = _mask_rust_comments_and_strings(detector)
    focal_code = _mask_rust_comments_and_strings(focal)
    migration_sql = _mask_sql_comments(migration)
    # Entry/scheduler: run_loop must be the spawned future and must be built
    # before listener bind, with explicit durable collaborators.
    require(main_code, "tokio::spawn(detector.run_loop())", "production scheduler")
    require(main_code, "detector_for_client", "production detector factory")
    require(main_code, "let _byok_revocation_task", "startup task ownership")
    require(main_code, "StorageEnv::from_env()", "fail-closed lifecycle")
    require(main_code, "D1HttpClient::new", "durable D1 startup")
    require(main_code, "make_provider()", "real provider startup")
    require(main_code, "detector.has_key_source()", "authoritative source startup witness")
    require(main_code, "TcpListener::bind", "listener bind")
    if main_code.index("tokio::spawn(detector.run_loop())") > main_code.index("TcpListener::bind"):
        raise AssertionError("scheduler must be spawned before listener bind")

    # Population: pending activation already depends on its exact CMK; active
    # and partial remain encryption-live. Malformed rows are rejected.
    population_query = _first_query_argument(
        runtime_production,
        "async fn list_active_byok_keys(",
    )
    population_sql = _rust_string_literals(population_query)
    require(population_sql, "tenant_byok_config", "authoritative population table")
    require(
        population_sql,
        "state IN ('pending', 'active', 'partial')",
        "closed dependent-CMK population",
    )
    require(runtime_code, "required_text(&row", "row identity validation")
    require(population_sql, "cmk_key_id", "key identity validation")
    require(population_sql, "cmk_region", "key region validation")
    require(population_sql, "a.source_cmk_provider", "pinned source CMK census")
    require(population_sql, "tenant_byok_secret_history", "pinned source TCS authority")
    require(runtime_code, "provider_from_db", "provider parse guard")
    require(runtime_code, "parsed_provider != provider", "returned provider mismatch guard")
    require(runtime_code, "if rows.is_empty()", "zero-row guards")
    require(runtime_code, "tracing::info!", "audit transaction contract")
    if "INSERT INTO customer_audit_events" in runtime_code:
        raise AssertionError("runtime alerter must not issue a second non-atomic audit INSERT")

    # The native D1 REST client has one statement per request. The trigger is
    # therefore the transaction boundary for the tenant update + activity row.
    require(migration_sql, "AFTER UPDATE OF byok_status ON tenant", "audit trigger")
    require(migration_sql, "INSERT INTO customer_audit_events", "transactional audit insert")
    require(migration_sql, "WHEN OLD.byok_status IS NOT NEW.byok_status", "audit transition guard")
    _verify_sqlite_trigger_semantics(migration)

    # Detector refuses an unconfigured production loop and rejects a provider
    # mismatch instead of silently checking another provider's keys.
    require(detector_code, "if !self.source_configured", "unconfigured loop guard")
    require(detector_code, "source_configured", "explicit source witness")
    require(detector_code, "key.provider != provider_kind", "provider population binding")
    require(detector_code, "self.handle_ok_status(provider, &key_id).await?", "recovery propagation")
    require(detector_code, "self.handle_revocation(provider, &key_id).await?", "revocation propagation")
    require(detector_code, "self.store.current_status(key_id).await?", "recovery status propagation")
    require(detector_code, "self.store.restore_active(key_id, restored_at_ms).await?", "restore propagation")
    require(detector_code, ".alert_recovery(", "recovery audit/alert")
    if "let _ = self.store.restore_active" in detector_code or "let _ = self\n                    .alerter" in detector_code:
        raise AssertionError("recovery failure is swallowed")
    if detector_code.index("alert_recovery(") > detector_code.index("info!(event = ?audit_event"):
        raise AssertionError("recovery is logged before audit/alert succeeds")

    # Behavioral focal is real async Rust coverage, not a token-only test.
    require(focal_code, "revoked_active_key_reaches_the_real_kill_switch", "revocation behavior")
    require(focal_code, "population_failure_is_not_coerced_to_no_keys", "source failure behavior")
    require(focal_code, "an_unconfigured_scheduler_does_not_start_a_vacuous_loop", "lifecycle behavior")
    require(focal_code, "zero_updated_rows_fail_closed_in_the_kill_switch", "zero-row behavior")
    require(focal_code, "revocation_alert_failure_reaches_the_cycle_result", "alert failure behavior")
    require(focal_code, "recovery_status_restore_and_audit_failures_reach_the_cycle_result", "recovery failure behavior")
    require(focal_code, ".with_key_source(source)", "focal source wiring")
    if "reqwest" in focal or "D1HttpClient" in focal:
        raise AssertionError("focal must remain hermetic; it must not call D1/provider network")


def verify_fenced_control(control: str, admin: str, runtime: str) -> None:
    """Pin the production fence and pending-only activation architecture."""
    control_code = _mask_rust_comments_and_strings(control)
    control_sql = _rust_string_literals(control)
    admin_code = _mask_rust_comments_and_strings(admin)
    runtime_code = _mask_rust_comments_and_strings(runtime)
    require(control_sql, "byok_transition_commit_guard", "atomic transition assertion")
    require(control_code, "prepare_activation", "pending activation entrypoint")
    require(control_sql, "'pending'", "pending activation state")
    require(control_code, "D1BatchStatement::new", "atomic D1 batch")
    require(control_code, "resolve_active_tenants", "tenant-granular key resolution")
    require(control_code, "cmk_region", "full CMK identity")
    require(control_code, "validate_rotation_boundary", "provider/region rotation boundary")
    require(control_code, "commit_activation_status_transition", "live activation control path")
    require(control_sql, "UPDATE byok_data_intent SET outcome='expired'", "priority data-intent expiry")
    require(control_sql, "byok_activation_suspension_postcondition", "atomic suspension assertion")
    require(control_sql, "publication_gate_epoch=CASE WHEN phase<>'copy'", "partial/purge restore resnapshot")
    require(control_sql, "transition_token=?1", "fresh activation fence rebind")
    require(control_code, "active_config_identity", "legacy active config authority")
    require(control_sql, "s.tcs_version=?12", "in-batch current TCS CAS")
    require(control_sql, "a.source_cmk_provider=?13", "source-key priority authorization")
    require(control_sql, "byok_activation_key_health", "dual-key recovery health")
    require(runtime_code, "D1FencedTenantStatusStore", "fenced production status adapter")
    factory = _function_source(runtime, "pub fn detector_for_client(")
    require(factory, "D1FencedTenantStatusStore::new", "production fenced detector")
    if "D1TenantStatusStore::new" in factory:
        raise AssertionError("legacy bulk status adapter is wired in production")
    require(admin_code, "writer.prepare_activation", "admin pending-only activation")
    require(admin_code, "StatusCode::ACCEPTED", "pending activation response")
    if "writer.activate(" in admin_code:
        raise AssertionError("admin route publishes activation directly")


def verify_rotation_dependency_resolution(control: str) -> None:
    """Execute the production tenant-resolution SQL for target and source keys."""
    sql = _rust_string_literals(
        _first_query_argument(control, "pub async fn resolve_active_tenants(")
    ).replace("\\", " ")
    db = sqlite3.connect(":memory:")
    db.executescript(
        """
        CREATE TABLE tenant(tenant_id TEXT PRIMARY KEY, byok_status TEXT);
        CREATE TABLE tenant_byok_config(tenant_id TEXT, state TEXT, cmk_provider TEXT,
          cmk_key_id TEXT, cmk_region TEXT, config_version INTEGER);
        CREATE TABLE tenant_byok_secret(tenant_id TEXT, tcs_version INTEGER,
          cmk_key_id TEXT, tcs_wrapped BLOB);
        CREATE TABLE byok_activation_intent(tenant_id TEXT, phase TEXT,
          source_generation INTEGER, source_cmk_provider TEXT,
          source_cmk_key_id TEXT, source_cmk_region TEXT);
        INSERT INTO tenant VALUES('tenant-rotation','active');
        INSERT INTO tenant_byok_config VALUES
          ('tenant-rotation','pending','aws','target-key','us-east-1',2),
          ('tenant-legacy','active','aws','legacy-key','us-east-1',9);
        INSERT INTO tenant_byok_secret VALUES
          ('tenant-legacy',4,'legacy-key',X'01');
        INSERT INTO byok_activation_intent VALUES
          ('tenant-rotation','copy',7,'aws','source-key','us-east-1');
        """
    )
    for key in ("source-key", "target-key"):
        rows = db.execute(sql, ("aws", key, "us-east-1")).fetchall()
        if rows != [("tenant-rotation",)]:
            raise AssertionError(f"{key} did not resolve the live rotation tenant: {rows}")

    active_sql = _rust_string_literals(
        _first_query_argument(control, "async fn active_config_identity(")
    ).replace("\\", " ")
    legacy = db.execute(active_sql, ("tenant-legacy",)).fetchone()
    if legacy != (9, "aws", "legacy-key", "us-east-1", 4):
        raise AssertionError(f"legacy active tenant bypassed current custody authority: {legacy}")

    db.executescript(
        """
        CREATE TABLE byok_activation_key_health(
          intent_id TEXT, dependency_role TEXT, access_state TEXT);
        INSERT INTO byok_activation_key_health VALUES
          ('intent-a','source','unavailable'),('intent-a','target','healthy');
        """
    )
    if db.execute("SELECT NOT EXISTS(SELECT 1 FROM byok_activation_key_health WHERE intent_id='intent-a' AND access_state='unavailable')").fetchone()[0] != 0:
        raise AssertionError("restore admitted while the pinned source key was unavailable")
    db.execute("UPDATE byok_activation_key_health SET access_state='healthy'")
    if db.execute("SELECT NOT EXISTS(SELECT 1 FROM byok_activation_key_health WHERE intent_id='intent-a' AND access_state='unavailable')").fetchone()[0] != 1:
        raise AssertionError("restore remained blocked after both dependencies recovered")


def mutation_self_test(main: str, runtime: str, detector: str, focal: str, migration: str) -> None:
    def insert_before_test_include(source: str, bait: str) -> str:
        """Insert bait at the production/test include boundary.

        The runtime adapter's tests live in the separately included
        ``byok_revocation_runtime/part-01.rs`` file.  Looking for a local
        ``#[cfg(test)]`` marker worked only while those tests happened to be
        in this file; after the split, the mutation fixture failed before it
        could exercise the verifier.  Keep the boundary assertion fail-closed
        by anchoring to the executable include itself.
        """
        marker = TEST_INCLUDE_MARKER
        if marker not in source:
            raise AssertionError("B083 mutation fixture lost the production/test include boundary")
        return source.replace(marker, bait + "\n" + marker, 1)

    inactive_population = runtime.replace(
        "state IN ('pending', 'active', 'partial')", "state IN ('inactive')", 1
    )
    mutations = {
        "test-include-commented": (
            main,
            runtime.replace(TEST_INCLUDE_MARKER, "// " + TEST_INCLUDE_MARKER, 1),
            detector,
            focal,
            migration,
        ),
        "test-include-string-bait": (
            main,
            runtime.replace(
                TEST_INCLUDE_MARKER,
                'const INCLUDE_BAIT: &str = "include!(\\"byok_revocation_runtime/part-01.rs\\");";',
                1,
            ),
            detector,
            focal,
            migration,
        ),
        "test-include-cfg-any": (
            main,
            runtime.replace(
                TEST_INCLUDE_MARKER,
                "#[cfg(any())]\n" + TEST_INCLUDE_MARKER,
                1,
            ),
            detector,
            focal,
            migration,
        ),
        "test-include-cfg-test": (
            main,
            runtime.replace(
                TEST_INCLUDE_MARKER,
                "#[cfg(test)]\n" + TEST_INCLUDE_MARKER,
                1,
            ),
            detector,
            focal,
            migration,
        ),
        "test-include-cfg-attr-all-test": (
            main,
            runtime.replace(
                TEST_INCLUDE_MARKER,
                "#[cfg_attr(all(), cfg(test))]\n" + TEST_INCLUDE_MARKER,
                1,
            ),
            detector,
            focal,
            migration,
        ),
        "spawn-empty-future": (main.replace("tokio::spawn(detector.run_loop())", "tokio::spawn(async {})", 1), runtime, detector, focal, migration),
        "population-empty-state": (main, inactive_population, detector, focal, migration),
        "focal-source-removed": (main, runtime, detector, focal.replace(".with_key_source(source)", "", 1), migration),
        "loop-guard-removed": (main, runtime, detector.replace("if !self.source_configured {", "if false {", 1), focal, migration),
        "audit-trigger-removed": (main, runtime, detector, focal, migration.replace("AFTER UPDATE OF byok_status ON tenant", "AFTER INSERT ON tenant", 1)),
        "zero-row-guard-removed": (main, runtime.replace("if rows.is_empty() {", "if false && rows.is_empty() {"), detector, focal, migration),
        "recovery-errors-swallowed": (main, runtime, detector.replace("self.handle_ok_status(provider, &key_id).await?", "self.handle_ok_status(provider, &key_id).await", 1), focal, migration),
        # A required query/guard copied into comments or a fixture string is
        # not executable evidence and must not make the verifier green.
        "sql-comment-bait": (main, insert_before_test_include(inactive_population, "// state IN ('active', 'partial')"), detector, focal, migration),
        "sql-string-bait": (
            main,
            insert_before_test_include(
                inactive_population,
                'const STRING_BAIT: &str = "state IN (\'active\', \'partial\')";',
            ),
            detector,
            focal,
            migration,
        ),
        "rust-comment-bait": (main, runtime, detector.replace("if !self.source_configured {", "if false {", 1) + "\n/* if !self.source_configured { */\n", focal, migration),
    }
    for label, candidate in mutations.items():
        try:
            verify(*candidate)
        except AssertionError:
            continue
        raise AssertionError(f"mutation was accepted as green: {label}")


def main() -> None:
    texts = [path.read_text(encoding="utf-8") for path in (MAIN, RUNTIME, DETECTOR, FOCAL, MIGRATION)]
    verify(*texts)
    mutation_self_test(*texts)
    control = CONTROL.read_text(encoding="utf-8")
    admin = ADMIN.read_text(encoding="utf-8")
    runtime = RUNTIME.read_text(encoding="utf-8")
    verify_fenced_control(control, admin, runtime)
    verify_rotation_dependency_resolution(control)
    for label, mutated_control, mutated_admin, mutated_runtime in (
        ("fenced-adapter", control, admin, runtime.replace("D1FencedTenantStatusStore::new", "D1TenantStatusStore::new", 1)),
        ("pending-state", control.replace("'pending'", "'active'"), admin, runtime),
        ("pending-route", control, admin.replace("writer.prepare_activation", "writer.activate", 1), runtime),
    ):
        try:
            verify_fenced_control(mutated_control, mutated_admin, mutated_runtime)
        except AssertionError:
            continue
        raise AssertionError(f"fenced-control mutation was accepted as green: {label}")
    print("B083 revocation wiring PASS: durable source, pre-bind run_loop, focal behavior, mutations")


if __name__ == "__main__":
    main()
