//! Canonical SQL templates used by the [`crate::MetaStore`] implementations.
//!
//! Every D1 round-trip the crate makes is rooted in one of the constants
//! below. They are the **only** place SQL string literals live; the
//! production CF D1 binding adapter (WI-S01-005) calls
//! `db.prepare(<const>)` and `.bind(<scalars>)` — never string-formats user
//! input.
//!
//! ## Why this is a separate module
//!
//! 1. **Auditability.** A reviewer (codex / human) can scan one file to see
//!    every SQL statement issued against D1. Inline-string-literal-style
//!    queries scattered across `store.rs` would force a diff against the
//!    whole crate to spot a SQL-injection regression.
//! 2. **Future compile-time validation.** A future ADR may introduce a
//!    `sqlite3_prepare_v2` call against an in-memory SQLite to assert each
//!    template parses against the schema. Centralizing the templates here
//!    makes that flip purely additive.
//! 3. **No runtime allocation.** Every template is a `&'static str`; the
//!    only allocations on the hot path are the bind-parameter buffers
//!    themselves (the canonical text forms of `tenant_id` and `digest`).
//!
//! ## Bind parameter ordering
//!
//! D1 (SQLite) supports both `?N`-style numeric placeholders and `?`-style
//! sequential placeholders. We use `?N` exclusively because (a) it survives
//! template re-shuffling without silent argument swaps, (b) it reads as
//! self-documenting at the call site (`bind!(?1=tenant, ?2=digest, ...)`),
//! (c) the WI text uses `?N` ordering in its examples.

/// `INSERT OR IGNORE` for [`crate::types::BlobMetaKey`] — the load-bearing
/// idempotent INSERT (INV-CAS-IDEMPOTENCY + INV-CAS-IMMUTABILITY).
///
/// Bind:
/// - `?1` `tenant_id` (canonical UUIDv7 text)
/// - `?2` `digest` (canonical `'algo:hex'` text)
/// - `?3` `size_bytes` (i64; > 0 enforced by CHECK)
/// - `?4` `created_at` (i64 unix-ms)
/// - `?5` `last_accessed_at` (i64 unix-ms; equals `created_at` on first
///   write per WI §1.4)
///
/// Refcount default = 1 is hard-coded in the schema; we do **not** bind it
/// here — first-write semantics derive from the schema, not the call site.
pub const INSERT_BLOB_META: &str = "\
INSERT OR IGNORE INTO blob_meta \
(tenant_id, digest, size_bytes, created_at, last_accessed_at) \
VALUES (?1, ?2, ?3, ?4, ?5)";

/// Atomic refcount increment with `RETURNING refcount`. D1 single-statement
/// UPDATE is atomic at the storage engine level (WI §2 mitigation 2). The
/// `WHERE deleted_at IS NULL` clause prevents incrementing a tombstoned row
/// — that case is surfaced as zero rows updated by the impl, mapped to
/// [`crate::MetaError::Tombstoned`].
///
/// Bind: `?1` tenant_id text; `?2` digest text; `?3` last_accessed_at ms.
pub const UPDATE_BLOB_META_INCREMENT_REFCOUNT: &str = "\
UPDATE blob_meta \
SET refcount = refcount + 1, last_accessed_at = ?3 \
WHERE tenant_id = ?1 AND digest = ?2 AND deleted_at IS NULL \
RETURNING refcount";

/// Atomic refcount decrement with `RETURNING refcount`. The CHECK
/// constraint `refcount >= 0` enforces server-side that the new value is
/// non-negative; D1 returns a constraint violation on underflow which the
/// impl maps to [`crate::MetaError::RefcountUnderflow`].
///
/// Bind: `?1` tenant_id text; `?2` digest text; `?3` last_accessed_at ms.
pub const UPDATE_BLOB_META_DECREMENT_REFCOUNT: &str = "\
UPDATE blob_meta \
SET refcount = refcount - 1, last_accessed_at = ?3 \
WHERE tenant_id = ?1 AND digest = ?2 AND deleted_at IS NULL \
RETURNING refcount";

/// Soft-delete: set `deleted_at` to a unix-ms timestamp. Idempotent — the
/// `WHERE deleted_at IS NULL` clause means a second call no-ops at the SQL
/// layer; the impl surfaces this as a successful no-op (the row is already
/// tombstoned; semantically the user's intent is satisfied).
///
/// Bind: `?1` tenant_id text; `?2` digest text; `?3` deleted_at ms.
pub const UPDATE_BLOB_META_SOFT_DELETE: &str = "\
UPDATE blob_meta \
SET deleted_at = ?3 \
WHERE tenant_id = ?1 AND digest = ?2 AND deleted_at IS NULL";

/// SELECT a single row by its composite PK. Returns the row regardless of
/// `deleted_at` state — callers that want only alive rows filter the
/// returned [`crate::types::BlobMetaRow::is_alive`] themselves.
///
/// Bind: `?1` tenant_id text; `?2` digest text.
pub const SELECT_BLOB_META: &str = "\
SELECT tenant_id, digest, size_bytes, refcount, created_at, last_accessed_at, deleted_at \
FROM blob_meta \
WHERE tenant_id = ?1 AND digest = ?2";

/// Audit-outbox INSERT. Pairs atomically with every blob_meta mutation in
/// the same `db.batch([...])` call (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
/// `INSERT OR IGNORE` makes retried commits with the same
/// `(request_id, event_type)` a no-op, preserving idempotency at the audit
/// layer; the impl additionally checks payload equality for retried inserts
/// and surfaces [`crate::MetaError::AuditIdempotencyConflict`] when the
/// payload differs (request_id reuse across distinct events).
///
/// Bind: `?1` id text; `?2` tenant_id text; `?3` digest text or NULL;
/// `?4` request_id text; `?5` event_type text; `?6` payload_json text;
/// `?7` enqueued_at ms. `region` is NOT a bind — it is tagged from a correlated
/// subquery on `tenant.primary_region` so the row satisfies migration 0023's
/// residency trigger (`NEW.region != tenant.primary_region → RAISE(ABORT)`).
/// Relying on the `'wnam'` column default aborts the INSERT for any non-`wnam`
/// tenant (tenants default to `'enam'`, 0028) → fail-CLOSED 503 (incident
/// 2026-07-17); COALESCE `'wnam'` covers the tenant-absent case.
pub const INSERT_AUDIT_OUTBOX: &str = "\
INSERT OR IGNORE INTO audit_outbox \
(id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, region) \
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, \
        COALESCE((SELECT primary_region FROM tenant WHERE tenant_id = ?2), 'wnam'))";

/// Lookup an audit_outbox row by its idempotency key. Used by the impl to
/// detect "same request_id + event_type, different payload" before
/// committing the batch. Returns the existing payload (if any) so the
/// caller can compare bytes.
///
/// Bind: `?1` request_id text; `?2` event_type text.
pub const SELECT_AUDIT_OUTBOX_BY_REQUEST: &str = "\
SELECT id, tenant_id, digest, payload_json, enqueued_at, emitted_at \
FROM audit_outbox \
WHERE request_id = ?1 AND event_type = ?2";

#[cfg(test)]
mod tests {
    use super::*;

    /// Spot-check that no template contains a string concatenation marker
    /// or a literal `{}` — both of which would suggest someone interpolated
    /// user input rather than binding it.
    #[test]
    fn no_template_uses_string_interpolation_markers() {
        for (label, sql) in [
            ("INSERT_BLOB_META", INSERT_BLOB_META),
            (
                "UPDATE_BLOB_META_INCREMENT_REFCOUNT",
                UPDATE_BLOB_META_INCREMENT_REFCOUNT,
            ),
            (
                "UPDATE_BLOB_META_DECREMENT_REFCOUNT",
                UPDATE_BLOB_META_DECREMENT_REFCOUNT,
            ),
            ("UPDATE_BLOB_META_SOFT_DELETE", UPDATE_BLOB_META_SOFT_DELETE),
            ("SELECT_BLOB_META", SELECT_BLOB_META),
            ("INSERT_AUDIT_OUTBOX", INSERT_AUDIT_OUTBOX),
            (
                "SELECT_AUDIT_OUTBOX_BY_REQUEST",
                SELECT_AUDIT_OUTBOX_BY_REQUEST,
            ),
        ] {
            assert!(
                !sql.contains("{}"),
                "{label} contains string-interpolation marker '{{}}': SQL injection risk"
            );
            assert!(
                !sql.contains("||"),
                "{label} contains SQLite concatenation '||': review for parameter binding"
            );
            // Every template must contain at least one numeric placeholder.
            assert!(
                sql.contains("?1"),
                "{label} has no ?1 placeholder — sanity check"
            );
        }
    }

    #[test]
    fn refcount_increment_filters_tombstoned_rows() {
        // Load-bearing predicate: incrementing a tombstoned blob would
        // resurrect it (INV-CAS-IMMUTABILITY violation). The WHERE clause
        // protects against this; this test asserts the predicate is in the
        // template.
        assert!(UPDATE_BLOB_META_INCREMENT_REFCOUNT.contains("deleted_at IS NULL"));
        assert!(UPDATE_BLOB_META_DECREMENT_REFCOUNT.contains("deleted_at IS NULL"));
    }

    #[test]
    fn idempotent_insert_uses_or_ignore() {
        // INV-CAS-IDEMPOTENCY: a duplicate INSERT must be a silent no-op.
        assert!(INSERT_BLOB_META.starts_with("INSERT OR IGNORE"));
        assert!(INSERT_AUDIT_OUTBOX.starts_with("INSERT OR IGNORE"));
    }
}
