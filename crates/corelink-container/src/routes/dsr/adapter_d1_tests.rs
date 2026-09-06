#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use super::*;

/// FK-ORDER guard. D1 enforces `PRAGMA foreign_keys = ON`, and
/// `stripe_checkout_sessions` has `FOREIGN KEY (tenant_id) REFERENCES
/// tier_selections(tenant_id)`. The erase loop deletes `TENANT_ID_TABLES` in
/// order via separate D1-REST statements, so the CHILD
/// (`stripe_checkout_sessions`) MUST be deleted BEFORE the PARENT
/// (`tier_selections`) — otherwise deleting the parent while an orphan child
/// is still pending fails the FK constraint, the D1 backend Errs, and the
/// tenant's Art.17 erasure 500s and stays stuck (the 2026-08-18 drain-tail
/// root cause). If a future edit reorders these, this test fails LOUD.
#[test]
fn stripe_checkout_sessions_precedes_tier_selections() {
    // (`.unwrap()` — the test module allow-lists `clippy::unwrap_used`; both
    // tables are compile-time constants in the slice, so these never panic.)
    let child = TENANT_ID_TABLES
        .iter()
        .position(|&t| t == "stripe_checkout_sessions")
        .unwrap();
    let parent = TENANT_ID_TABLES
        .iter()
        .position(|&t| t == "tier_selections")
        .unwrap();
    assert!(
        child < parent,
        "FK-ORDER VIOLATION: stripe_checkout_sessions (idx {child}) must be \
             deleted BEFORE tier_selections (idx {parent}) — it holds \
             FOREIGN KEY (tenant_id) REFERENCES tier_selections(tenant_id) and D1 \
             enforces FKs, so parent-first deletion 500s the whole erasure."
    );
}

#[test]
fn erase_set_has_no_overlap_and_no_dupes() {
    // Duplicate guard (kept from the original) across the full erase-set:
    // tenant_id + namespace + the bespoke specials.
    let all: Vec<&str> = TENANT_ID_TABLES
        .iter()
        .chain(NAMESPACE_TABLES.iter())
        .chain(SPECIAL_ERASE_TABLES.iter())
        .copied()
        .collect();
    let mut sorted = all.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), all.len(), "erase-set has a duplicate table");
}

#[test]
fn erase_set_never_touches_a_retain_table() {
    for t in TENANT_ID_TABLES
        .iter()
        .chain(NAMESPACE_TABLES.iter())
        .chain(SPECIAL_ERASE_TABLES.iter())
    {
        assert!(
            !RETAIN_SET.contains(t),
            "RETAIN-set table {t} must NEVER be in the D1 erase-set (ADR-S11-013)"
        );
    }
}

#[test]
fn tenant_linked_pii_tables_are_in_the_erase_set() {
    // Regression: these tenant_id-keyed tables carry tenant PII / seat PII /
    // spend state and MUST be erased on a DSR (GDPR Art.17). A removal would
    // silently leave tenant data behind after an erasure request.
    // `team_member` is the CF-1 worst-case (seat roster: raw Clerk user_id +
    // email_hash) that previously survived a "VerifiedComplete" attestation.
    for t in ["survey_responses", "tenant_quota", "team_member"] {
        assert!(
            TENANT_ID_TABLES.contains(&t),
            "{t} must be in the D1 erase-set (tenant PII)"
        );
        assert!(!RETAIN_SET.contains(&t), "{t} is not a retain-set table");
    }
}

/// CF-1 in-code completeness gate: every table in the hand-maintained
/// registry is classified into EXACTLY ONE bucket (no unclassified, no
/// ambiguous double-classification). This is the runtime-checkable half of
/// the fix (mirrors the `debug_assert!` in `erase()`).
#[test]
fn every_registered_tenant_keyed_table_is_classified_exactly_once() {
    let gaps = unclassified_tenant_keyed_tables();
    assert!(
        gaps.is_empty(),
        "CF-1: these tenant-keyed tables are unclassified (0) or \
             ambiguously multi-classified (>1) — they would be silently \
             skipped on erase yet attested VerifiedComplete: {gaps:?}"
    );
    // Registry itself must be dupe-free.
    let mut sorted: Vec<&str> = ALL_TENANT_KEYED_TABLES.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        ALL_TENANT_KEYED_TABLES.len(),
        "ALL_TENANT_KEYED_TABLES has a duplicate"
    );
}

/// CF-1 LOAD-BEARING drift gate: parse `migrations/d1/*.sql` on disk and
/// assert EVERY live tenant-scoped table (keyed by tenant_id / namespace /
/// an opaque principal id / a subject hash) is present in the registry —
/// and therefore classified erase-or-retain by the test above. This is what
/// makes a FUTURE tenant-keyed migration impossible to land without a
/// conscious erase-vs-retain decision: add the table to a migration and
/// forget the adapter, and THIS test goes red.
#[test]
fn every_migrated_tenant_keyed_table_is_classified() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations/d1");
    let mut found: Vec<String> = Vec::new();
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("CF-1 drift gate cannot read {dir}: {e}"));
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let sql = std::fs::read_to_string(&path).unwrap();
        found.extend(extract_tenant_keyed_tables(&sql));
    }
    found.sort();
    found.dedup();
    assert!(
        !found.is_empty(),
        "CF-1 drift gate parsed ZERO tables — parser or path is broken"
    );

    let missing: Vec<&String> = found
        .iter()
        .filter(|t| !ALL_TENANT_KEYED_TABLES.contains(&t.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "CF-1: tenant-keyed table(s) exist in migrations/d1 but are NOT in \
             the DSR classification registry (a future table escaped erasure \
             classification — add to ALL_TENANT_KEYED_TABLES + classify \
             erase-vs-retain per ADR-S11-013): {missing:?}"
    );
}

/// The MIRROR of `every_migrated_tenant_keyed_table_is_classified`, and
/// the direction that gate never checked.
///
/// CF-1 walked migrations → registry: a table that exists on disk but is
/// unclassified fails. Nothing walked registry → migrations, so a name in
/// the registry that no migration ever creates was structurally invisible.
///
/// That is not hypothetical. `devenv_monthly_vcpu` was added to both
/// `TENANT_ID_TABLES` and `ALL_TENANT_KEYED_TABLES` in #1405 citing
/// "migr. 0094", but 0094 is `0094_runner_usage_counter.sql` and no
/// migration creates that table on `main` — it ships with the unmerged
/// #1397. Because `erase()` runs `count_then_delete` in a bare `for` loop
/// with `?` and NO transaction, the phantom sat at the boundary and turned
/// an Art.17 erasure into: delete the 16 operational tables before it,
/// error on the phantom, and never reach `byok_envelope`,
/// `tenant_byok_config`, `tenant_byok_secret`, the namespace tables,
/// `signup_*`, or the root `tenant` row. Operational data destroyed,
/// identity PII left intact, 500 returned, no attestation, no SEV-1.
///
/// Compared against EVERY `CREATE TABLE` in the migrations, not just the
/// tenant-keyed ones, so a registry entry whose key column the CF-1
/// heuristic does not recognise is not failed for the wrong reason.
#[test]
fn every_registry_table_is_actually_created_by_a_migration() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations/d1");
    let mut created: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("CF-1 mirror gate cannot read {dir}: {e}"));
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let sql = std::fs::read_to_string(&path).unwrap();
        created.extend(extract_all_created_tables(&sql));
    }
    created.sort();
    created.dedup();
    assert!(
        !created.is_empty(),
        "CF-1 mirror gate parsed ZERO CREATE TABLEs — parser or path is broken"
    );

    // EVERY registry, not just `ALL_TENANT_KEYED_TABLES`. `erase()` walks
    // `TENANT_ID_TABLES` and `NAMESPACE_TABLES` directly, so a phantom in
    // one of THOSE is what actually splits a sweep in half — checking only
    // the completeness registry would leave the load-bearing lists
    // unguarded. Caught by mutating the fix: re-adding the phantom to
    // `TENANT_ID_TABLES` alone left an ALL_TENANT_KEYED_TABLES-only
    // version of this test GREEN.
    let mut registry: Vec<&str> = ALL_TENANT_KEYED_TABLES.to_vec();
    for (_, set) in CLASSIFICATION_SETS {
        registry.extend_from_slice(set);
    }
    registry.sort_unstable();
    registry.dedup();

    let phantom: Vec<&&str> = registry
        .iter()
        .filter(|t| !created.iter().any(|c| c == *t))
        .collect();
    assert!(
        phantom.is_empty(),
        "CF-1 MIRROR: table(s) are classified in the DSR registry but no \
             migration in migrations/d1 creates them. A DSR erase runs its \
             deletes in a bare loop with no transaction, so a name that does \
             not exist aborts the sweep PART-WAY — destroying the tables \
             before it and leaving every table after it, including the \
             identity rows, intact. Remove the entry or land its migration: \
             {phantom:?}"
    );
}

#[test]
fn kind_is_d1() {
    // Construction needs a client; assert the const instead (kind() is
    // a pure const map). The orchestrator pins the canonical position.
    assert_eq!(BackendKind::D1.as_str(), "d1");
}

/// Strip SQL line (`--`) and block (`/* */`) comments so `CREATE TABLE`
/// inside doc-comments is not mistaken for a real DDL statement.
///
/// ⚠️ **Line comments are stripped FIRST, and the order is load-bearing.**
/// This helper used to run the block pass first, which made an unpaired
/// `/*` inside a LINE comment swallow the rest of the file: the scan for
/// the closing `*/` ran to EOF and everything after it disappeared. Two
/// migrations contain exactly that — `0090_dsr_tickets.sql:3` documents
/// the `/v1/privacy/dsr/*` route and `0061_adapter_oci_kv.sql` has the
/// same shape — so `dsr_tickets` and `adapter_oci_kv` were INVISIBLE to
/// the CF-1 drift gate that exists to notice unclassified tables. Both
/// happen to be classified already, so nothing was mis-erased; the hole
/// was in the gate, and any future table declared in either file (or any
/// file whose prose mentions a `/*` glob) would have escaped it silently.
fn strip_sql_comments(sql: &str) -> String {
    // Line comments FIRST: a `--` comment can contain an unpaired `/*`
    // (a route glob, a path), and stripping blocks first would treat it
    // as the start of a block that never ends.
    let mut no_line = String::with_capacity(sql.len());
    for line in sql.lines() {
        let l = match line.find("--") {
            Some(k) => &line[..k],
            None => line,
        };
        no_line.push_str(l);
        no_line.push('\n');
    }
    // then block comments (migrations are ASCII; byte scan is safe)
    let mut out = String::with_capacity(no_line.len());
    let bytes = no_line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // skip to closing */
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                j += 1;
            }
            i = (j + 2).min(bytes.len());
            out.push(' ');
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Every `CREATE TABLE` name in a migration, regardless of its columns.
///
/// The tenant-keyed extractor below answers "which tables need
/// classification"; this one answers "which tables exist at all", which is
/// what the mirror gate needs — a registry entry must correspond to a real
/// table even when the CF-1 key-column heuristic would not have flagged it.
fn extract_all_created_tables(sql: &str) -> Vec<String> {
    let clean = strip_sql_comments(sql);
    let mut out = Vec::new();
    let mut rest = clean.as_str();
    while let Some(pos) = rest.find("CREATE TABLE") {
        let after = rest[pos + "CREATE TABLE".len()..].trim_start();
        let after = {
            let lower = after.to_ascii_lowercase();
            if lower.starts_with("if not exists") {
                after["if not exists".len()..].trim_start()
            } else {
                after
            }
        };
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        // `*_new` are transient table-rebuild artifacts, DROP+RENAMEd to
        // their canonical name inside the same migration.
        if !name.is_empty() && !name.ends_with("_new") {
            out.push(name);
        }
        rest = after;
    }
    out
}

/// Extract the names of `CREATE TABLE`s that have a tenant-scoping key
/// column. Transient table-rebuild artifacts (`*_new`) are excluded — they
/// are `DROP`+`RENAME`'d to their canonical name in the same migration.
fn extract_tenant_keyed_tables(sql: &str) -> Vec<String> {
    const KEY_COLS: &[&str] = &[
        "tenant_id",
        "namespace",
        "clerk_sub",
        "clerk_user_id",
        "email_hash",
        "recipient_hash",
    ];
    let clean = strip_sql_comments(sql);
    let mut out = Vec::new();
    let mut rest = clean.as_str();
    while let Some(pos) = rest.find("CREATE TABLE") {
        let after = rest[pos + "CREATE TABLE".len()..].trim_start();
        // optional IF NOT EXISTS (case-insensitive)
        let after = {
            let lower = after.to_ascii_lowercase();
            if lower.starts_with("if not exists") {
                after["if not exists".len()..].trim_start()
            } else {
                after
            }
        };
        // table name = up to first whitespace or '('
        let name_end = after
            .find(|c: char| c.is_whitespace() || c == '(')
            .unwrap_or(after.len());
        let name = after[..name_end].trim().to_string();
        // body = from first '(' to the first "); " statement terminator.
        // tenant_id/namespace are early columns, so truncating at the first
        // ");" is sufficient for key-column detection.
        let body = match after.find('(') {
            Some(open) => {
                let from_open = &after[open..];
                match from_open.find(");") {
                    Some(close) => &from_open[..close],
                    None => from_open,
                }
            }
            None => "",
        };
        if !name.is_empty()
            && !name.ends_with("_new")
            && KEY_COLS.iter().any(|k| contains_word(body, k))
        {
            out.push(name);
        }
        rest = &after[name_end..];
    }
    out
}

/// `body.contains(key)` but only as a whole identifier token, so e.g.
/// `actor_email_hash` does NOT match `email_hash` and `kv_namespace_id`
/// does NOT match `namespace` (word char = ASCII alnum or `_`).
fn contains_word(body: &str, key: &str) -> bool {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let kb = key.as_bytes();
    let bb = body.as_bytes();
    let mut i = 0usize;
    while let Some(rel) = body[i..].find(key) {
        let start = i + rel;
        let end = start + kb.len();
        let before_ok = start == 0 || !is_word(bb[start - 1] as char);
        let after_ok = end >= bb.len() || !is_word(bb[end] as char);
        if before_ok && after_ok {
            return true;
        }
        i = start + 1;
    }
    false
}
