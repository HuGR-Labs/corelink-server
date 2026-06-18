//! Integration test: replay every `migrations/d1/*.sql` against an
//! in-memory SQLite database and assert there are no hard ordering
//! failures. Two allow-lists keep historical exceptions diff-visible:
//! [`KNOWN_HAZARDS`] (files that legitimately SKIP a statement under the
//! D1-IF-NOT-EXISTS rewrite) and [`PRE_EXISTING_FAILURES`] (files with a
//! pre-existing hard failure surfaced for the first time). Both are
//! currently EMPTY — every prior pin (KNOWN_HAZARDS 0023/0028/0031/0041/0064,
//! PRE_EXISTING_FAILURES 0027/0036/0037) was retired after the underlying
//! migration was corrected. Each list has a stale-entry guard that panics
//! if a pinned file no longer matches its pinned outcome, so dead pins
//! cannot silently accumulate.
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
///
/// STALE-ENTRY GUARD: like [`KNOWN_HAZARDS`], any file pinned here that
/// now replays cleanly (or skips instead of hard-failing) is flagged by
/// the `stale_failures` panic below so dead pins cannot silently
/// accumulate. The three original entries (0027 / 0036 / 0037) were
/// removed (2026-06-18) after a full in-memory replay showed all of them
/// now apply cleanly — the underlying migrations were corrected in
/// earlier schema-shape-unification waves (the same pass that retired the
/// old KNOWN_HAZARDS pins). The list is empty again until a genuinely
/// new pre-existing failure is surfaced.
const PRE_EXISTING_FAILURES: &[(&str, &str)] = &[];

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
    // Mirror of `missing_hazards` for the failure pin-list: any file
    // pinned in PRE_EXISTING_FAILURES that did NOT hard-fail this run is a
    // dead pin and must be removed (stale-entry guard, see panic #4).
    let mut stale_failures: Vec<String> = Vec::new();

    for (fname, outcome) in &report.per_file {
        let pinned_failure = PRE_EXISTING_FAILURES.iter().any(|(name, _)| name == fname);
        match outcome {
            MigrationOutcome::AppliedClean => {
                println!("  clean   {fname}");
                // If a file is now clean but was previously known
                // to skip, the allow-list is stale — surface it.
                if KNOWN_HAZARDS.iter().any(|(name, _)| name == fname) {
                    missing_hazards.push(fname.clone());
                }
                // Likewise: a clean file pinned as a pre-existing hard
                // failure is a stale pin.
                if pinned_failure {
                    stale_failures.push(fname.clone());
                }
            }
            MigrationOutcome::AppliedWithKnownD1Skip { skip_count } => {
                println!("  skip    {fname}  ({skip_count} stmt(s))");
                if !KNOWN_HAZARDS.iter().any(|(name, _)| name == fname) {
                    unexpected_hazards.push(fname.clone());
                }
                // A skip (not a hard failure) also means a
                // PRE_EXISTING_FAILURES pin no longer reflects reality.
                if pinned_failure {
                    stale_failures.push(fname.clone());
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

    // 4. If a file pinned as a PRE_EXISTING_FAILURE no longer hard-fails
    //    (it now applies cleanly or merely skips), the pin is dead and
    //    must be removed so stale failure-pins can't silently accumulate.
    //    Mirrors guard #3 for the KNOWN_HAZARDS list.
    if !stale_failures.is_empty() {
        panic!(
            "\nPRE_EXISTING_FAILURES pin-list is stale — these files no \
             longer hard-fail under in-memory replay and must be removed \
             from the list (the underlying migration was fixed):\n  {}",
            stale_failures.join("\n  ")
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
