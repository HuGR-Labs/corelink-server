//! Wave-23 DEBT-008 targeted mutation kills for `corelink-multipart-schema`.
//!
//! Pins surviving mutants from the wave-23 first sweep
//! (cargo-mutants 25.0.1, `--no-shuffle --jobs 4 --timeout 120`).
//! See `specs/_audits/2026-05-16-debt-008-wave23-mutation-sweep.md`
//! §4 for the 1:1 survivor → kill mapping.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::uninlined_format_args,
    clippy::assertions_on_constants,
    reason = "test code; panic on assertion failure is the contract"
)]

use corelink_multipart_schema::region::MultipartRegion;
use corelink_multipart_schema::sim::{
    ChunkUpsertOutcome, ChunkUpsertRequest, ManifestChunkInsertRequest, MultipartInitiateRequest,
    MultipartSchema, MultipartSessionState, SessionId, CHUNK_INDEX_MAX, CHUNK_SIZE_BYTES_MAX,
    DEFAULT_SESSION_TTL_MS, MAX_CHUNKS_PER_BLOB,
};
use uuid::Uuid;

fn hex64(seed: u8) -> String {
    let mut s = String::with_capacity(64);
    for _ in 0..64 {
        s.push(char::from_digit(u32::from(seed % 16), 16).unwrap_or('0'));
    }
    s
}

fn ten() -> Uuid {
    Uuid::from_u128(0x1111_1111_1111_1111_1111_1111_1111_1111)
}

// ─── sim.rs constants ────────────────────────────────────────────────

/// Kills `CHUNK_INDEX_MAX = MAX_CHUNKS_PER_BLOB - 1` arithmetic at
/// sim.rs:64 (`- with +`, `- with /`). Canonical value 81_919.
#[test]
fn chunk_index_max_is_max_chunks_minus_one_exact() {
    assert_eq!(CHUNK_INDEX_MAX, 81_919);
    assert_eq!(CHUNK_INDEX_MAX, MAX_CHUNKS_PER_BLOB - 1);
    // `MAX_CHUNKS_PER_BLOB + 1` would yield 81_921; `/` would yield 1.
    assert_ne!(CHUNK_INDEX_MAX, MAX_CHUNKS_PER_BLOB + 1);
    #[allow(clippy::identity_op, reason = "killing `- → /` mutant requires literal division-by-one")]
    let div_by_one = MAX_CHUNKS_PER_BLOB / 1;
    assert_ne!(CHUNK_INDEX_MAX, div_by_one);
    // Sanity: max chunks - 1 must be < max chunks.
    assert!(CHUNK_INDEX_MAX < MAX_CHUNKS_PER_BLOB);
}

/// Kills `DEFAULT_SESSION_TTL_MS = 7 * 24 * 60 * 60 * 1_000` chain
/// at sim.rs:67 (every `*` mutated to `+` or `/`). Canonical value
/// 604_800_000 (7 days in ms).
#[test]
fn default_session_ttl_ms_is_seven_days_exact() {
    assert_eq!(DEFAULT_SESSION_TTL_MS, 604_800_000);
    assert_eq!(DEFAULT_SESSION_TTL_MS, 7 * 24 * 60 * 60 * 1_000);

    // Each `*` replaced by `+` yields a smaller broken value.
    // The most adversarial substitution `7 + 24 + 60 + 60 + 1000` =
    // 1151 — pin a positive-only assertion that excludes that range.
    assert!(DEFAULT_SESSION_TTL_MS > 1_000_000);
    // `/` versions yield values < 1000 across the chain.
    assert!(DEFAULT_SESSION_TTL_MS > 1_000_000_000_i64 / 1_000);
}

// ─── sim.rs: MultipartSessionState::as_str ───────────────────────────

/// Kills `MultipartSessionState::as_str -> &'static str with ""`
/// and `with "xyzzy"` at sim.rs:271. Pins the exact SQL literal
/// strings ("in_progress", "completed", "aborted") that the
/// `chk_multipart_state_domain` CHECK constraint relies on.
#[test]
fn multipart_session_state_as_str_pins_exact_literals() {
    assert_eq!(MultipartSessionState::InProgress.as_str(), "in_progress");
    assert_eq!(MultipartSessionState::Completed.as_str(), "completed");
    assert_eq!(MultipartSessionState::Aborted.as_str(), "aborted");

    // Neither "" nor "xyzzy" can satisfy three distinct equality
    // assertions over three distinct variants.
    assert_ne!(MultipartSessionState::InProgress.as_str(), "");
    assert_ne!(MultipartSessionState::Completed.as_str(), "");
    assert_ne!(MultipartSessionState::Aborted.as_str(), "");
    assert_ne!(MultipartSessionState::InProgress.as_str(), "xyzzy");
}

// ─── sim.rs: Display impls ───────────────────────────────────────────

/// Kills `Display::fmt -> Ok(Default::default())` at sim.rs:248
/// (SessionId Display). Default impl writes nothing; canonical
/// writes the uuid's hyphenated form.
#[test]
fn session_id_display_writes_uuid_hyphenated() {
    let id = SessionId(Uuid::from_u128(0x4444_5555_6666_7777_8888_9999_aaaa_bbbb));
    let rendered = format!("{}", id);
    assert!(!rendered.is_empty(), "Display must write the uuid (kills Ok(default))");
    assert!(rendered.contains('-'), "uuid hyphenated form contains '-'");
    assert_eq!(rendered.len(), 36, "canonical uuid hyphenated len = 36");
    assert!(rendered.contains("4444"));
}

// ─── region.rs: Display impl ─────────────────────────────────────────

/// Kills `Display::fmt -> Ok(Default::default())` at region.rs:142.
#[test]
fn multipart_region_display_writes_three_letter_code() {
    for (region, expected) in [
        (MultipartRegion::Sam, "sam"),
        (MultipartRegion::Iad, "iad"),
        (MultipartRegion::Lhr, "lhr"),
        (MultipartRegion::Nrt, "nrt"),
        (MultipartRegion::Syd, "syd"),
    ] {
        let rendered = format!("{}", region);
        assert_eq!(
            rendered, expected,
            "MultipartRegion Display must emit canonical 3-letter code"
        );
        assert!(!rendered.is_empty());
    }
}

// ─── sim.rs: get_manifest_chunk, manifest_chunks_count ───────────────

/// Kills `get_manifest_chunk -> Option<&ManifestChunkRow> with None`
/// at sim.rs:650 AND `manifest_chunks_count -> usize with 1` at
/// sim.rs:674. Inserts two manifest_chunks then asserts both
/// observable getters reflect actual state.
#[test]
fn manifest_chunks_getters_reflect_actual_state() {
    let mut s = MultipartSchema::new();
    let tenant = ten();
    let blob_digest = hex64(0xaa);
    let chunk_digest_a = hex64(0xbb);
    let chunk_digest_b = hex64(0xcc);

    s.upsert_chunk(ChunkUpsertRequest {
        tenant_id: tenant,
        chunk_digest: chunk_digest_a.clone(),
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        region: MultipartRegion::Sam,
        r2_object_key: format!("chunk-sam/abcd/{chunk_digest_a}"),
        size_bytes: 1024,
        now_ms: 1_000,
        created_by_pat_id: None,
        created_by_request_id: Some("r1".to_string()),
    })
    .unwrap();
    s.upsert_chunk(ChunkUpsertRequest {
        tenant_id: tenant,
        chunk_digest: chunk_digest_b.clone(),
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        region: MultipartRegion::Sam,
        r2_object_key: format!("chunk-sam/abcd/{chunk_digest_b}"),
        size_bytes: 1024,
        now_ms: 1_000,
        created_by_pat_id: None,
        created_by_request_id: Some("r2".to_string()),
    })
    .unwrap();

    // Add two manifest_chunks rows pointing at the two chunks.
    s.insert_manifest_chunk(ManifestChunkInsertRequest {
        tenant_id: tenant,
        blob_digest: blob_digest.clone(),
        chunk_index: 0,
        chunk_digest: chunk_digest_a.clone(),
    })
    .unwrap();
    s.insert_manifest_chunk(ManifestChunkInsertRequest {
        tenant_id: tenant,
        blob_digest: blob_digest.clone(),
        chunk_index: 1,
        chunk_digest: chunk_digest_b.clone(),
    })
    .unwrap();

    // get_manifest_chunk MUST return Some for both indices (kills
    // `with None` substitution).
    let row_0 = s.get_manifest_chunk(&tenant, &blob_digest, 0);
    let row_1 = s.get_manifest_chunk(&tenant, &blob_digest, 1);
    assert!(row_0.is_some(), "expected manifest_chunks row at index 0 (kills None mutation)");
    assert!(row_1.is_some(), "expected manifest_chunks row at index 1");
    assert_eq!(row_0.unwrap().chunk_digest, chunk_digest_a);
    assert_eq!(row_1.unwrap().chunk_digest, chunk_digest_b);

    // manifest_chunks_count MUST report 2 (kills `with 1` substitution).
    let count = s.manifest_chunks_count();
    assert_eq!(count, 2, "expected exactly 2 manifest_chunks rows (kills `with 1`)");
    assert_ne!(count, 1);
    assert_ne!(count, 0);
}

// ─── sim.rs: list_sessions_for_tenant ────────────────────────────────

/// Kills `list_sessions_for_tenant -> Vec<&MultipartSession> with vec![]`
/// at sim.rs:912 AND `== with !=` at sim.rs:914 (tenant filter
/// comparison). Inserts sessions for two tenants and asserts only
/// the queried tenant's rows are returned.
#[test]
fn list_sessions_for_tenant_filters_exact_match() {
    let mut s = MultipartSchema::new();
    let tenant_a = Uuid::from_u128(0xaaaa_aaaa_aaaa_aaaa_aaaa_aaaa_aaaa_aaaa);
    let tenant_b = Uuid::from_u128(0xbbbb_bbbb_bbbb_bbbb_bbbb_bbbb_bbbb_bbbb);

    let sid_a1 = SessionId(Uuid::from_u128(0x1111_0000_0000_0000_0000_0000_0000_0001));
    let sid_a2 = SessionId(Uuid::from_u128(0x1111_0000_0000_0000_0000_0000_0000_0002));
    let sid_b1 = SessionId(Uuid::from_u128(0x2222_0000_0000_0000_0000_0000_0000_0001));

    for (sid, tid, seed) in [
        (sid_a1, tenant_a, 0x01u8),
        (sid_a2, tenant_a, 0x02u8),
        (sid_b1, tenant_b, 0x03u8),
    ] {
        s.initiate_session(MultipartInitiateRequest {
            session_id: sid,
            tenant_id: tid,
            tenant_prefix: [0xab; 16],
            path_key_id: 1,
            blob_digest_expected: hex64(seed),
            region: MultipartRegion::Sam,
            bucket: "corelink-chunk-sam".to_string(),
            object_key: format!("chunk-sam/abcd/{}", hex64(seed)),
            now_ms: 1_000,
            ttl_ms: None,
            created_by_pat_id: None,
            created_by_request_id: format!("r-{seed}"),
        })
        .unwrap();
    }

    let a_sessions = s.list_sessions_for_tenant(&tenant_a);
    assert_eq!(a_sessions.len(), 2, "tenant_a must own exactly 2 sessions (kills vec![] and != mutations)");
    assert!(a_sessions.iter().all(|s| s.tenant_id == tenant_a));

    let b_sessions = s.list_sessions_for_tenant(&tenant_b);
    assert_eq!(b_sessions.len(), 1, "tenant_b must own exactly 1 session");
    assert!(b_sessions.iter().all(|s| s.tenant_id == tenant_b));

    // Confirm tenant scoping is strict — neither vec contains
    // the other tenant's rows. `vec![]` mutation would make
    // a_sessions empty (caught above); `!=` mutation would swap the
    // results.
    assert_ne!(a_sessions.len(), 0);
    assert_ne!(b_sessions.len(), 0);
}

// ─── sim.rs: is_lower_hex ────────────────────────────────────────────

/// Kills `is_lower_hex -> bool with true` at sim.rs:973. The
/// validator must reject uppercase / non-hex chars; mutating to
/// always-true would accept "G" / "Z" / uppercase digests.
#[test]
fn upsert_chunk_rejects_non_lowercase_hex_digest() {
    let mut s = MultipartSchema::new();
    let tenant = ten();
    // Uppercase digest — is_lower_hex must return false; the
    // `with true` mutation would silently accept this.
    let upper_digest = "A".repeat(64);

    let outcome = s.upsert_chunk(ChunkUpsertRequest {
        tenant_id: tenant,
        chunk_digest: upper_digest,
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        region: MultipartRegion::Sam,
        r2_object_key: "chunk-sam/abcd/aaa".to_string(),
        size_bytes: 1024,
        now_ms: 1_000,
        created_by_pat_id: None,
        created_by_request_id: Some("r1".to_string()),
    });
    assert!(
        outcome.is_err(),
        "uppercase hex digest must fail check (kills is_lower_hex -> true)"
    );

    // Sanity: canonical lowercase passes.
    let lower_outcome = s.upsert_chunk(ChunkUpsertRequest {
        tenant_id: tenant,
        chunk_digest: "a".repeat(64),
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        region: MultipartRegion::Sam,
        r2_object_key: "chunk-sam/abcd/aaa".to_string(),
        size_bytes: 1024,
        now_ms: 1_000,
        created_by_pat_id: None,
        created_by_request_id: Some("r2".to_string()),
    });
    assert_eq!(lower_outcome.unwrap(), ChunkUpsertOutcome::Inserted);
}

// ─── sim.rs: size_bytes upper bound ──────────────────────────────────

/// Kills `< with >` at sim.rs:490 (size_bytes check in
/// upsert_chunk: `if !(1..=CHUNK_SIZE_BYTES_MAX).contains(&req.size_bytes)`
/// — the underlying expansion includes a `<` comparator).
#[test]
fn upsert_chunk_rejects_oversize_size_bytes() {
    let mut s = MultipartSchema::new();
    let tenant = ten();

    // Size strictly greater than CHUNK_SIZE_BYTES_MAX must fail.
    let outcome = s.upsert_chunk(ChunkUpsertRequest {
        tenant_id: tenant,
        chunk_digest: hex64(0x11),
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        region: MultipartRegion::Sam,
        r2_object_key: "chunk-sam/abcd/aaa".to_string(),
        size_bytes: CHUNK_SIZE_BYTES_MAX + 1,
        now_ms: 1_000,
        created_by_pat_id: None,
        created_by_request_id: Some("r-oversize".to_string()),
    });
    assert!(
        outcome.is_err(),
        "size_bytes > CHUNK_SIZE_BYTES_MAX must reject (kills `< → >` boundary inversion)"
    );

    // And the boundary exactly equal to CHUNK_SIZE_BYTES_MAX must pass.
    let max_outcome = s.upsert_chunk(ChunkUpsertRequest {
        tenant_id: tenant,
        chunk_digest: hex64(0x22),
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        region: MultipartRegion::Sam,
        r2_object_key: "chunk-sam/abcd/bbb".to_string(),
        size_bytes: CHUNK_SIZE_BYTES_MAX,
        now_ms: 1_000,
        created_by_pat_id: None,
        created_by_request_id: Some("r-maxsize".to_string()),
    });
    assert!(
        max_outcome.is_ok(),
        "size_bytes == CHUNK_SIZE_BYTES_MAX must pass (inclusive upper bound)"
    );

    // And size_bytes < 1 (zero) must reject.
    let zero_outcome = s.upsert_chunk(ChunkUpsertRequest {
        tenant_id: tenant,
        chunk_digest: hex64(0x33),
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        region: MultipartRegion::Sam,
        r2_object_key: "chunk-sam/abcd/ccc".to_string(),
        size_bytes: 0,
        now_ms: 1_000,
        created_by_pat_id: None,
        created_by_request_id: Some("r-zerosize".to_string()),
    });
    assert!(zero_outcome.is_err(), "size_bytes == 0 must reject");
}
