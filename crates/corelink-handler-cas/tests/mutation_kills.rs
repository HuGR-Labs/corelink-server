//! Mutation-kill regression suite — DEBT-008 wave-22.
//!
//! Each test pins behaviour that an empirical `cargo mutants` sweep
//! (2026-05-16) found surviving against the wave-21 baseline. The
//! mapping is documented per-test below; do not delete a test without
//! re-running `cargo mutants -p corelink-handler-cas` to confirm the
//! corresponding mutant is still caught by another assertion.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_handler_cas::audit::{AuditEventKind, InMemoryAuditSink};
use corelink_handler_cas::handler::{fake_hash, CasReadHandler, InMemoryCasHandler};
use corelink_handler_cas::observer::{InMemorySliObserver, Sli};
use corelink_handler_cas::request::CasReadRequest;

fn fixture() -> (
    Arc<InMemoryAuditSink>,
    Arc<InMemorySliObserver>,
    InMemoryCasHandler,
) {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let h = InMemoryCasHandler::new(audit.clone(), sli.clone());
    (audit, sli, h)
}

/// Kills `audit.rs:45 slug -> ""` and `slug -> "xyzzy"`.
///
/// Pin the exact canonical dotted slug for every `AuditEventKind`
/// variant. The mutant family rewrites the `match` arm to a single
/// constant string; any kind whose slug differs from that constant
/// fails one of the assertions below.
#[test]
fn audit_event_kind_slug_is_canonical_dotted() {
    assert_eq!(
        AuditEventKind::ReadAttempted.slug(),
        "corelink.cas.read.attempted"
    );
    assert_eq!(
        AuditEventKind::ReadServed.slug(),
        "corelink.cas.read.served"
    );
    assert_eq!(
        AuditEventKind::ReadDenied.slug(),
        "corelink.cas.read.denied"
    );
    assert_eq!(
        AuditEventKind::WriteAttempted.slug(),
        "corelink.cas.write.attempted"
    );
    assert_eq!(
        AuditEventKind::WriteCommitted.slug(),
        "corelink.cas.write.committed"
    );
    assert_eq!(
        AuditEventKind::WriteDenied.slug(),
        "corelink.cas.write.denied"
    );
    assert_eq!(
        AuditEventKind::CorrectnessViolation.slug(),
        "corelink.cas.correctness.violation"
    );
}

/// Kills `handler.rs:92 Debug::fmt -> Ok(Default::default())`.
///
/// The custom Debug impl renders `InMemoryCasHandler { .. }` (struct
/// debug with non-exhaustive marker). The mutant short-circuits to
/// `Ok(())` without writing anything, so the output becomes empty.
#[test]
fn in_memory_cas_handler_debug_writes_struct_name() {
    let (_audit, _sli, h) = fixture();
    let rendered = format!("{h:?}");
    assert!(
        rendered.contains("InMemoryCasHandler"),
        "Debug must include type name, got {rendered:?}"
    );
    assert!(
        rendered.contains(".."),
        "Debug uses non-exhaustive marker, got {rendered:?}"
    );
    assert!(
        !rendered.is_empty(),
        "Debug must not be empty (mutation guard)"
    );
}

/// Kills `handler.rs:214:37 && -> ||` in correctness-injection check.
///
/// The injection match condition is `t == &req.tenant && h == &req.hash`.
/// Replacing `&&` with `||` makes the injection fire when EITHER the
/// tenant OR the hash matches. We seed an injection for `(t_inject,
/// h_inject)` and then read `(t_inject, h_other)` — the original
/// returns NotFound (different hash, lookup proceeds normally) while
/// the `||` mutant fires a HashMismatch on the tenant-match-alone.
#[test]
fn correctness_injection_requires_both_tenant_and_hash_match() {
    let (_audit, sli, h) = fixture();
    let bytes_a = b"alpha".to_vec();
    let hash_a = fake_hash(&bytes_a);
    let bytes_b = b"beta-bytes".to_vec();
    let hash_b = fake_hash(&bytes_b);
    assert_ne!(
        hash_a, hash_b,
        "fixture hashes must differ to exercise the && branch"
    );

    // Seed BOTH entries so the lookup path is reachable; inject for A.
    h.seed("tenant-x", &hash_a, bytes_a.clone())
        .expect("seed-a");
    h.seed("tenant-x", &hash_b, bytes_b.clone())
        .expect("seed-b");
    h.inject_correctness_mismatch("tenant-x", &hash_a, "f".repeat(64))
        .expect("inject");

    // Read with the SAME tenant but a DIFFERENT hash. Under the
    // original `&&` semantics the injection does NOT fire and the
    // read succeeds (bytes_b returned). Under the `||` mutant the
    // injection fires on tenant-match alone and a HashMismatch
    // error is returned.
    let resp = h
        .read(CasReadRequest::new(
            "tenant-x",
            hash_b.clone(),
            "p",
            "tenant-x",
            1,
        ))
        .expect("read must succeed under original `&&` semantics");
    assert_eq!(resp.bytes, bytes_b);
    assert_eq!(resp.content_hash, hash_b);

    // Negative SLI corroboration: no CorrectnessCas-ERROR observation
    // for the non-matching read. (The happy-path emits a non-error
    // CorrectnessCas observation as numerator-excluded confirmation;
    // the mutation we hunt would emit an `is_error=true` observation
    // via the inject-fire branch.)
    let obs = sli.snapshot().expect("snapshot");
    assert!(
        !obs.iter()
            .any(|o| o.sli == Sli::CorrectnessCas && o.is_error),
        "no CorrectnessCas-ERROR SLI on non-matching read; got {obs:?}"
    );
}

/// Kills `handler.rs:396 fake_hash -> String::new()` and `-> "xyzzy".into()`.
///
/// `fake_hash` must return a deterministic 64-char hex string that
/// encodes the input length in the first 16 hex chars. The mutants
/// return empty / `"xyzzy"`; both have length != 64 and the length-
/// prefix assertion fails.
#[test]
fn fake_hash_is_64_hex_chars_with_length_prefix() {
    let bytes = b"hello".to_vec();
    let s = fake_hash(&bytes);
    assert_eq!(s.len(), 64, "fake_hash must be 64 chars, got {s:?}");
    assert!(
        s.chars().all(|c| c.is_ascii_hexdigit()),
        "fake_hash must be hex, got {s:?}"
    );
    // First 16 hex chars = u64-be length. len=5 → ...0000000000000005.
    let len_prefix = &s[..16];
    assert_eq!(
        len_prefix, "0000000000000005",
        "fake_hash must encode length-as-u64-be in the first 16 hex chars"
    );
    // Pos 16..18 = first byte (b'h' = 0x68).
    assert_eq!(
        &s[16..18],
        "68",
        "fake_hash must encode the first byte after the length prefix"
    );
}

/// Kills `handler.rs:398 < with ==` and `< with >` in the padding loop.
///
/// `fake_hash` builds a prefix then pads with `'0'` until length 64.
/// Replacing `<` with `==` makes the loop body run only when length
/// already equals 64 (so the prefix-only string is returned, length
/// 18 instead of 64). Replacing `<` with `>` flips the loop into
/// an infinite-or-empty regime; in either case the output length is
/// not 64 for a short input, OR (for `>`) it is 64 with non-`'0'`
/// padding characters present.
#[test]
fn fake_hash_pads_short_inputs_to_64_with_zero_chars() {
    // 5-byte input → prefix is 16 (len-hex) + 2 (first-byte-hex) = 18.
    // Padding loop must extend to 64 with `'0'` chars.
    let bytes = b"abcde".to_vec();
    let s = fake_hash(&bytes);
    assert_eq!(s.len(), 64);
    let tail = &s[18..]; // 46 padding chars
    assert_eq!(tail.len(), 46);
    assert!(
        tail.chars().all(|c| c == '0'),
        "padding region must be all '0', got tail={tail:?}"
    );

    // Empty input → length-prefix = 0..0, first_byte fallback = 0 → "00".
    let s_empty = fake_hash(&[]);
    assert_eq!(s_empty.len(), 64);
    assert_eq!(&s_empty[..16], "0000000000000000");
    assert_eq!(&s_empty[16..18], "00");
    assert!(s_empty[18..].chars().all(|c| c == '0'));
}

/// Kills `observer.rs:73 count -> Ok(0)` and `-> Ok(1)` and `== with !=`.
///
/// `InMemorySliObserver::count` filters by exact-`Sli` match. We
/// observe one `AvailCasGet` ok and one `LatencyCasGetP99` ok, then
/// assert the per-Sli counts. The const-return mutants force 0 or 1;
/// the `!=` mutant swaps which SLI's count is returned. Three distinct
/// expected values (0, 1, 2) cover all three mutant families.
#[test]
fn in_memory_sli_observer_count_filters_by_sli_kind() {
    // Drive observations through the real CAS-read handler so we
    // don't need a `SliObservation` ctor (the struct is
    // `#[non_exhaustive]`). Each successful read emits exactly one
    // `AvailCasGet` (ok) AND one `LatencyCasGetP99` (ok). A
    // cross-tenant denial emits an `AvailCasGet` (err) AND a
    // `LatencyCasGetP99` (err). No emission ever targets
    // `AvailCasPut` on the read path.
    let (_audit, sli, h) = fixture();
    let bytes = b"hello".to_vec();
    let hash = fake_hash(&bytes);
    h.seed("t1", &hash, bytes).expect("seed");

    // Two successful reads → AvailCasGet=2 + LatencyCasGetP99=2.
    for _ in 0..2 {
        let _ = h
            .read(CasReadRequest::new("t1", hash.clone(), "p", "t1", 1))
            .expect("read");
    }

    // Per-Sli counts: AvailCasGet=2, LatencyCasGetP99=2,
    // AvailCasPut=0 (kills the Ok(1) const-return: 0 != 1).
    assert_eq!(
        sli.count(Sli::AvailCasGet).expect("count"),
        2,
        "AvailCasGet must report 2 (kills Ok(0)/Ok(1) const-returns + != filter)"
    );
    assert_eq!(
        sli.count(Sli::LatencyCasGetP99).expect("count"),
        2,
        "LatencyCasGetP99 must report 2 (kills Ok(0)/Ok(1) const-return + != filter)"
    );
    assert_eq!(
        sli.count(Sli::AvailCasPut).expect("count"),
        0,
        "AvailCasPut must report 0 (kills Ok(1) const-return)"
    );
}
