//! Integration test: replay every `migrations/d1/*.sql` against an
//! in-memory SQLite database and assert there are no hard ordering
//! failures. Known schema dependency hazards (e.g. `0023` references
//! `billing_events_staging` which is never created by any migration in
//! the chain, `0031` references `tenants` plural where only `tenant`
//! singular is declared) are pinned in [`KNOWN_HAZARDS`] so future
//! regressions become diff-visible.
//!
//! Run:
//!     cargo test -p corelink-d1-migrations --test d1_migration_integration -- --nocapture

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::panic,
    clippy::indexing_slicing
)]

use corelink_ops::migrations::{
    list_migration_files, locate_migrations_dir, replay_all, MigrationOutcome,
};

/// Files where we currently expect at least one statement skip. Items
/// here are PINNED — adding a new one requires the WI author to record
/// the rationale (typically a schema dependency hazard pre-existing
/// before R2-13). The intent: no NEW hazards land silently.
const KNOWN_HAZARDS: &[(&str, &str)] = &[
    // W35-P2-OPS absorption surfaced that the 4 previously-pinned
    // hazards (0023, 0028, 0031, 0041) now apply cleanly to a fresh
    // in-memory SQLite. The corresponding upstream migrations were
    // patched in earlier waves (likely the schema-shape unification
    // pass that introduced `tenant` / `tenants` view aliases). Removing
    // stale pins per the test's "no stale entry" guard (line 174 panic).
    // Empty again as of #262: 0064 was previously pinned here because its
    // table-rebuild did not replay cleanly against the in-memory SQLite. The
    // 2026-06-13 prod-apply failure exposed the real cause — the rebuild's
    // `ALTER TABLE … RENAME` re-parsed residency triggers on other tables that
    // reference `tenant`, hitting "no such table: main.tenant". #262 fixes 0064
    // (`PRAGMA legacy_alter_table=ON` + recreate the `trg_tenant_*` triggers),
    // so it now replays cleanly — the test's "no stale entry" guard requires it
    // be removed from this list.
];

/// Files with hard failures that are PRE-EXISTING bugs in the migration
/// corpus, surfaced by R2-13 for the first time. These are NOT to be
/// fixed in R2-13 (out of scope per WI deliverables — R2-13 is about
/// building the runner + verify harness + integration test). Each
/// entry must record the failure mode so a follow-up WI can address it.
///
/// IMPORTANT: this is an explicit pin. Adding new entries here means
/// the migration corpus has regressed and a separate fix is needed.
const PRE_EXISTING_FAILURES: &[(&str, &str)] = &[
    (
        "0027_region_provisioning.sql",
        "CREATE TABLE region_migration_progress places PRIMARY KEY (tenant_id, \
         migration_run_id) BEFORE source_region column declaration. SQLite rejects \
         table-level constraints interleaved with column defs. Follow-up: move PK \
         to the end of the column list in a 0027b additive correction.",
    ),
    (
        "0036_oncall_pages.sql",
        "CREATE TABLE oncall_shifts interleaves CONSTRAINT clauses BETWEEN column defs \
         (correlation_id appears AFTER shift_positive_duration constraint). SQLite syntax \
         requires all column defs to precede table-level constraints. This will likely \
         also fail D1 if/when re-applied to a fresh database; the existing prod D1 may \
         have been hand-rolled. Follow-up: re-issue as 0036b additive migration.",
    ),
    (
        "0037_signup_orchestration.sql",
        "CREATE INDEX on tenant(email_hash) + tenant(tenant_state) — the `tenant` table \
         was first declared in 0023 with only {tenant_id, primary_region, created_at_ms, \
         updated_at_ms}. The CREATE TABLE IF NOT EXISTS in 0037 is a NO-OP because the \
         table already exists, so the new columns are not added. Follow-up: split into \
         ALTER TABLE ADD COLUMN statements per INV-AUTH-MIGRATION-ADDITIVE.",
    ),
];

#[test]
fn every_migration_replays_against_in_memory_sqlite() {
    let dir = locate_migrations_dir();
    assert!(
        dir.is_dir(),
        "migrations/d1/ not found at {dir:?} — harness must be run from workspace"
    );

    let files = list_migration_files(&dir).expect("list migration files");
    assert!(
        files.len() >= 40,
        "expected ≥40 D1 migrations, found {} — has the directory been truncated?",
        files.len()
    );

    let report = replay_all(&files).expect("replay should not error");

    println!(
        "\n=== D1 migration replay report ({} files) ===",
        report.total_files
    );
    println!(
        "  clean:        {}",
        report.total_files - report.files_with_skip - report.files_failed
    );
    println!("  with_skip:    {}", report.files_with_skip);
    println!("  hard_failed:  {}", report.files_failed);

    // Per-file breakdown for human eyeballs / CI logs.
    let mut unexpected_hazards: Vec<String> = Vec::new();
    let mut missing_hazards: Vec<String> = Vec::new();
    let mut unexpected_failures: Vec<String> = Vec::new();

    for (fname, outcome) in &report.per_file {
        match outcome {
            MigrationOutcome::AppliedClean => {
                println!("  clean   {fname}");
                // If a file is now clean but was previously known
                // to skip, the allow-list is stale — surface it.
                if KNOWN_HAZARDS.iter().any(|(name, _)| name == fname) {
                    missing_hazards.push(fname.clone());
                }
            }
            MigrationOutcome::AppliedWithKnownD1Skip { skip_count } => {
                println!("  skip    {fname}  ({skip_count} stmt(s))");
                if !KNOWN_HAZARDS.iter().any(|(name, _)| name == fname) {
                    unexpected_hazards.push(fname.clone());
                }
            }
            MigrationOutcome::FailedHardOrdering { failures } => {
                println!("  FAIL    {fname}");
                for f in failures {
                    println!("    -> {f}");
                }
                if !PRE_EXISTING_FAILURES.iter().any(|(name, _)| name == fname) {
                    unexpected_failures.push(fname.clone());
                }
            }
        }
    }

    // 1. No NEW hard failures. Pre-existing failures are pinned in
    //    PRE_EXISTING_FAILURES (out of scope for R2-13 to fix; see
    //    rationale in that const).
    if !unexpected_failures.is_empty() {
        panic!(
            "\n{} new D1 migration(s) hard-failed under in-memory replay:\n  {}\n\
             If this is a pre-existing bug surfaced for the first time, add the file \
             to PRE_EXISTING_FAILURES with a rationale and a follow-up plan. \
             If this is a NEW regression in your PR, fix the migration before merging.",
            unexpected_failures.len(),
            unexpected_failures.join("\n  ")
        );
    }

    // 2. Files that newly skip without being in KNOWN_HAZARDS = regression.
    if !unexpected_hazards.is_empty() {
        panic!(
            "\nUnexpected new schema dependency hazards introduced — \
             these migrations newly require statements that SQLite \
             rejects beyond the documented D1-IF-NOT-EXISTS rewrite:\n  {}\n\
             If this is intentional, add an entry to KNOWN_HAZARDS in \
             tests/d1_migration_integration.rs with rationale.",
            unexpected_hazards.join("\n  ")
        );
    }

    // 3. If a previously-hazardous file is now clean, the allow-list
    //    is stale — remove the entry to lock in the fix.
    if !missing_hazards.is_empty() {
        panic!(
            "\nKNOWN_HAZARDS allow-list is stale — these files now apply \
             cleanly and should be removed from the list:\n  {}",
            missing_hazards.join("\n  ")
        );
    }
}

#[test]
fn migration_count_meets_floor() {
    // R2-13 baseline: 41 migrations (0001..0043 with 0004/0005 gaps).
    // R2-12 may add 0044; floor is intentionally generous.
    let dir = locate_migrations_dir();
    let files = list_migration_files(&dir).expect("list");
    assert!(
        files.len() >= 41,
        "migration count regressed to {} (floor: 41)",
        files.len()
    );
}

#[test]
fn migration_filenames_match_canonical_pattern() {
    let dir = locate_migrations_dir();
    let files = list_migration_files(&dir).expect("list");
    for f in &files {
        let name = f
            .file_name()
            .expect("filename")
            .to_string_lossy()
            .to_string();
        // NNNN_lowercase_underscore.sql
        let prefix: String = name.chars().take(4).collect();
        assert!(
            prefix.chars().all(|c| c.is_ascii_digit()),
            "filename {name} does not start with 4-digit prefix"
        );
        assert!(name.ends_with(".sql"), "filename {name} not .sql");
        assert!(
            name.chars().nth(4) == Some('_'),
            "filename {name} missing `_` after numeric prefix"
        );
    }
}
