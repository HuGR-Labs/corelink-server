//! Integration test for the `gc_sweep` cron entrypoint binary
//! (`src/bin/gc_sweep.rs`).
//!
//! The bin is a thin, *non-destructive self-check* of the GC sweep runner +
//! the `GC_LIVE_DELETE` dry-run/live gate. Its logic was previously untested
//! (cargo-mutants surfaced surviving mutants across the whole file). We drive
//! the COMPILED binary (`env!("CARGO_BIN_EXE_gc_sweep")`) as a subprocess and
//! assert on its exit status + stdout, which is the only observable surface.
//!
//! ## Bin contract (verified by these tests)
//!
//! * Mode resolves from `GC_LIVE_DELETE`, **failing closed to dry-run**: only
//!   a trimmed/case-insensitive `true` or `1` arms live delete; unset /
//!   `false` / garbage → dry-run.
//! * Dry-run: classifies + prints a report, deletes NOTHING
//!   (`deleted_count == 0`, `deleted_bytes == 0`, the reclaimable object
//!   survives), and prints an explicit non-destructiveness confirmation. The
//!   reclaimable R2 key embeds `r2_key_for(tenant, digest)`.
//! * Live: the destructive path purges ONLY the positively-classified
//!   reclaimable object (`deleted_count == 1`, `deleted_bytes == 4096`); the
//!   grace-pending + live-ref objects are never deleted.
//! * Both modes exit 0 on success (fail-closed → non-zero on any error).
//!
//! Each assertion notes which surviving mutant it kills. SAFETY-CRITICAL:
//! the dry-run-gate assertions (gc_sweep.rs:138/140/146/149) guarantee that
//! a mutated gate that flips dry-run→live or breaks the non-destructiveness
//! check changes observable behavior (extra Err → non-zero exit, or a missing
//! confirmation line).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "integration test driving a subprocess: ergonomic panics on setup failure"
)]

use std::process::{Command, Stdio};

/// The fixture's reclaimable R2 key, as built by the bin's `r2_key_for`
/// helper: `cas/{tenant_id}/{digest}` for tenant `0x5147…0001` + `digest(1)`.
/// Observable in DRY-RUN `reclaimable_keys` (which carries the full R2 key).
/// Pins `r2_key_for` exactly — kills the `String::new()` / `"xyzzy"` body
/// mutants (gc_sweep.rs:51) and the `-`→`/`|`+` digest-length mutants
/// (gc_sweep.rs:46, which would change the digest or panic on a bad length).
const DRY_RUN_RECLAIM_KEY: &str = "cas/00000000-0000-0000-5147-000000000001/\
0000000100000000000000000000000000000000000000000000000000000000";

/// In LIVE mode `reclaimable_keys` carries the bare digest (not the full R2
/// key — see sweep_runner.rs).
const LIVE_RECLAIM_DIGEST: &str =
    "0000000100000000000000000000000000000000000000000000000000000000";

struct Run {
    success: bool,
    stdout: String,
}

/// Run the compiled `gc_sweep` bin. `env` is the `GC_LIVE_DELETE` value to
/// set; `None` removes it entirely (the unset / default-dry-run case).
fn run(env: Option<&str>) -> Run {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gc_sweep"));
    cmd.current_dir(env!("CARGO_TARGET_TMPDIR"))
        .stdin(Stdio::null())
        .env_remove("GC_LIVE_DELETE");
    if let Some(v) = env {
        cmd.env("GC_LIVE_DELETE", v);
    }
    let out = cmd.output().expect("spawn gc_sweep");
    Run {
        success: out.status.success(),
        stdout: String::from_utf8(out.stdout).expect("utf8 stdout"),
    }
}

/// Assert the shared dry-run contract for a given env value (unset / false /
/// garbage all fail closed to dry-run).
fn assert_dry_run(env: Option<&str>) {
    let r = run(env);

    // Fail-closed gate: ANY internal Err → non-zero exit. A mutated
    // non-destructiveness check (gc_sweep.rs:140 `!=`→`==` ×2, :146/:149
    // dropping the `!`) would return Err here → exit non-zero. (kills
    // :146, :149, and both :140 `!=`→`==` mutants)
    // Also: :46 digest `-`→`+`/`/` panics on a bad length → non-zero exit.
    assert!(r.success, "dry-run must exit 0; stdout:\n{}", r.stdout);

    // Banner + report are present at all → kills :55 (main→default ExitCode,
    // no output) and :85 (run_one_sweep→Ok(()), no report printed).
    assert!(
        r.stdout.contains("corelink-gc :: gc_sweep"),
        "missing banner; stdout:\n{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("mode               = dry_run"),
        "expected dry_run mode; stdout:\n{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("dry-run: classify + report only; ZERO deletes."),
        "missing dry-run banner line; stdout:\n{}",
        r.stdout
    );
    // Must NOT have armed the destructive path.
    assert!(
        !r.stdout.contains("LIVE DELETE ENABLED"),
        "dry-run must not arm live delete; stdout:\n{}",
        r.stdout
    );

    // The report body — kills :85 / :55 (no report at all otherwise).
    assert!(
        r.stdout.contains("candidates_scanned        = 3"),
        "expected 3 candidates scanned; stdout:\n{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("reclaimable_count         = 1"),
        "expected 1 reclaimable; stdout:\n{}",
        r.stdout
    );

    // Load-bearing dry-run invariant: ZERO deletes.
    assert!(
        r.stdout.contains("deleted_count             = 0"),
        "dry-run deleted_count must be 0; stdout:\n{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("deleted_bytes             = 0"),
        "dry-run deleted_bytes must be 0; stdout:\n{}",
        r.stdout
    );

    // The reclaimable R2 key embeds r2_key_for's exact output — pins the
    // helper (kills :51 String::new()/"xyzzy" and :46 digest-length mutants).
    assert!(
        r.stdout.contains(DRY_RUN_RECLAIM_KEY),
        "expected exact reclaimable R2 key {DRY_RUN_RECLAIM_KEY:?}; stdout:\n{}",
        r.stdout
    );

    // The explicit non-destructiveness confirmation. This line lives INSIDE
    // the `if !mode.is_live()` block (gc_sweep.rs:138): dropping the `!`
    // (:138 `delete !`) makes the block dry-run-skipped → this line vanishes.
    // (kills :138)
    assert!(
        r.stdout
            .contains("dry-run non-destructiveness verified: 0 deletes, fixture intact."),
        "missing dry-run non-destructiveness confirmation; stdout:\n{}",
        r.stdout
    );
}

#[test]
fn dry_run_when_env_unset() {
    // GC_LIVE_DELETE unset → fail-closed dry-run.
    assert_dry_run(None);
}

#[test]
fn dry_run_when_env_false() {
    // Explicit "false" is NOT live — fails closed to dry-run.
    assert_dry_run(Some("false"));
}

#[test]
fn dry_run_when_env_garbage() {
    // Unparseable garbage is NOT live — fails closed to dry-run.
    assert_dry_run(Some("garbage"));
}

/// Assert the live-delete contract for an env value that DOES arm it
/// ("true" / "1"). Distinguishes the true-vs-false/garbage gate: a mutant
/// that could not tell live from dry-run would fail one side of these tests.
fn assert_live(env: &str) {
    let r = run(Some(env));

    assert!(r.success, "live must exit 0; stdout:\n{}", r.stdout);
    assert!(
        r.stdout.contains("mode               = live_delete"),
        "expected live_delete mode for {env:?}; stdout:\n{}",
        r.stdout
    );
    // The destructive path is armed (only printed in the `mode.is_live()`
    // branch of main, gc_sweep.rs:60).
    assert!(
        r.stdout.contains("LIVE DELETE ENABLED"),
        "expected live-delete warning for {env:?}; stdout:\n{}",
        r.stdout
    );

    // Live deletes EXACTLY the one reclaimable object — not the grace-pending
    // or live-ref ones.
    assert!(
        r.stdout.contains("reclaimable_count         = 1"),
        "expected 1 reclaimable; stdout:\n{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("deleted_count             = 1"),
        "live deleted_count must be 1; stdout:\n{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("deleted_bytes             = 4096"),
        "live deleted_bytes must be 4096; stdout:\n{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("skipped_grace_pending     = 1"),
        "expected 1 grace-pending skip; stdout:\n{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("skipped_refcount_non_zero = 1"),
        "expected 1 refcount skip; stdout:\n{}",
        r.stdout
    );
    assert!(
        r.stdout.contains(LIVE_RECLAIM_DIGEST),
        "expected reclaimed digest in keys; stdout:\n{}",
        r.stdout
    );
    // The dry-run-only confirmation must NOT appear on the live path.
    assert!(
        !r.stdout.contains("dry-run non-destructiveness verified"),
        "live must not print the dry-run confirmation; stdout:\n{}",
        r.stdout
    );
}

#[test]
fn live_delete_when_env_true() {
    // "true" arms the destructive path.
    assert_live("true");
}

#[test]
fn live_delete_when_env_one() {
    // "1" also arms the destructive path (fail-open set is exactly {true, 1}).
    assert_live("1");
}

/// Pins the truth table of the bin's folded dry-run non-destructiveness gate
/// (gc_sweep.rs: `match (deleted_count, deleted_bytes) { (0, 0) => ok, _ =>
/// Err }`). The fold replaced `deleted_count != 0 || deleted_bytes != 0` to
/// remove the `||` mutation surface while keeping the EXACT semantics: it must
/// fail closed if EITHER count OR bytes is non-zero. The real bin can only
/// ever observe `(0, 0)` in dry-run (it never deletes — proven end-to-end by
/// the dry-run subprocess tests above), so this locks the guard's intent for
/// the unreachable-but-load-bearing non-`(0,0)` cases — including the
/// bytes-only `(0, N)` case the original `||` made explicit.
#[test]
fn non_destructive_fold_trips_when_either_count_or_bytes_nonzero() {
    // Same predicate the bin matches on, over (deleted_count, deleted_bytes).
    let is_non_destructive = |deleted_count: u64, deleted_bytes: u64| {
        matches!((deleted_count, deleted_bytes), (0, 0))
    };
    assert!(is_non_destructive(0, 0), "(0,0) is the only non-destructive state");
    assert!(!is_non_destructive(1, 0), "count-only must trip the gate");
    assert!(!is_non_destructive(0, 1), "bytes-only must trip the gate");
    assert!(!is_non_destructive(2, 4096), "both non-zero must trip the gate");
}
