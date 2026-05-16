-- Wave-19 schema lift — closes wave-18 caveat #4 on WI-S09-008.
--
-- WI-S09-008 / `apps/server/src/routes/audit_export.rs` ships the
-- customer-facing `/v1/audit/export` route with a route-level audit
-- sink (`ExportAuditSink`). Wave-15.3 wired the in-memory sink;
-- wave-18 added the mid-stream chain-break SEV-0 emit carrying the
-- canonical `{break_at_seq, break_at_chunk, observed, expected}`
-- payload colon-prefix-encoded into `exit_status` because the row
-- shape lacked a dedicated payload column. **Wave-19 lifts the
-- payload into a first-class `payload TEXT` column** so durable D1
-- sinks (the production target for the route's `ExportAuditSink`
-- impl) can persist the structured JSON map without smuggling it
-- through `exit_status`.
--
-- ## Schema baseline
--
-- The production durable sink for `ExportAuditRow` lifts into the
-- standard CloudEvents audit-chain envelope (the same envelope the
-- chain producer emits to R2; see WI-S09-004). The D1 mirror table
-- is operator-facing — analysts query it to slice the per-window
-- export audit (cross-tenant attempts, mid-stream chain breaks,
-- rate-limit denials) without paging the R2 archive. The table is
-- additive-only (INV-AUTH-MIGRATION-ADDITIVE + INV-AUDIT-APPEND-ONLY).
--
-- ## Idempotency
--
-- `CREATE TABLE IF NOT EXISTS` lets the migration replay safely on
-- a database that already has the table (wave-19 forward-compat with
-- the wave-15.3 baseline). The `payload TEXT` column is included in
-- the canonical baseline shape so a fresh database starts wave-19-
-- aware; an existing wave-18 database without the column lifts via
-- the companion migration `0049_export_audit_log_add_payload.sql`
-- below (D1 / SQLite supports `ALTER TABLE ADD COLUMN`; the column
-- defaults to NULL on existing rows which preserves the
-- backwards-compat `#[serde(default)]` round-trip on the
-- `ExportAuditRow::payload: Option<serde_json::Value>` field).

CREATE TABLE IF NOT EXISTS export_audit_log (
    -- Surrogate key for the audit row (D1 INTEGER PRIMARY KEY = rowid alias).
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Canonical CloudEvents `type` (one of `EVENT_TYPE_*` constants
    -- in `apps/server/src/routes/audit_export.rs`).
    event_type TEXT NOT NULL,
    -- Authenticated tenant id (UUID hex / canonical string). NULL
    -- if the request never reached the auth step (401 path).
    authenticated_tenant TEXT,
    -- Attempted tenant id (cross-tenant-attempt arm only). NULL
    -- elsewhere.
    attempted_tenant TEXT,
    -- Window lower bound (Unix epoch ms; inclusive).
    from_ms INTEGER NOT NULL,
    -- Window upper bound (Unix epoch ms; exclusive).
    to_ms INTEGER NOT NULL,
    -- NDJSON byte count flushed to the response stream.
    bytes_written INTEGER NOT NULL,
    -- Number of audit events flushed.
    events_written INTEGER NOT NULL,
    -- Canonical exit status enum. See WI-S09-008 §4 + the
    -- `EXIT_STATUS_*` constants in `audit_export.rs`.
    exit_status TEXT NOT NULL,
    -- **Wave-19 lift** — structured CloudEvents `data` payload as
    -- canonical JSON text. Today populated on the
    -- `verify_failed_mid_stream` arm with
    -- `{"break_at_seq":<u64>,"break_at_chunk":<u64>,"observed":"<hex>","expected":"<hex>"}`.
    -- D1 stores JSON as TEXT; deserialize via `serde_json::from_str`
    -- into `ExportAuditRow::payload: Option<serde_json::Value>`.
    -- NULL on every other arm (preserves the
    -- `skip_serializing_if = "Option::is_none"` discipline in the
    -- Rust shape).
    payload TEXT,
    -- Server-side emit wall-clock (Unix epoch ms). The CloudEvents
    -- envelope already carries `time`; we duplicate here so D1
    -- analytic queries don't need to peer inside `payload`.
    emitted_at_ms INTEGER NOT NULL DEFAULT 0
);

-- Index for the per-tenant per-window slice (the dominant analyst
-- query shape: "show me every audit-export emit for tenant T in the
-- last 24h"). Additive — `CREATE INDEX IF NOT EXISTS` is idempotent.
CREATE INDEX IF NOT EXISTS idx_export_audit_log_tenant_emitted
    ON export_audit_log (authenticated_tenant, emitted_at_ms);

-- Index for the SEV-0/SEV-1 paged emits (cross-tenant attempt +
-- verify-failed + mid-stream chain break). The audit-export
-- security analyst dashboard pages off this index.
CREATE INDEX IF NOT EXISTS idx_export_audit_log_event_type_emitted
    ON export_audit_log (event_type, emitted_at_ms);
