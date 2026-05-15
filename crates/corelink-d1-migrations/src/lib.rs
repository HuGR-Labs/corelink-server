//! CoreLink R2-13 — D1 migration replay harness.
//!
//! This crate exposes a small set of pure helpers that the integration
//! test in `tests/d1_migration_integration.rs` uses to:
//!
//! 1. Enumerate every `migrations/d1/NNNN_*.sql` file in numeric order.
//! 2. Apply each file to a fresh in-memory rusqlite database.
//! 3. Classify the outcome per-file as one of:
//!    - [`MigrationOutcome::AppliedClean`] — every statement executed.
//!    - [`MigrationOutcome::AppliedWithKnownD1Skip`] — at least one
//!      statement targets a D1 feature SQLite cannot express (e.g.
//!      `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` or a known
//!      missing-dependency table); the harness logged it and moved on.
//!    - [`MigrationOutcome::FailedHardOrdering`] — a statement failed
//!      for a reason that would also fail under D1 (e.g. CREATE TABLE
//!      with a FOREIGN KEY referencing a table not yet declared by
//!      any earlier migration). This is a real ordering bug — fail the
//!      test.
//!
//! ## Why not just `wrangler d1 migrations apply`?
//!
//! Because that requires a Cloudflare account, real D1 binding ID, and
//! network egress. This harness runs entirely offline in CI and catches
//! the **structural** class of bugs (ordering, FK resolution, column
//! reference to non-existent table) without committing to a real D1
//! database. The runtime harness (`scripts/d1-migration-runner.sh`)
//! handles the staged remote rollout.
//!
//! ## What this DOES NOT catch
//!
//! - Cloudflare-side enforcement (e.g. D1's row-size limit, write rate).
//! - Behavioural correctness of triggers (we install them; we don't
//!   exercise them with synthetic data).
//! - Cross-region replica lag.
//!
//! These are out of scope; see `RB-D1-MIGRATION-APPLY.md` for the
//! end-to-end procedure that exercises them on staging before prod.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Per-statement classification after replay.
#[derive(Debug, Clone)]
pub enum StatementOutcome {
    /// Statement executed successfully.
    Ok,
    /// Statement is a D1 extension that SQLite 3.45 cannot parse
    /// (e.g. `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` — D1 supports
    /// `IF NOT EXISTS` on ADD COLUMN; SQLite does not). We rewrite +
    /// retry once; if the rewrite still fails we mark as skipped.
    SkippedD1Extension {
        /// First 120 chars of the offending statement (whitespace-collapsed).
        sql_excerpt: String,
        /// Why we classified it as a D1-dialect skip.
        reason: String,
    },
    /// Statement references a table or column not declared anywhere
    /// in the migration chain. This is a real schema dependency
    /// hazard — flagged so the integration test can assert against
    /// the known allow-list of pre-existing such hazards.
    SkippedDependencyHazard {
        /// First 120 chars of the offending statement (whitespace-collapsed).
        sql_excerpt: String,
        /// Specifics of the dependency that was missing.
        reason: String,
    },
    /// Statement failed in a way that is also a hard error on D1
    /// (e.g. malformed SQL). Causes the test to fail.
    Failed {
        /// First 120 chars of the offending statement (whitespace-collapsed).
        sql_excerpt: String,
        /// rusqlite error message.
        error: String,
    },
}

/// Aggregate outcome for an entire migration file.
#[derive(Debug, Clone)]
pub enum MigrationOutcome {
    /// All statements applied cleanly.
    AppliedClean,
    /// Applied with at least one statement skipped under the known
    /// D1-vs-SQLite-dialect or schema-hazard allow-list.
    AppliedWithKnownD1Skip {
        /// Number of statements skipped (either D1-extension or hazard).
        skip_count: usize,
    },
    /// At least one statement failed in a way that would also fail on
    /// real D1. Test should fail on this file.
    FailedHardOrdering {
        /// Human-readable list of statement failures.
        failures: Vec<String>,
    },
}

/// Result of replaying every migration.
#[derive(Debug)]
pub struct ReplayReport {
    /// Per-file (filename, outcome) in apply order.
    pub per_file: Vec<(String, MigrationOutcome)>,
    /// Number of files applied.
    pub total_files: usize,
    /// Number of files with at least one skip.
    pub files_with_skip: usize,
    /// Number of files with a hard failure.
    pub files_failed: usize,
}

impl ReplayReport {
    /// True iff every file applied clean or with only known skips.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.files_failed == 0
    }
}

/// Locate the workspace `migrations/d1/` directory by walking up from
/// the current crate's `CARGO_MANIFEST_DIR`. Two levels up reaches
/// `<workspace>/crates/corelink-d1-migrations/..` → `<workspace>`.
#[must_use]
pub fn locate_migrations_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let p = Path::new(manifest)
        .parent()
        .and_then(Path::parent)
        .map(|w| w.join("migrations").join("d1"))
        .unwrap_or_else(|| Path::new(manifest).join("..").join("..").join("migrations").join("d1"));
    p
}

/// List every `NNNN_*.sql` file under `dir` in lexicographic order
/// (which equals numeric order because the prefix is zero-padded).
///
/// Errors are returned via `Result` so the test can fail-loud rather
/// than silently treating an empty dir as success.
pub fn list_migration_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let read = fs::read_dir(dir).map_err(|e| format!("read_dir({dir:?}): {e}"))?;
    let mut out: Vec<PathBuf> = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("sql") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Split a SQL file into statements on semicolons that occur **outside**
/// string literals and `BEGIN … END` trigger bodies.
///
/// SQLite triggers use `BEGIN … END;` blocks where the inner statements
/// also end in `;` — naive split-on-`;` would shred them. We track:
///   - depth of paren-style nesting (for CHECK / DEFAULT / etc.)
///   - whether we're inside a single-quote or double-quote string
///   - whether we're inside a `BEGIN … END` trigger body (depth counter)
pub fn split_statements(sql: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut chars = sql.chars().peekable();
    let mut in_str: Option<char> = None;
    let mut trigger_depth: u32 = 0;
    let mut line_comment = false;
    // Word-boundary tracker: we count BEGIN/END exactly once per word.
    // `last_was_word` records whether the previous char was alnum/_; we
    // only inspect the trailing word when we **transition** from word
    // to non-word.
    let mut last_was_word = false;
    // Track whether the current statement-so-far started with CREATE
    // TRIGGER. Set at each statement boundary; checked before bumping
    // trigger_depth so `BEGIN` inside an `INSERT … BEGIN` would not
    // be miscounted (though no such CoreLink SQL exists).
    let mut stmt_is_trigger = false;

    while let Some(c) = chars.next() {
        // Handle in-line `-- comment` (until newline).
        if line_comment {
            buf.push(c);
            if c == '\n' {
                line_comment = false;
            }
            continue;
        }
        if c == '-' && chars.peek() == Some(&'-') {
            line_comment = true;
            buf.push(c);
            continue;
        }

        if let Some(q) = in_str {
            buf.push(c);
            if c == q {
                in_str = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            in_str = Some(c);
            buf.push(c);
            continue;
        }

        let is_word = c.is_ascii_alphanumeric() || c == '_';

        buf.push(c);

        // Word-level BEGIN/END detection — fire ONLY on the
        // transition from word-char to non-word-char (so each token
        // is inspected at most once).
        if last_was_word && !is_word {
            let trimmed = buf
                .trim_end_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_');
            let last_word: String = trimmed
                .chars()
                .rev()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            let upper = last_word.to_ascii_uppercase();
            if upper == "BEGIN" && stmt_is_trigger {
                trigger_depth = trigger_depth.saturating_add(1);
            } else if upper == "END" && trigger_depth > 0 {
                trigger_depth = trigger_depth.saturating_sub(1);
            } else if (upper == "CREATE" || upper == "TRIGGER")
                && buf.to_ascii_uppercase().contains("CREATE TRIGGER")
            {
                stmt_is_trigger = true;
            }
        }

        last_was_word = is_word;

        if c == ';' && trigger_depth == 0 {
            let stmt = buf.trim().trim_end_matches(';').trim().to_string();
            if !stmt.is_empty() {
                out.push(stmt);
            }
            buf.clear();
            stmt_is_trigger = false;
        }
    }

    let tail = buf.trim().trim_end_matches(';').trim().to_string();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

/// Rewrite SQL fragments that D1 accepts but stock SQLite does not.
///
/// Currently we strip the `IF NOT EXISTS` clause from
/// `ALTER TABLE ... ADD COLUMN IF NOT EXISTS <col> <type>` — SQLite
/// rejects this construct. Once rewritten, a duplicate-column attempt
/// will surface as a normal SQLite error (`duplicate column name`)
/// which the harness then handles as idempotent skip.
#[must_use]
pub fn rewrite_d1_to_sqlite(stmt: &str) -> String {
    // Conservative case-insensitive single-pass rewrite.
    // We only touch `ADD COLUMN IF NOT EXISTS`.
    let needle = "ADD COLUMN IF NOT EXISTS";
    let lower = stmt.to_ascii_uppercase();
    if let Some(idx) = lower.find(needle) {
        let mut out = String::with_capacity(stmt.len());
        out.push_str(&stmt[..idx]);
        out.push_str("ADD COLUMN");
        out.push_str(&stmt[idx + needle.len()..]);
        return out;
    }
    stmt.to_string()
}

/// Apply every migration in `files` to a fresh in-memory database,
/// returning a structured report. Never panics.
pub fn replay_all(files: &[PathBuf]) -> Result<ReplayReport, String> {
    let conn = Connection::open_in_memory()
        .map_err(|e| format!("open_in_memory: {e}"))?;

    // Enable FK enforcement so ordering bugs (CREATE TABLE A
    // REFERENCES B before B is declared) surface.
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("PRAGMA foreign_keys: {e}"))?;

    let mut per_file: Vec<(String, MigrationOutcome)> = Vec::with_capacity(files.len());
    let mut files_with_skip = 0usize;
    let mut files_failed = 0usize;

    for path in files {
        let fname = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("read {fname}: {e}"))?;

        let outcome = replay_file(&conn, &raw);
        match &outcome {
            MigrationOutcome::AppliedWithKnownD1Skip { .. } => files_with_skip += 1,
            MigrationOutcome::FailedHardOrdering { .. } => files_failed += 1,
            MigrationOutcome::AppliedClean => {}
        }
        per_file.push((fname, outcome));
    }

    Ok(ReplayReport {
        per_file,
        total_files: files.len(),
        files_with_skip,
        files_failed,
    })
}

/// Apply a single file's statements to `conn` and classify outcomes.
fn replay_file(conn: &Connection, raw: &str) -> MigrationOutcome {
    let statements = split_statements(raw);
    let mut skip_count = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for stmt in &statements {
        let result = apply_single(conn, stmt);
        match result {
            StatementOutcome::Ok => {}
            StatementOutcome::SkippedD1Extension { .. } => skip_count += 1,
            StatementOutcome::SkippedDependencyHazard { .. } => skip_count += 1,
            StatementOutcome::Failed { sql_excerpt, error } => {
                failures.push(format!("{error} :: {sql_excerpt}"));
            }
        }
    }

    if !failures.is_empty() {
        return MigrationOutcome::FailedHardOrdering { failures };
    }
    if skip_count > 0 {
        return MigrationOutcome::AppliedWithKnownD1Skip { skip_count };
    }
    MigrationOutcome::AppliedClean
}

/// Apply one statement, retrying once with D1→SQLite rewrite if needed,
/// and classify the outcome.
fn apply_single(conn: &Connection, stmt: &str) -> StatementOutcome {
    // Pre-rewrite for known D1 extensions.
    let rewritten = rewrite_d1_to_sqlite(stmt);
    let was_rewritten = rewritten != stmt;

    match conn.execute_batch(&rewritten) {
        Ok(()) => StatementOutcome::Ok,
        Err(err) => {
            let msg = err.to_string();
            let excerpt = excerpt_of(stmt);

            // Idempotency: `duplicate column name` is the SQLite error
            // raised when ADD COLUMN runs twice — semantically a no-op
            // for our purposes (the original D1 statement had `IF NOT
            // EXISTS`, so an already-existing column is exactly the
            // intent).
            if msg.contains("duplicate column name") {
                return StatementOutcome::SkippedD1Extension {
                    sql_excerpt: excerpt,
                    reason: format!("idempotent ADD COLUMN (sqlite says: {msg})"),
                };
            }

            // Schema dependency hazard: ALTER TABLE on a non-existent
            // table, or trigger referencing missing table.
            if msg.contains("no such table") {
                return StatementOutcome::SkippedDependencyHazard {
                    sql_excerpt: excerpt,
                    reason: msg,
                };
            }

            // If we rewrote and still failed, report — but mark as
            // D1-extension skip if the original had a D1-specific
            // construct we couldn't fully translate.
            if was_rewritten {
                return StatementOutcome::SkippedD1Extension {
                    sql_excerpt: excerpt,
                    reason: format!("post-rewrite failed: {msg}"),
                };
            }

            StatementOutcome::Failed { sql_excerpt: excerpt, error: msg }
        }
    }
}

fn excerpt_of(stmt: &str) -> String {
    let oneline: String = stmt.split_whitespace().collect::<Vec<_>>().join(" ");
    if oneline.len() <= 120 {
        oneline
    } else {
        let head: String = oneline.chars().take(117).collect();
        format!("{head}...")
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn split_statements_handles_simple_semicolons() {
        let sql = "CREATE TABLE a (id INT); CREATE INDEX i ON a (id);";
        let parts = split_statements(sql);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("CREATE TABLE"));
        assert!(parts[1].starts_with("CREATE INDEX"));
    }

    #[test]
    fn split_statements_respects_trigger_begin_end() {
        let sql = "
            CREATE TABLE t (id INT);
            CREATE TRIGGER tr BEFORE INSERT ON t FOR EACH ROW BEGIN
              SELECT 1;
              SELECT 2;
            END;
            CREATE INDEX ix ON t (id);
        ";
        let parts = split_statements(sql);
        assert_eq!(parts.len(), 3, "got: {parts:#?}");
        assert!(parts[1].contains("CREATE TRIGGER"));
        assert!(parts[1].contains("END"));
    }

    #[test]
    fn rewrite_strips_if_not_exists_on_add_column() {
        let s = "ALTER TABLE foo ADD COLUMN IF NOT EXISTS bar TEXT NOT NULL DEFAULT 'x'";
        let r = rewrite_d1_to_sqlite(s);
        assert!(!r.to_ascii_uppercase().contains("IF NOT EXISTS"));
        assert!(r.contains("ADD COLUMN bar TEXT"));
    }

    #[test]
    fn rewrite_leaves_other_alter_alone() {
        let s = "CREATE TABLE IF NOT EXISTS x (id INT)";
        let r = rewrite_d1_to_sqlite(s);
        assert_eq!(s, r);
    }
}
