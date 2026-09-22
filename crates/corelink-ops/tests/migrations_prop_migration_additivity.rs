//! Property tests pinning the load-bearing additive-migration invariant
//! of `corelink-d1-migrations` (WI-PROPTEST-FU-004 — DEBT-009).
//!
//! Closes the proptest-density gap identified in
//! `specs/_audits/sealed/2026-05-15-proptest-density.md` (ratio 0/1 → 3/1).
//!
//! # Invariant coverage
//!
//! | Test                                                    | Invariant pinned                    |
//! |---------------------------------------------------------|-------------------------------------|
//! | `prop_inv_auth_migration_additive_no_drop_alter_rename` | INV-AUTH-MIGRATION-ADDITIVE         |
//! | `prop_inv_auth_migration_additive_filename_ordering`    | INV-AUTH-MIGRATION-ADDITIVE (order) |
//! | `prop_inv_auth_migration_additive_idempotent_create`    | INV-AUTH-MIGRATION-ADDITIVE (idem)  |
//!
//! Adversarial inputs covered:
//! - SQL comments containing forbidden keywords (`-- DROP this later`) MUST NOT
//!   trigger false positives (comment-aware lexer required).
//! - Mixed-case keywords (`Drop`, `dRoP`, `DROP`) MUST all be detected.
//! - Whitespace variations (`DROP\tTABLE`, `DROP\nTABLE`).
//! - File ordering perturbations — the canonical numeric sort MUST match
//!   the lexicographic sort (zero-padded 4-digit prefix guarantees this).
//!
//! The corpus is `migrations/d1/*.sql` (the workspace's canonical
//! migration directory; ≥ 41 files at audit time).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: panics surface as test failures by design"
)]

use std::fs;
use std::path::PathBuf;

use corelink_ops::migrations::{list_migration_files, locate_migrations_dir};
use proptest::prelude::*;
use proptest::test_runner::Config;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

// =====================================================================
// PROPTEST_CASES runtime knob (S-07 P1-2 contract — runtime fn, NOT const).
// =====================================================================

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256)
}

/// Comment-aware SQL lexer: strip `-- line comments` and `/* block
/// comments */` before keyword scanning. Returns the canonicalized
/// (comment-stripped, lowercase-ASCII-folded) SQL.
///
/// Intentionally simple: we only need to detect leading-keyword DROP /
/// ALTER COLUMN / RENAME statements; we do NOT need full SQL grammar.
fn strip_sql_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_str: Option<char> = None;

    while let Some(c) = chars.next() {
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                out.push(c); // preserve newline for line-number debugging
            }
            continue;
        }
        if in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if let Some(q) = in_str {
            out.push(c);
            if c == q {
                in_str = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            in_str = Some(c);
            out.push(c);
            continue;
        }
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next();
            in_line_comment = true;
            continue;
        }
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block_comment = true;
            continue;
        }
        out.push(c);
    }
    out
}

/// Statements that start a destructive (non-additive) operation,
/// case-insensitive. Each entry is canonicalized to uppercase + space-
/// separated; the scanner normalizes whitespace before matching.
const FORBIDDEN_PREFIXES: &[&str] = &[
    "DROP TABLE",
    "DROP COLUMN",
    "DROP INDEX",
    "DROP VIEW",
    "DROP TRIGGER",
    "DROP CONSTRAINT",
    "ALTER COLUMN",
    "ALTER TABLE RENAME",
    "RENAME TO",
    "RENAME TABLE",
    "RENAME COLUMN",
];

fn line_comment_start(
    line: &str,
    in_block_comment: &mut bool,
    quote: &mut Option<u8>,
) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if *in_block_comment {
            if bytes.get(index..index + 2) == Some(b"*/") {
                *in_block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if let Some(delimiter) = *quote {
            if bytes[index] == delimiter {
                if bytes.get(index + 1) == Some(&delimiter) {
                    index += 2;
                    continue;
                }
                *quote = None;
            }
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            *in_block_comment = true;
            index += 2;
        } else if bytes.get(index..index + 2) == Some(b"--") {
            return Some(index);
        } else if matches!(bytes[index], b'\'' | b'"') {
            *quote = Some(bytes[index]);
            index += 1;
        } else {
            index += 1;
        }
    }
    None
}

fn valid_line_waiver(line: &str, comment_start: usize) -> bool {
    let sql = &line[..comment_start];
    if sql.matches(';').count() != 1 || !sql.trim_end().ends_with(';') {
        return false;
    }
    let comment = line[comment_start + 2..].to_ascii_lowercase();
    let Some((_, waiver)) = comment.split_once("additive-allowed:") else {
        return false;
    };
    let Some(adr) = waiver.trim_start().strip_prefix("adr-") else {
        return false;
    };
    let digits: String = adr.chars().take(4).collect();
    let remainder = adr.get(4..).unwrap_or_default();
    digits.len() == 4
        && digits.chars().all(|c| c.is_ascii_digit())
        && remainder.starts_with(char::is_whitespace)
        && !remainder.trim().is_empty()
}

/// Normalize a SQL fragment for keyword detection: apply audited line-local
/// waivers, strip comments, uppercase, and collapse whitespace.
const B071_TRIGGER_REPLACEMENT_FILE: &str = "migrations/d1/0143_gc_accounting_region_upgrade.sql";

fn valid_b071_trigger_replacement(
    line: &str,
    comment_start: usize,
    migration_path: Option<&str>,
) -> bool {
    if migration_path != Some(B071_TRIGGER_REPLACEMENT_FILE) {
        return false;
    }
    let sql = line[..comment_start].trim().to_ascii_uppercase();
    let comment = line[comment_start + 2..].trim().to_ascii_lowercase();
    matches!(
        (sql.as_str(), comment.as_str()),
        (
            "DROP TRIGGER IF EXISTS TRG_GC_PURGE_ACCOUNTING_REQUIRED;",
            "additive-allowed: adr-0103 replace the deployed b-071 accounting guard"
        ) | (
            "DROP TRIGGER IF EXISTS TRG_GC_PURGE_FINALIZE_ACCOUNTING;",
            "additive-allowed: adr-0103 replace the deployed b-071 accounting finalizer"
        )
    )
}

fn normalize_for_scan(sql: &str) -> String {
    normalize_for_migration(sql, None)
}

fn normalize_for_migration(sql: &str, migration_path: Option<&str>) -> String {
    let mut in_block_comment = false;
    let mut quote = None;
    let mut b071_replacement_counts = [0usize; 2];
    let waiver_filtered = sql
        .lines()
        .map(|line| {
            let comment_start = line_comment_start(line, &mut in_block_comment, &mut quote);
            if comment_start.is_some_and(|start| {
                let valid_replacement = valid_b071_trigger_replacement(line, start, migration_path);
                if valid_replacement {
                    let statement = line[..start].trim().to_ascii_uppercase();
                    if statement.contains("TRG_GC_PURGE_ACCOUNTING_REQUIRED") {
                        b071_replacement_counts[0] += 1;
                    } else if statement.contains("TRG_GC_PURGE_FINALIZE_ACCOUNTING") {
                        b071_replacement_counts[1] += 1;
                    }
                }
                valid_replacement
                    || (migration_path != Some(B071_TRIGGER_REPLACEMENT_FILE)
                        && !line[start + 2..].to_ascii_uppercase().contains("ADR-0103")
                        && valid_line_waiver(line, start))
            }) {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let stripped = strip_sql_comments(&waiver_filtered);
    let upper = stripped.to_ascii_uppercase();
    let mut out = String::with_capacity(upper.len());
    let mut prev_ws = false;
    for c in upper.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    if migration_path == Some(B071_TRIGGER_REPLACEMENT_FILE) && b071_replacement_counts != [1, 1] {
        // Keep the scope failure visible to the shared forbidden-DDL scanner.
        out.push_str(" DROP TRIGGER INVALID B071 REPLACEMENT COUNT;");
    }
    out
}

/// Scan canonicalized SQL for the appearance of any forbidden prefix as
/// a standalone statement start. We split on `;` and check each statement's
/// leading tokens. Returns the list of violating statements (truncated to
/// 80 chars each for log surface).
fn find_violations(canonical_sql: &str) -> Vec<String> {
    let mut viol: Vec<String> = Vec::new();
    for stmt in canonical_sql.split(';') {
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }
        for prefix in FORBIDDEN_PREFIXES {
            // SQLite spells a table swap `ALTER TABLE <name> RENAME TO`; the
            // rename token is therefore not at statement offset zero.
            let forbidden = trimmed.starts_with(prefix)
                || (*prefix == "DROP COLUMN" && trimmed.contains("DROP COLUMN"))
                || (*prefix == "RENAME TO" && trimmed.contains("RENAME TO"));
            if forbidden {
                // Truncate excerpt.
                let excerpt: String = trimmed.chars().take(80).collect();
                viol.push(format!("[{prefix}] {excerpt}"));
                break;
            }
        }
    }
    viol
}

/// Load every migration file's raw text. Cached lazily by the proptest
/// closure (we re-read per iter; the corpus is < 200 KB total so this is
/// cheap and avoids `OnceLock` dance).
fn load_corpus() -> Vec<(String, String)> {
    let dir = locate_migrations_dir();
    let files = list_migration_files(&dir).expect("list migration files");
    files
        .into_iter()
        .map(|p: PathBuf| {
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            let raw = fs::read_to_string(&p).expect("read migration");
            (name, raw)
        })
        .collect()
}

proptest! {
    #![proptest_config(Config { cases: proptest_cases(), .. Config::default() })]

    /// INV-AUTH-MIGRATION-ADDITIVE: no migration statement may start with
    /// DROP / ALTER COLUMN / RENAME (case-insensitive). The property is
    /// checked against EVERY committed migration file; the proptest
    /// random index selects one file per iteration (with replacement) so
    /// over `proptest_cases()` iterations every file is covered with
    /// probability ≈ 1 - (1 - 1/N)^cases.
    ///
    /// Adversarial robustness:
    /// - `-- DROP this later` in a comment MUST NOT trigger (comment-aware lexer).
    /// - `Drop`, `dRoP`, `DROP` all detected (case-insensitive normalize).
    /// - `DROP\tTABLE` / `DROP\nTABLE` detected (whitespace canonicalized).
    #[test]
    fn prop_inv_auth_migration_additive_no_drop_alter_rename(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let corpus = load_corpus();
        prop_assert!(!corpus.is_empty(), "migration corpus is empty");

        let idx = rng.random_range(0..corpus.len());
        let (name, raw) = &corpus[idx];

        let path = format!("migrations/d1/{name}");
        let canonical = normalize_for_migration(raw, Some(&path));
        let violations = find_violations(&canonical);
        prop_assert!(
            violations.is_empty(),
            "INV-AUTH-MIGRATION-ADDITIVE violation in {name}: {} forbidden statement(s) — {:#?}",
            violations.len(),
            violations
        );
    }

    /// INV-AUTH-MIGRATION-ADDITIVE (filename ordering): the numeric
    /// 4-digit prefix MUST yield a NON-DECREASING sequence under the
    /// canonical lexicographic sort `list_migration_files` uses. Two
    /// migrations MAY share a prefix (e.g. `0044_a.sql` + `0044_b.sql`)
    /// in which case the tie is broken deterministically by the
    /// remainder of the filename — but the apply order is still
    /// totally ordered and reproducible.
    ///
    /// Random pairs (i, j) with i < j MUST observe `prefix(i) <=
    /// prefix(j)`; when the prefixes tie, the full filename MUST satisfy
    /// `name(i) < name(j)` (strict, because lexicographic sort yields a
    /// total order and `list_migration_files` deduplicates).
    #[test]
    fn prop_inv_auth_migration_additive_filename_ordering(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let corpus = load_corpus();
        prop_assert!(corpus.len() >= 2, "need ≥ 2 migrations to test ordering");

        // Random distinct ordered pair (i < j).
        let n = corpus.len();
        let i = rng.random_range(0..(n - 1));
        let j = rng.random_range((i + 1)..n);

        let pi: u32 = corpus[i].0.chars().take(4).collect::<String>().parse().unwrap_or(u32::MAX);
        let pj: u32 = corpus[j].0.chars().take(4).collect::<String>().parse().unwrap_or(u32::MAX);

        prop_assert!(pi != u32::MAX && pj != u32::MAX,
            "filename prefix not parseable as u32 — {} or {}", corpus[i].0, corpus[j].0);
        prop_assert!(pi <= pj,
            "INV-AUTH-MIGRATION-ADDITIVE filename ordering drift: prefix({}) = {} should be <= prefix({}) = {} (i={} < j={})",
            corpus[i].0, pi, corpus[j].0, pj, i, j);
        // Strict total ordering on full filename (no duplicates).
        prop_assert!(corpus[i].0 < corpus[j].0,
            "INV-AUTH-MIGRATION-ADDITIVE filename ordering drift: full filename {} should be < {} (i={} < j={})",
            corpus[i].0, corpus[j].0, i, j);
    }

    /// INV-AUTH-MIGRATION-ADDITIVE (idempotency hint): EVERY migration
    /// that contains a `CREATE TABLE` or `CREATE INDEX` MUST also include
    /// the `IF NOT EXISTS` clause. This is the canonical idempotency
    /// guard documented in `migrations/d1/0001_blob_meta.sql` header
    /// comments and `DD-001 D1 migration discipline`.
    ///
    /// Without this clause a re-apply would fail with "object already
    /// exists" instead of being a no-op — breaking the idempotent-replay
    /// property required for safe re-runs.
    #[test]
    fn prop_inv_auth_migration_additive_idempotent_create(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let corpus = load_corpus();
        prop_assert!(!corpus.is_empty(), "migration corpus is empty");

        let idx = rng.random_range(0..corpus.len());
        let (name, raw) = &corpus[idx];
        let canonical = normalize_for_scan(raw);

        // Scan for non-idempotent CREATE TABLE / CREATE INDEX statements.
        // A statement starting with `CREATE TABLE <name>` (without IF NOT
        // EXISTS following) is the canonical anti-pattern. We tolerate
        // `CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`,
        // `CREATE UNIQUE INDEX IF NOT EXISTS`, `CREATE TRIGGER IF NOT EXISTS`,
        // `CREATE VIEW IF NOT EXISTS`.
        let mut anti_pattern: Vec<String> = Vec::new();
        for stmt in canonical.split(';') {
            let t = stmt.trim();
            if t.is_empty() {
                continue;
            }
            let looks_create_table = t.starts_with("CREATE TABLE ");
            let looks_create_index = t.starts_with("CREATE INDEX ")
                || t.starts_with("CREATE UNIQUE INDEX ");
            let looks_create_trigger = t.starts_with("CREATE TRIGGER ");
            let looks_create_view = t.starts_with("CREATE VIEW ");
            let needs_ine = looks_create_table
                || looks_create_index
                || looks_create_trigger
                || looks_create_view;
            if !needs_ine {
                continue;
            }
            // After the leading CREATE-<kind>, the next 3 tokens should
            // be IF NOT EXISTS for idempotency. We check the first 64
            // chars of the statement to keep the matcher simple.
            let head: String = t.chars().take(64).collect();
            let has_ine = head.contains("IF NOT EXISTS");
            if !has_ine {
                let excerpt: String = t.chars().take(80).collect();
                anti_pattern.push(excerpt);
            }
        }

        prop_assert!(
            anti_pattern.is_empty(),
            "INV-AUTH-MIGRATION-ADDITIVE idempotency violation in {name}: \
             {} CREATE statement(s) missing `IF NOT EXISTS` — replay would fail:\n{:#?}",
            anti_pattern.len(), anti_pattern
        );
    }
}

// =====================================================================
// Determinism canary — PRNG seed reproducibility.
// =====================================================================

#[test]
fn prng_seed_is_deterministic_across_invocations() {
    let mut a = ChaCha20Rng::seed_from_u64(0xCAFE_F00D_DEAD_BEEF);
    let mut b = ChaCha20Rng::seed_from_u64(0xCAFE_F00D_DEAD_BEEF);
    for _ in 0..128 {
        let av: u64 = a.random();
        let bv: u64 = b.random();
        assert_eq!(av, bv, "ChaCha20Rng output must be deterministic per seed");
    }
}

/// Unit canary: forbidden-prefix detection covers the canonical
/// mixed-case + whitespace variants. Pinned for fast-fail.
#[test]
fn forbidden_prefix_detector_handles_case_and_whitespace() {
    // Mixed case.
    let sql = "DrOp TaBlE foo;";
    let canon = normalize_for_scan(sql);
    let viol = find_violations(&canon);
    assert!(!viol.is_empty(), "mixed-case DROP TABLE not detected");

    // Tab whitespace.
    let sql = "DROP\tTABLE foo;";
    let canon = normalize_for_scan(sql);
    let viol = find_violations(&canon);
    assert!(!viol.is_empty(), "tab-separated DROP TABLE not detected");

    // Newline whitespace.
    let sql = "DROP\nTABLE foo;";
    let canon = normalize_for_scan(sql);
    let viol = find_violations(&canon);
    assert!(
        !viol.is_empty(),
        "newline-separated DROP TABLE not detected"
    );

    // SQLite table swaps place `RENAME TO` after the table identifier.
    let sql = "ALTER TABLE tier_selections_new RENAME TO tier_selections;";
    let canon = normalize_for_scan(sql);
    let viol = find_violations(&canon);
    assert!(
        !viol.is_empty(),
        "ALTER TABLE ... RENAME TO was not detected"
    );
}

#[test]
fn adr_waiver_is_line_local_and_requires_a_reason() {
    let valid = "DROP TABLE tenant; -- additive-allowed: ADR-0064 widening rebuild";
    assert!(find_violations(&normalize_for_scan(valid)).is_empty());

    let missing_reason = "DROP TABLE tenant; -- additive-allowed: ADR-0064";
    assert!(!find_violations(&normalize_for_scan(missing_reason)).is_empty());

    let next_line = "DROP TABLE tenant;\n-- additive-allowed: ADR-0064 wrong line";
    assert!(!find_violations(&normalize_for_scan(next_line)).is_empty());

    let string_bypass = "SELECT '-- additive-allowed: ADR-0064 approved'; DROP TABLE tenant;";
    assert!(!find_violations(&normalize_for_scan(string_bypass)).is_empty());

    let block_bypass = "/* -- additive-allowed: ADR-0064 approved */ DROP TABLE tenant;";
    assert!(!find_violations(&normalize_for_scan(block_bypass)).is_empty());

    let two_statement_bypass =
        "SELECT 1; DROP TABLE tenant; -- additive-allowed: ADR-0064 approved";
    assert!(!find_violations(&normalize_for_scan(two_statement_bypass)).is_empty());
}

#[test]
fn b071_trigger_replacement_waiver_is_name_and_file_scoped() {
    let valid = concat!(
        "DROP TRIGGER IF EXISTS trg_gc_purge_accounting_required; ",
        "-- additive-allowed: ADR-0103 replace the deployed B-071 accounting guard\n",
        "DROP TRIGGER IF EXISTS trg_gc_purge_finalize_accounting; ",
        "-- additive-allowed: ADR-0103 replace the deployed B-071 accounting finalizer"
    );
    let target = normalize_for_migration(valid, Some(B071_TRIGGER_REPLACEMENT_FILE));
    assert!(find_violations(&target).is_empty());

    let wrong_file = normalize_for_migration(valid, Some("migrations/d1/0144_other.sql"));
    assert!(!find_violations(&wrong_file).is_empty());

    let wrong_trigger = valid.replace(
        "trg_gc_purge_accounting_required",
        "trg_gc_purge_other_trigger",
    );
    let wrong_name = normalize_for_migration(&wrong_trigger, Some(B071_TRIGGER_REPLACEMENT_FILE));
    assert!(!find_violations(&wrong_name).is_empty());

    let duplicate = format!(
        "{valid}\n{}",
        concat!(
            "DROP TRIGGER IF EXISTS trg_gc_purge_accounting_required; ",
            "-- additive-allowed: ADR-0103 replace the deployed B-071 accounting guard"
        )
    );
    assert!(
        !find_violations(&normalize_for_migration(
            &duplicate,
            Some(B071_TRIGGER_REPLACEMENT_FILE)
        ))
        .is_empty(),
        "duplicate authorized trigger DROP was accepted"
    );

    for destructive in [
        "DROP TABLE tenant; -- additive-allowed: ADR-0103 unrelated drop",
        "DROP INDEX IF EXISTS idx_unrelated; -- additive-allowed: ADR-0103 unrelated drop",
        "ALTER TABLE tenant DROP COLUMN name; -- additive-allowed: ADR-0103 unrelated drop",
        "DROP TRIGGER IF EXISTS trg_unrelated; -- additive-allowed: ADR-0103 unrelated trigger",
        "DROP\nTRIGGER IF EXISTS trg_unrelated; -- additive-allowed: ADR-0103 multiline trigger",
        "DROP\nTRIGGER IF EXISTS trg_gc_purge_accounting_required; -- additive-allowed: ADR-0103 replace the deployed B-071 accounting guard",
    ] {
        let mutation = format!("{valid}\n{destructive}");
        let canonical = normalize_for_migration(&mutation, Some(B071_TRIGGER_REPLACEMENT_FILE));
        assert!(
            !find_violations(&canonical).is_empty(),
            "out-of-scope migration replacement was accepted: {destructive}"
        );
    }
}

/// Unit canary: comment-aware lexer prevents false positives from
/// `-- DROP this later` and `/* DROP */` fragments.
#[test]
fn comment_aware_lexer_no_false_positives() {
    let sql = "-- DROP this later\nCREATE TABLE IF NOT EXISTS foo (id INT);";
    let canon = normalize_for_scan(sql);
    let viol = find_violations(&canon);
    assert!(
        viol.is_empty(),
        "false positive on line-comment DROP: {viol:?}"
    );

    let sql = "/* DROP COLUMN x */ CREATE TABLE IF NOT EXISTS bar (id INT);";
    let canon = normalize_for_scan(sql);
    let viol = find_violations(&canon);
    assert!(
        viol.is_empty(),
        "false positive on block-comment DROP: {viol:?}"
    );
}

/// Unit canary: a real DROP COLUMN is detected (true positive).
#[test]
fn forbidden_detector_catches_real_drop_column() {
    let sql = "ALTER TABLE foo DROP COLUMN bar;";
    let canon = normalize_for_scan(sql);
    let viol = find_violations(&canon);
    assert!(
        !viol.is_empty(),
        "ALTER TABLE ... DROP COLUMN was not detected"
    );
}
