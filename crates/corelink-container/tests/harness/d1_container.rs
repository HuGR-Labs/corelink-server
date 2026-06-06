//! Wave-30 stream-9 — in-process D1 surrogate for the live-D1 signup
//! integration tests.
//!
//! ## Why an in-process surrogate (not a real Docker container)
//!
//! D1 is SQLite at the storage layer. Cloudflare's Workers runtime
//! wraps a SQLite engine + a regional sync layer, but the per-row
//! semantics (transactions, UNIQUE indexes, CHECK constraints) are
//! stock SQLite. The wave-29 stream-1 signup route writes via the
//! [`SignupStore`] trait-object surface, so for the live-D1
//! regression all we need is a real SQLite instance plus the
//! canonical migration applied.
//!
//! We deliberately avoid the wave-22 testcontainers-style pattern
//! (`crates/corelink-audit-chain/tests/harness/pg_container.rs`) for
//! this stream because:
//!
//! 1. Postgres-the-image is a third-party binary distribution we have
//!    to fetch + sandbox per CI lane. SQLite ships in-process,
//!    `bundled` via rusqlite — zero infra fan-out, zero Docker
//!    requirement on the developer laptop.
//! 2. The D1 surface we exercise here (CREATE TABLE + UNIQUE INDEX +
//!    CHECK constraint + INSERT) is byte-identical between SQLite-
//!    in-process and D1-on-Cloudflare. The Postgres harness has to
//!    compensate for `set_config(...)` RLS + JSONB casts because
//!    those features are Postgres-only; D1-vs-SQLite has no such
//!    delta on the signup table.
//! 3. The R2-13 migration replay harness
//!    (`crates/corelink-d1-migrations`) already established the
//!    "rusqlite + bundled" pattern as the canonical offline D1
//!    surrogate; this module reuses the same pin.
//!
//! The audit doc captures this as a deliberate design call —
//! `specs/_audits/sealed/2026-05-16-signup-live-d1-tests.md` §2.
//!
//! ## Migration surface
//!
//! The harness applies [`PILOT_SIGNUPS_MIGRATION_PATH`] —
//! `migrations/d1/0053_pilot_signups.sql` — which declares the
//! `pilot_signups` table, two UNIQUE indexes (`email` and
//! `token_id`), and one composite index on `(state, signed_up_at)`.
//! The migration is the only one this harness exercises because the
//! route only writes to that table.
//!
//! ## Connection model
//!
//! [`D1Harness`] owns a [`tempfile::NamedTempFile`] (so SQLite is
//! backed by a real on-disk file — the WAL + journal modes are
//! identical to a D1 deployment); the rusqlite [`rusqlite::Connection`]
//! is wrapped in a [`std::sync::Mutex`] so the harness can satisfy
//! the `SignupStore: Send + Sync` bound. The route is sync at the
//! trait boundary so we don't need an async pool.
//!
//! Cleanup: dropping the harness drops the tempfile, which removes
//! the SQLite database file. The [`SqliteSignupStore`] holds an
//! [`Arc<Mutex<Connection>>`] so the connection is closed when the
//! last [`Arc`] reference goes away.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code,
    reason = "test harness: full surface kept for future regression hooks"
)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use corelink_server::routes::signup::{PilotSignupRecord, SignupStore};
use rusqlite::{params, Connection, OpenFlags};
use tempfile::NamedTempFile;
use uuid::Uuid;

/// Path to the canonical wave-29 stream-1 pilot-signup D1 migration,
/// resolved at compile time so the harness does not depend on the
/// current working directory at runtime.
pub const PILOT_SIGNUPS_MIGRATION_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../migrations/d1/0053_pilot_signups.sql"
);

/// Live D1 surrogate handle. Drop removes the backing tempfile.
pub struct D1Harness {
    /// Held for its `Drop` impl — when the harness leaves scope, the
    /// tempfile is unlinked from the filesystem and the SQLite
    /// database disappears. Wrapped in `Option` only so a future
    /// `into_inner`-style hook can take ownership.
    _file: Option<NamedTempFile>,
    /// On-disk path of the SQLite database. Exposed so a test can
    /// open a second connection to the SAME database (e.g. to assert
    /// that a write made via the route is visible from outside the
    /// route's connection).
    pub path: PathBuf,
    /// The single shared connection used by [`SqliteSignupStore`].
    /// SQLite serialises writes on the file lock, and the rusqlite
    /// `Connection` is `!Sync` — so we own one connection and gate
    /// it behind a `Mutex` per the trait-object's `Send + Sync`
    /// requirements.
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for D1Harness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1Harness")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl D1Harness {
    /// Spawn a fresh tempfile-backed SQLite database, apply the
    /// canonical `0053_pilot_signups.sql` migration, and return the
    /// handle. Panics on any infra failure — every error is a hard
    /// test-infra issue.
    #[must_use]
    pub fn spawn() -> Self {
        let file = NamedTempFile::new().expect("NamedTempFile::new");
        let path = file.path().to_path_buf();
        let conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI,
        )
        .expect("rusqlite open");
        // Enable FK enforcement so any future migration that adds
        // foreign keys surfaces ordering bugs. The `pilot_signups`
        // migration declares no FKs yet but the pragma is harmless
        // and matches D1's default.
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("PRAGMA foreign_keys");
        let migration = std::fs::read_to_string(PILOT_SIGNUPS_MIGRATION_PATH)
            .unwrap_or_else(|e| panic!("read migration {PILOT_SIGNUPS_MIGRATION_PATH}: {e}"));
        conn.execute_batch(&migration)
            .expect("apply 0053_pilot_signups migration");
        Self {
            _file: Some(file),
            path,
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// Re-apply the canonical migration. Because the migration uses
    /// `CREATE TABLE IF NOT EXISTS` + `CREATE UNIQUE INDEX IF NOT
    /// EXISTS` it is idempotent. Used by the migration-idempotency
    /// test.
    pub fn reapply_migration(&self) -> Result<(), String> {
        let migration = std::fs::read_to_string(PILOT_SIGNUPS_MIGRATION_PATH)
            .map_err(|e| format!("read migration: {e}"))?;
        let g = self
            .conn
            .lock()
            .map_err(|_| "D1Harness conn mutex poisoned".to_string())?;
        g.execute_batch(&migration)
            .map_err(|e| format!("reapply migration: {e}"))
    }

    /// Count rows in the `pilot_signups` table. Used by every test
    /// that asserts on row count.
    pub fn count_rows(&self) -> Result<i64, String> {
        let g = self
            .conn
            .lock()
            .map_err(|_| "D1Harness conn mutex poisoned".to_string())?;
        let count: i64 = g
            .query_row("SELECT COUNT(*) FROM pilot_signups", [], |r| r.get(0))
            .map_err(|e| format!("count_rows: {e}"))?;
        Ok(count)
    }

    /// Fetch every row in `pilot_signups` ordered by `signed_up_at`.
    /// Used by tests that assert on schema-level field persistence
    /// (tenant_id, email, state).
    pub fn list_rows(&self) -> Result<Vec<PersistedRow>, String> {
        let g = self
            .conn
            .lock()
            .map_err(|_| "D1Harness conn mutex poisoned".to_string())?;
        let mut stmt = g
            .prepare(
                "SELECT id, tenant_id, email, company_name, tier_hint, \
                 expected_use_case, signed_up_at, state, token_id \
                 FROM pilot_signups ORDER BY signed_up_at ASC",
            )
            .map_err(|e| format!("prepare list_rows: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PersistedRow {
                    id: r.get::<_, String>(0)?,
                    tenant_id: r.get::<_, String>(1)?,
                    email: r.get::<_, String>(2)?,
                    company_name: r.get::<_, String>(3)?,
                    tier_hint: r.get::<_, String>(4)?,
                    expected_use_case: r.get::<_, String>(5)?,
                    signed_up_at: r.get::<_, i64>(6)?,
                    state: r.get::<_, String>(7)?,
                    token_id: r.get::<_, String>(8)?,
                })
            })
            .map_err(|e| format!("query_map list_rows: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| format!("row: {e}"))?);
        }
        Ok(out)
    }

    /// Build a [`SqliteSignupStore`] wired to this harness's
    /// connection. Cloning is cheap (`Arc`).
    #[must_use]
    pub fn store(&self) -> SqliteSignupStore {
        SqliteSignupStore {
            conn: Arc::clone(&self.conn),
            injected_failure: Arc::new(Mutex::new(None)),
        }
    }
}

/// Decoded row shape for [`D1Harness::list_rows`]. Mirrors the
/// `pilot_signups` schema literally (every column TEXT or INTEGER).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedRow {
    /// `id` PK (UUID v7 as TEXT).
    pub id: String,
    /// `tenant_id` (UUID v7 as TEXT).
    pub tenant_id: String,
    /// `email` (operator outreach surface; plaintext per wave-29
    /// stream-1 audit-doc §7 PII note).
    pub email: String,
    /// `company_name` free-text.
    pub company_name: String,
    /// `tier_hint` free-text.
    pub tier_hint: String,
    /// `expected_use_case` free-text.
    pub expected_use_case: String,
    /// `signed_up_at` (Unix epoch ms).
    pub signed_up_at: i64,
    /// `state` lifecycle marker — always `"RESERVED"` at insertion.
    pub state: String,
    /// `token_id` (16-hex random body of the source pilot token).
    pub token_id: String,
}

/// Production-shaped [`SignupStore`] backed by a rusqlite SQLite
/// connection. The harness uses this for the live-D1 regression
/// suite; production wiring uses the wave-29 follow-on D1 binding
/// (deferred to the PRR ship gate per the wave-29 audit doc §12).
#[derive(Clone)]
pub struct SqliteSignupStore {
    conn: Arc<Mutex<Connection>>,
    /// Injection seam mirroring [`InMemorySignupStore::inject_failure`]
    /// so the fail-CLOSED test can drive a 503 without tampering with
    /// the underlying SQLite file (which would couple this to SQLite
    /// internals).
    injected_failure: Arc<Mutex<Option<&'static str>>>,
}

impl std::fmt::Debug for SqliteSignupStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteSignupStore").finish_non_exhaustive()
    }
}

impl SqliteSignupStore {
    /// Inject a static failure to drive the 503 regression. Mirrors
    /// the in-memory store's seam so the live-D1 tests can stay 1:1
    /// with the wave-29 in-memory baseline.
    pub fn inject_failure(&self, msg: &'static str) -> Result<(), &'static str> {
        let mut g = self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        *g = Some(msg);
        Ok(())
    }
}

impl SignupStore for SqliteSignupStore {
    fn insert_or_existing(
        &self,
        record: PilotSignupRecord,
    ) -> Result<PilotSignupRecord, &'static str> {
        // 0) Honour injected failure (drives the 503 fail-CLOSED arm).
        let injected = *self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        if let Some(msg) = injected {
            return Err(msg);
        }

        let g = self
            .conn
            .lock()
            .map_err(|_| "sqlite signup store mutex poisoned")?;

        // 1) Idempotency lookup — same email OR same token_id
        //    returns the existing row verbatim (mirrors the wave-29
        //    in-memory store's contract).
        //
        //    UNIQUE indexes on (email) and (token_id) make this query
        //    O(1). We hand-roll the lookup rather than INSERT-OR-
        //    ABORT-then-SELECT because the route wants the EXISTING
        //    record (not an error) and we need the SAME `tenant_id`
        //    + `id` the original insertion produced.
        if let Some(existing) = lookup_existing(&g, &record.email, &record.token_id)? {
            return Ok(existing);
        }

        // 2) Insert the new reservation atomically. The transaction
        //    ensures that a transient SQLite failure mid-INSERT
        //    leaves no half-written row. (SQLite implicit-txn
        //    semantics already give us this for a single-statement
        //    INSERT, but the explicit BEGIN..COMMIT documents intent
        //    and makes the rollback-on-error regression test
        //    self-explanatory.)
        g.execute("BEGIN", []).map_err(|_| "sqlite begin failed")?;
        let exec_result = g.execute(
            "INSERT INTO pilot_signups ( \
                 id, tenant_id, email, company_name, tier_hint, \
                 expected_use_case, signed_up_at, state, token_id \
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.id.to_string(),
                record.tenant_id.to_string(),
                record.email,
                record.company_name,
                record.tier_hint,
                record.expected_use_case,
                i64::try_from(record.signed_up_at_ms).unwrap_or(i64::MAX),
                record.state,
                record.token_id,
            ],
        );
        match exec_result {
            Ok(_) => {
                g.execute("COMMIT", [])
                    .map_err(|_| "sqlite commit failed")?;
            }
            Err(e) => {
                // Best-effort rollback — if the rollback itself errors
                // there is nothing useful we can do at this layer; the
                // outer test fail-CLOSED path will surface the original
                // failure as a 503.
                let _ = g.execute("ROLLBACK", []);
                let msg = e.to_string();
                if msg.contains("UNIQUE constraint failed")
                    && (msg.contains("idx_pilot_signups_email")
                        || msg.contains("idx_pilot_signups_token_id")
                        || msg.contains("pilot_signups.email")
                        || msg.contains("pilot_signups.token_id"))
                {
                    // Concurrent racer landed first — re-run the
                    // idempotent lookup and return that record.
                    if let Some(existing) = lookup_existing(&g, &record.email, &record.token_id)? {
                        return Ok(existing);
                    }
                    return Err("sqlite race window: unique-fail without follow-up row");
                }
                return Err("sqlite insert failed");
            }
        }

        // 3) Return the canonical record — same shape as the in-
        //    memory store so the route's downstream audit emit sees
        //    identical fields regardless of backend.
        Ok(record)
    }
}

/// Look up an existing pilot signup by `email` OR `token_id`.
/// Returns the first matching row (the UNIQUE indexes guarantee at
/// most one).
fn lookup_existing(
    conn: &Connection,
    email: &str,
    token_id: &str,
) -> Result<Option<PilotSignupRecord>, &'static str> {
    let mut stmt = conn
        .prepare(
            "SELECT id, tenant_id, email, company_name, tier_hint, \
             expected_use_case, signed_up_at, state, token_id \
             FROM pilot_signups \
             WHERE email = ?1 OR token_id = ?2 \
             LIMIT 1",
        )
        .map_err(|_| "sqlite prepare lookup failed")?;
    let mut rows = stmt
        .query(params![email, token_id])
        .map_err(|_| "sqlite query lookup failed")?;
    let next = rows.next().map_err(|_| "sqlite step lookup failed")?;
    match next {
        None => Ok(None),
        Some(r) => {
            let id_str: String = r.get(0).map_err(|_| "sqlite col id")?;
            let tenant_str: String = r.get(1).map_err(|_| "sqlite col tenant_id")?;
            let id = Uuid::parse_str(&id_str).map_err(|_| "sqlite id not UUID")?;
            let tenant_id =
                Uuid::parse_str(&tenant_str).map_err(|_| "sqlite tenant_id not UUID")?;
            let signed_up_at_i64: i64 = r.get(6).map_err(|_| "sqlite col signed_up_at")?;
            let signed_up_at_ms = u64::try_from(signed_up_at_i64).unwrap_or(0);
            Ok(Some(PilotSignupRecord {
                id,
                tenant_id,
                email: r.get(2).map_err(|_| "sqlite col email")?,
                company_name: r.get(3).map_err(|_| "sqlite col company_name")?,
                tier_hint: r.get(4).map_err(|_| "sqlite col tier_hint")?,
                expected_use_case: r.get(5).map_err(|_| "sqlite col expected_use_case")?,
                signed_up_at_ms,
                token_id: r.get(8).map_err(|_| "sqlite col token_id")?,
                state: r.get(7).map_err(|_| "sqlite col state")?,
            }))
        }
    }
}
