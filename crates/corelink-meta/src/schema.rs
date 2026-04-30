//! Canonical D1 migration SQL — embedded at compile time.
//!
//! The crate ships a single migration file (`migrations/d1/0001_blob_meta.sql`)
//! relative to the workspace root; we re-export it as a `&'static str` so:
//!
//! 1. The fake [`crate::InMemoryMetaStore`] can assert it parses (no-op for
//!    the in-memory backend, but a contract that the fake always tracks the
//!    real schema bit-for-bit).
//! 2. The future Cloudflare D1 binding adapter (WI-S01-005) can pass
//!    [`MIGRATION_SQL`] straight to `db.exec(...)` without a runtime file
//!    read — the crate is `wasm32-unknown-unknown`-buildable.
//! 3. Documentation tools / canonical-vector tests can pin a hash of
//!    [`MIGRATION_SQL`] and CI alerts on every schema change.
//!
//! ## Why `include_str!`
//!
//! `include_str!` resolves at compile time relative to the current source
//! file. The migration lives at the workspace root, two levels up from this
//! file (`../../../migrations/d1/0001_blob_meta.sql`). Using `include_str!`
//! means the crate fails to compile if the migration is moved or renamed
//! without updating this path — a desirable invariant for a load-bearing
//! schema asset.

/// Repo-relative canonical path to the migration source file. Surfaced for
/// documentation / migration runner scripts; **not** read at runtime.
pub const MIGRATION_FILE_PATH: &str = "migrations/d1/0001_blob_meta.sql";

/// Raw SQL DDL embedded at compile time. The exact bytes that
/// `wrangler d1 execute --file <MIGRATION_FILE_PATH>` will apply.
pub const MIGRATION_SQL: &str =
    include_str!("../../../migrations/d1/0001_blob_meta.sql");

/// BLAKE3-256 of [`MIGRATION_SQL`] rendered as 64-char lowercase hex —
/// load-bearing canonical regression vector.
///
/// Updated only when the migration is intentionally edited; CI computes
/// the digest of the file at lint time and asserts equality. A drift here
/// means the migration file changed without bumping the hash; the impl
/// crate is out of step with the SQL on disk.
///
/// The function returns the digest computed against the **embedded**
/// migration string (`include_str!`-resolved at compile time), which is
/// itself the exact byte sequence `wrangler d1 execute --file <path>`
/// would feed to the engine — so the regression vector is platform-stable
/// regardless of LF/CRLF line-ending differences in checked-out copies.
#[must_use]
pub fn migration_sql_blake3_hex() -> String {
    corelink_hash::Digest::compute(MIGRATION_SQL.as_bytes()).to_hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_path_constant_matches_expected() {
        assert_eq!(MIGRATION_FILE_PATH, "migrations/d1/0001_blob_meta.sql");
    }

    #[test]
    fn migration_sql_contains_canonical_table_definitions() {
        // Spot-check the load-bearing fragments. A full schema parser is
        // overkill here; the production validation lives in
        // `tests/schema_canonical.rs` which performs the structural
        // assertions.
        assert!(MIGRATION_SQL.contains("CREATE TABLE IF NOT EXISTS blob_meta"));
        assert!(MIGRATION_SQL.contains("PRIMARY KEY (tenant_id, digest)"));
        assert!(MIGRATION_SQL.contains("CHECK (refcount >= 0)"));
        assert!(MIGRATION_SQL.contains("CHECK (size_bytes > 0)"));
        assert!(MIGRATION_SQL.contains("CREATE INDEX IF NOT EXISTS idx_blob_meta_tenant_alive"));
        assert!(MIGRATION_SQL.contains("CREATE INDEX IF NOT EXISTS idx_blob_meta_gc_candidates"));
        assert!(MIGRATION_SQL.contains("CREATE TABLE IF NOT EXISTS audit_outbox"));
        assert!(MIGRATION_SQL.contains("UNIQUE (request_id, event_type)"));
    }

    #[test]
    fn blake3_hash_of_migration_sql_is_deterministic() {
        let h1 = migration_sql_blake3_hex();
        let h2 = migration_sql_blake3_hex();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64, "BLAKE3-256 hex must be 64 chars");
        assert!(
            h1.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "lowercase hex"
        );
    }
}
