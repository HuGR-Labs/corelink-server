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
    runtime_production = runtime.split("#[cfg(test)]", 1)[0]
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

    # Population: no static empty query; active and partial are the only
    # encryption-live states and malformed rows are rejected.
    population_query = _first_query_argument(
        runtime_production,
        "async fn list_active_byok_keys(",
    )
    population_sql = _rust_string_literals(population_query)
    require(population_sql, "tenant_byok_config", "authoritative population table")
    require(population_sql, "state IN ('active', 'partial')", "closed active population")
    require(runtime_code, "required_text(&row", "row identity validation")
    require(population_sql, "cmk_key_id", "key identity validation")
    require(population_sql, "cmk_region", "key region validation")
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
        marker = 'include!("byok_revocation_runtime/part-01.rs");'
        if marker not in source:
            raise AssertionError("B083 mutation fixture lost the production/test include boundary")
        return source.replace(marker, bait + "\n" + marker, 1)

    inactive_population = runtime.replace("state IN ('active', 'partial')", "state IN ('inactive')", 1)
    mutations = {
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
    print("B083 revocation wiring PASS: durable source, pre-bind run_loop, focal behavior, mutations")


if __name__ == "__main__":
    main()
