//! Cross-component end-to-end property tests at 10 000+ iter (WI-S01-006).
//!
//! This file is the WI-S01-006 deliverable — a consolidated proptest suite
//! that exercises the **integrated** S-01 stack (`corelink-hash`,
//! `corelink-tenant-path`, `corelink-worker`, `corelink-meta`,
//! `corelink-reapi`) against the three CRITICAL invariants:
//!
//! - **INV-TENANT-ISOLATION** — distinct tenants cannot collide on R2 keys
//!   nor read each other's bytes through the storage seam.
//! - **INV-CAS-INTEGRITY** — bodies whose claimed BLAKE3 digest does not
//!   match the actual content are rejected before any storage I/O.
//! - **INV-CAS-IDEMPOTENCY** — replaying the same `(tenant, digest, body,
//!   request_id)` produces a single durable row in R2 + D1 + audit_outbox.
//!
//! ## Why this file is *cross-component* rather than per-crate
//!
//! Each S-01 crate already ships its own per-crate property tests at 10k or
//! 100k iter:
//!
//! - `corelink-tenant-path::tests::prop_*` — HMAC injectivity at 10k.
//! - `corelink-worker::tests::prop_r2_path::*` — cross-tenant key
//!   disjointness at 100k.
//! - `corelink-reapi::tests::prop_idempotency::*` — orchestrator dedup at
//!   256 cases (with internal retry loops).
//!
//! The **integration gap** addressed here is: when those crates are wired
//! together through `CasWriteOrchestrator`, do the invariants still hold
//! end-to-end at 10k cases against random adversarial inputs? A regression
//! in any one seam (digest verify removed, prefix derivation skipped,
//! audit dedup keyed wrong) shows up here even if each per-crate test
//! still passes in isolation.
//!
//! ## File location rationale
//!
//! WI-S01-006 v1.0.0 §13 named the canonical path
//! `crates/corelink-worker/tests/prop_cas.rs`. The v1.1.0 §13 (FROZEN)
//! moves the canonical path to `crates/corelink-reapi/tests/prop_cas.rs`
//! because the cross-component scope dev-depends on
//! `corelink-reapi::orchestrator` AND `corelink-meta`, and
//! `corelink-worker` cannot dev-depend on `corelink-reapi` without
//! creating a workspace cycle (`corelink-reapi → corelink-worker →
//! corelink-reapi`). The path-routing decision is recorded in the
//! v1.1.0 §31 changelog. The substantive deliverable — three
//! CRITICAL-invariant property tests at 10k iter through the integrated
//! stack — is unchanged.
//!
//! ## Iter counts and runtime budget
//!
//! Each of the **seven serial `proptest!` properties** runs at
//! `cases = 10_000` (= 70 000 effective serial iterations); the
//! **single concurrent retry-storm property** runs at `cases = 256`
//! and each case spawns **16 concurrent tokio tasks** (= 4 096 total
//! concurrent `commit_put` task instances across the whole property).
//! Three additional **concrete boundary tests** (empty body, 5 MiB
//! exact, 5 MiB + 1) pin the size-edge behavior outside the proptest
//! generator's `1..=4096` body range — see the rustdoc on each
//! `#[tokio::test]` for what is asserted.
//!
//! On commodity CI hardware the full file completes in well under 30 s
//! (the WI §AC budget). Property failures persist seeds in
//! `crates/corelink-reapi/tests/prop_cas.proptest-regressions` (proptest's
//! sibling-file convention; one file per `*.rs` test target) for
//! deterministic reproduce. The file is created on first failure and
//! `git`-tracked.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines,
    reason = "test code: property test framework + adversarial assertions"
)]

use std::collections::HashSet;
use std::sync::Arc;

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_meta::{BlobMetaKey, InMemoryMetaStore, MetaStore};
use corelink_reapi::orchestrator::{
    audit_request_id_for_blob, CasPutOutcome, CasWriteOrchestrator, CommitPutPlan,
    NoopOrphanReconciler, OrchestratorError,
};
use corelink_tenant_path::TenantDerivationKey;
use corelink_cas::r2_storage::{InMemoryR2, R2Reader, R2Writer};
use corelink_replication::region_resolver::{Region, TenantCtx};
use proptest::prelude::*;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Returns the
/// env-overridden value if set, else `default`. The 100k nightly
/// variant overrides via `PROPTEST_CASES=100_000`.
fn proptest_cases(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

const FIXTURE_TDK_BYTES: [u8; 32] = *b"prop-cas-WI-S01-006-tdk-32-bytes";

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn fixture_tdk() -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new(FIXTURE_TDK_BYTES))
}

/// Build a `CommitPutPlan` for a `(ctx, body, digest, request_id)` tuple.
///
/// `audit_id` is derived **deterministically** from `audit_request_id` via
/// BLAKE3-128 truncation so retries with the same
/// `(client_request_id, tenant_id, digest)` collapse to the same
/// `audit_outbox.id`, exercising the same idempotent-retry semantics the
/// outbox dedupe relies on. This is *not* the production handler's UUIDv7
/// minting strategy (`handler::process_one_blob` mints a UUIDv7 from a
/// per-RPC clock); it is a test-only stable-id strategy that lets us assert
/// "row count = 1 across N retries" without a clock fixture. The
/// load-bearing dedupe key — `audit_outbox.UNIQUE (request_id, event_type)`
/// — is exercised identically: the `(audit_request_id, event_type)` pair
/// is what the meta layer dedupes on, and that pair is held constant
/// across retries.
fn make_plan<'a>(
    ctx: &'a TenantCtx,
    body: Bytes,
    digest: Digest,
    client_request_id: &'a str,
    now_ms: u64,
) -> CommitPutPlan<'a> {
    let canonical = format!("blake3:{}", digest.to_hex());
    let audit_request_id =
        audit_request_id_for_blob(client_request_id, ctx.tenant_id(), &canonical);
    // Stable per-(audit_request_id) UUID: hash the audit_request_id and re-cast
    // the first 16 bytes. Deterministic so retries dedupe; collisions across
    // distinct audit_request_ids are vanishingly improbable (BLAKE3-128).
    let audit_id_bytes: [u8; 16] = {
        let h = blake3::hash(audit_request_id.as_bytes());
        let mut out = [0u8; 16];
        out.copy_from_slice(&h.as_bytes()[..16]);
        out
    };
    CommitPutPlan {
        ctx,
        body: body.clone(),
        claimed_digest: digest,
        size_bytes: u64::try_from(body.len()).unwrap_or(u64::MAX),
        digest_canonical_text: canonical,
        principal_id: Uuid::from_u128(0xC0FE_C0FE_C0FE_C0FE_C0FE_C0FE_C0FE_C0FE),
        region_str: "wnam",
        client_request_id,
        audit_request_id,
        audit_id: Uuid::from_bytes(audit_id_bytes),
        now_ms,
    }
}

/// Drive a single `commit_put` against a single integrated stack instance,
/// blocking on the current-thread runtime. Used by every property below so
/// the integrated stack is the same across the three invariants.
fn drive_commit_put(
    backend: &Arc<InMemoryR2>,
    meta: &InMemoryMetaStore,
    ctx: &TenantCtx,
    body: Bytes,
    digest: Digest,
    client_request_id: &str,
    now_ms: u64,
) -> Result<CasPutOutcome, OrchestratorError> {
    let writer = R2Writer::new(Region::Wnam, Arc::clone(backend));
    let reconciler = NoopOrphanReconciler;
    let orch = CasWriteOrchestrator::new(&writer, meta, &reconciler);
    let plan = make_plan(ctx, body, digest, client_request_id, now_ms);
    rt().block_on(orch.commit_put(plan)).map(|o| o.outcome)
}

// ---------------------------------------------------------------------------
// Property 1 — INV-TENANT-ISOLATION (cross-component)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(10_000),
        max_shrink_iters: 32,
        ..ProptestConfig::default()
    })]

    /// **INV-TENANT-ISOLATION (CRITICAL) — A wrote, B did NOT, B cannot see A's blob**.
    ///
    /// This is the load-bearing assertion for the no-cross-tenant-leak
    /// property: tenant A writes a blob with digest D under prefix(A);
    /// tenant B never writes anything. We then probe with `R2Reader` under
    /// `ctx_b`, asking for the SAME digest D. Because the canonical R2 key
    /// is `(region, prefix(tenant), digest)`, B's read must hit a key that
    /// does not exist in the backend and surface `R2Error::NotFound`. A
    /// reader regression that ignored or misrouted `ctx.prefix()` (e.g.
    /// strip the prefix from the canonical key constructor) would let B
    /// read A's bytes — this property catches that regression.
    ///
    /// Distinct from the earlier `prop_tenant_isolation_e2e_distinct_keys`
    /// (which compares two writers' keys) because here B never writes —
    /// the only way B could see A's bytes is by misrouting on the read
    /// path. **Codex round-1 P0 fix.**
    #[test]
    fn prop_tenant_isolation_no_cross_tenant_read(
        tid_a in any::<u128>(),
        tid_b in any::<u128>(),
        body_seed in any::<u64>(),
        body_len in 1usize..=4096usize,
    ) {
        prop_assume!(tid_a != tid_b);
        let body_vec: Vec<u8> = (0..body_len)
            .map(|i| (body_seed.rotate_left(i as u32 & 63) ^ (i as u64)) as u8)
            .collect();
        let body = Bytes::from(body_vec);
        let digest = Digest::compute(&body);

        let tdk = fixture_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();

        let ctx_a = TenantCtx::new(&tdk, Uuid::from_u128(tid_a), Region::Wnam);
        let ctx_b = TenantCtx::new(&tdk, Uuid::from_u128(tid_b), Region::Wnam);

        // Only tenant A writes.
        let out_a = drive_commit_put(
            &backend,
            &meta,
            &ctx_a,
            body.clone(),
            digest,
            "req-tenant-A-only",
            1_700_000_000_000,
        ).unwrap();
        prop_assert_eq!(out_a, CasPutOutcome::Fresh);
        prop_assert_eq!(backend.len(), 1, "one R2 object after A's write");

        // Tenant B asks for the SAME digest. Must NotFound — A's bytes
        // live behind prefix(A), and the canonical key derivation for B
        // produces prefix(B)+digest, which does not exist.
        let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
        let b_result = rt().block_on(reader.get(&ctx_b, &digest));
        prop_assert!(
            matches!(
                b_result,
                Err(corelink_cas::r2_storage::R2Error::NotFound)
            ),
            "tenant B must NOT read tenant A's blob; got: {:?}",
            b_result
        );

        // Sanity: A can read A's own bytes.
        let a_bytes = rt().block_on(reader.get(&ctx_a, &digest)).unwrap();
        prop_assert_eq!(a_bytes, body);

        // D1 has exactly one row keyed under tenant A.
        prop_assert_eq!(meta.row_count(), 1);
        let row_a = rt().block_on(
            meta.get(&BlobMetaKey::new(Uuid::from_u128(tid_a), digest))
        ).unwrap().expect("tenant A row");
        prop_assert_eq!(row_a.refcount, 1);
        let row_b = rt().block_on(
            meta.get(&BlobMetaKey::new(Uuid::from_u128(tid_b), digest))
        ).unwrap();
        prop_assert!(row_b.is_none(), "tenant B has no D1 row");
    }

    /// **INV-TENANT-ISOLATION — distinct R2 keys + distinct D1 rows when both tenants write the same body**.
    ///
    /// Stays valuable even with the dedicated `no_cross_tenant_read`
    /// property above because it exercises the *parallel-write* outcome:
    /// 2 R2 keys, 2 D1 rows, both refcount=1. A regression that collapsed
    /// keys (e.g. dropped the prefix from `canonical_key`) would surface
    /// here with `keys.len() == 1` even if the no-cross-tenant-read
    /// property happens to still pass.
    #[test]
    fn prop_tenant_isolation_distinct_keys_when_both_write(
        tid_a in any::<u128>(),
        tid_b in any::<u128>(),
        body_seed in any::<u64>(),
        body_len in 1usize..=4096usize,
    ) {
        prop_assume!(tid_a != tid_b);
        let body_vec: Vec<u8> = (0..body_len)
            .map(|i| (body_seed.rotate_left(i as u32 & 63) ^ (i as u64)) as u8)
            .collect();
        let body = Bytes::from(body_vec);
        let digest = Digest::compute(&body);

        let tdk = fixture_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();

        let ctx_a = TenantCtx::new(&tdk, Uuid::from_u128(tid_a), Region::Wnam);
        let ctx_b = TenantCtx::new(&tdk, Uuid::from_u128(tid_b), Region::Wnam);

        let out_a = drive_commit_put(
            &backend, &meta, &ctx_a, body.clone(), digest,
            "req-tenant-A", 1_700_000_000_000,
        ).unwrap();
        prop_assert_eq!(out_a, CasPutOutcome::Fresh);
        let out_b = drive_commit_put(
            &backend, &meta, &ctx_b, body.clone(), digest,
            "req-tenant-B", 1_700_000_000_001,
        ).unwrap();
        prop_assert_eq!(out_b, CasPutOutcome::Fresh);

        // 2 R2 keys, 2 D1 rows.
        let keys: HashSet<String> = backend.keys_snapshot().into_iter().collect();
        prop_assert_eq!(keys.len(), 2, "tenants must land at distinct R2 keys");
        prop_assert_eq!(meta.row_count(), 2);
        prop_assert_eq!(meta.outbox_snapshot().len(), 2);
    }

    /// **INV-TENANT-ISOLATION — tenant-scoped audit dedupe**.
    ///
    /// Tenants A and B share the EXACT SAME `client_request_id` (the gRPC
    /// `x-request-id` header). The handler must NOT collide on the
    /// global `audit_outbox.UNIQUE (request_id, event_type)` constraint —
    /// the `audit_request_id_for_blob` derivation MUST scope the dedupe
    /// key by `tenant_id`. This is the codex-round-1-P1 fix from
    /// WI-S01-005 made into a 10k-iter integration witness here.
    ///
    /// If `audit_request_id_for_blob` were changed to drop the `tenant`
    /// segment (e.g. revert to plain `client_request_id` as the dedupe
    /// key), the second `commit_put` would surface
    /// `MetaError::AuditIdempotencyConflict` and this property would
    /// fail. **Codex round-1 P1 fix.**
    #[test]
    fn prop_tenant_isolation_audit_dedupe_is_tenant_scoped(
        tid_a in any::<u128>(),
        tid_b in any::<u128>(),
        body_seed in any::<u64>(),
        body_len in 1usize..=512usize,
        client_req in "[a-z0-9]{1,32}",
    ) {
        prop_assume!(tid_a != tid_b);
        let body_vec: Vec<u8> = (0..body_len)
            .map(|i| (body_seed.rotate_left(i as u32 & 63) ^ (i as u64)) as u8)
            .collect();
        let body = Bytes::from(body_vec);
        let digest = Digest::compute(&body);

        let tdk = fixture_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();

        let ctx_a = TenantCtx::new(&tdk, Uuid::from_u128(tid_a), Region::Wnam);
        let ctx_b = TenantCtx::new(&tdk, Uuid::from_u128(tid_b), Region::Wnam);

        // SAME client_request_id across tenants.
        let out_a = drive_commit_put(
            &backend, &meta, &ctx_a, body.clone(), digest,
            client_req.as_str(), 1_700_000_000_000,
        ).unwrap();
        prop_assert_eq!(out_a, CasPutOutcome::Fresh);

        // Tenant B must succeed with the SAME client_request_id;
        // tenant scoping closes the global UNIQUE collision.
        let out_b = drive_commit_put(
            &backend, &meta, &ctx_b, body.clone(), digest,
            client_req.as_str(), 1_700_000_000_001,
        ).unwrap();
        prop_assert_eq!(out_b, CasPutOutcome::Fresh);

        prop_assert_eq!(backend.len(), 2);
        prop_assert_eq!(meta.row_count(), 2);
        prop_assert_eq!(meta.outbox_snapshot().len(), 2);
    }
}

// ---------------------------------------------------------------------------
// Property 2 — INV-CAS-INTEGRITY (cross-component)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(10_000),
        max_shrink_iters: 32,
        ..ProptestConfig::default()
    })]

    /// **INV-CAS-INTEGRITY (CRITICAL)**.
    ///
    /// For any random `(body, claimed_digest)` pair pushed through the
    /// orchestrator, the outcome must be:
    ///
    /// - `Ok(_)` iff `BLAKE3(body) == claimed_digest`.
    /// - `Err(OrchestratorError::HashMismatch)` otherwise — AND the R2
    ///   backend is byte-identical to its pre-call state, AND the D1 row
    ///   count is unchanged, AND the audit outbox has no new row.
    ///
    /// This guarantees that a poisoned-body attack (client lying about the
    /// digest) physically cannot reach R2 or D1 — `VerifiedBody::new` is
    /// the first gate in the orchestrator's seven-step contract.
    #[test]
    fn prop_cas_integrity_rejects_mismatch(
        body_seed in any::<u64>(),
        wrong_seed in any::<u64>(),
        body_len in 1usize..=4096usize,
        wrong_len in 1usize..=4096usize,
    ) {
        // Build the body and a *different* set of bytes; if they happen to
        // produce equal BLAKE3 (≈2^-256 chance, well below the iteration
        // budget) skip the case via prop_assume!.
        let body_vec: Vec<u8> = (0..body_len)
            .map(|i| (body_seed.rotate_left(i as u32 & 63) ^ (i as u64)) as u8)
            .collect();
        let wrong_vec: Vec<u8> = (0..wrong_len)
            .map(|i| (wrong_seed.rotate_left(i as u32 & 63) ^ ((i as u64).wrapping_mul(31))) as u8)
            .collect();
        prop_assume!(body_vec != wrong_vec);
        let body = Bytes::from(body_vec.clone());
        let lying_digest = Digest::compute(&wrong_vec);
        prop_assume!(Digest::compute(&body_vec) != lying_digest);

        let tdk = fixture_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(7), Region::Wnam);

        // Snapshot pre-call state.
        let pre_keys = backend.keys_snapshot();
        let pre_rows = meta.row_count();
        let pre_outbox = meta.outbox_snapshot().len();

        let result = drive_commit_put(
            &backend,
            &meta,
            &ctx,
            body,
            lying_digest,
            "req-mismatch",
            1_700_000_000_000,
        );

        // Must return HashMismatch.
        prop_assert!(matches!(
            result,
            Err(OrchestratorError::HashMismatch(_))
        ));

        // R2 + D1 + audit outbox all byte-identical post-call.
        prop_assert_eq!(backend.keys_snapshot(), pre_keys);
        prop_assert_eq!(meta.row_count(), pre_rows);
        prop_assert_eq!(meta.outbox_snapshot().len(), pre_outbox);
    }

    /// **INV-CAS-INTEGRITY happy path**.
    ///
    /// For any random body, a `(body, BLAKE3(body))` pair pushed through the
    /// orchestrator produces a Fresh outcome and persists exactly one R2
    /// object + one D1 row + one audit row.
    #[test]
    fn prop_cas_integrity_accepts_match(
        body_seed in any::<u64>(),
        body_len in 1usize..=4096usize,
    ) {
        let body_vec: Vec<u8> = (0..body_len)
            .map(|i| (body_seed.rotate_left(i as u32 & 63) ^ (i as u64)) as u8)
            .collect();
        let body = Bytes::from(body_vec);
        let digest = Digest::compute(&body);

        let tdk = fixture_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(13), Region::Wnam);

        let out = drive_commit_put(
            &backend,
            &meta,
            &ctx,
            body.clone(),
            digest,
            "req-match",
            1_700_000_000_000,
        ).unwrap();

        prop_assert_eq!(out, CasPutOutcome::Fresh);
        prop_assert_eq!(backend.len(), 1);
        prop_assert_eq!(meta.row_count(), 1);
        prop_assert_eq!(meta.outbox_snapshot().len(), 1);

        // Body round-trip via the reader confirms what we PUT is what we GET.
        let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
        let got = rt().block_on(reader.get(&ctx, &digest)).unwrap();
        prop_assert_eq!(got, body);
    }
}

// ---------------------------------------------------------------------------
// Property 3 — INV-CAS-IDEMPOTENCY (cross-component)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(10_000),
        max_shrink_iters: 32,
        ..ProptestConfig::default()
    })]

    /// **INV-CAS-IDEMPOTENCY (CRITICAL)**.
    ///
    /// Replaying the SAME `(tenant, digest, body, client_request_id)`
    /// through the assembled stack `retries` times produces:
    ///
    /// - Exactly one `CasPutOutcome::Fresh`; the rest are `Idempotent`.
    /// - Exactly one R2 object (the second PUT returns Duplicate).
    /// - Exactly one D1 row (PK enforcement).
    /// - Exactly one audit_outbox row (UNIQUE on `(request_id, event_type)`).
    ///
    /// The assertion that the *same* request_id collapses to a single audit
    /// row is the cross-component witness for FF-HR-005 — at-least-once
    /// retries from a flaky Bazel client cannot multiply downstream
    /// audit-chain emissions.
    #[test]
    fn prop_cas_idempotency_collapses_retries(
        tid in any::<u128>(),
        body_seed in any::<u64>(),
        body_len in 1usize..=4096usize,
        retries in 2u8..=8u8,
    ) {
        let body_vec: Vec<u8> = (0..body_len)
            .map(|i| (body_seed.rotate_left(i as u32 & 63) ^ (i as u64)) as u8)
            .collect();
        let body = Bytes::from(body_vec);
        let digest = Digest::compute(&body);

        let tdk = fixture_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(tid), Region::Wnam);

        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let reconciler = NoopOrphanReconciler;
        let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);

        let mut fresh_count = 0u32;
        let mut idempotent_count = 0u32;

        rt().block_on(async {
            for i in 0..retries {
                // Same request_id → audit_request_id is identical → outbox
                // dedup at the (request_id, event_type) UNIQUE constraint.
                let plan = make_plan(
                    &ctx,
                    body.clone(),
                    digest,
                    "req-shared-idempotent",
                    1_700_000_000_000 + u64::from(i),
                );
                let out = orch.commit_put(plan).await.unwrap();
                match out.outcome {
                    CasPutOutcome::Fresh => fresh_count += 1,
                    CasPutOutcome::Idempotent => idempotent_count += 1,
                }
            }
        });

        prop_assert_eq!(fresh_count, 1, "exactly one Fresh outcome on retry storm");
        prop_assert_eq!(
            idempotent_count,
            u32::from(retries) - 1,
            "remaining outcomes must be Idempotent"
        );
        prop_assert_eq!(backend.len(), 1, "R2 holds exactly one object");
        prop_assert_eq!(meta.row_count(), 1, "D1 holds exactly one row");
        prop_assert_eq!(
            meta.outbox_snapshot().len(),
            1,
            "audit_outbox holds exactly one row (UNIQUE dedupe)"
        );
    }

    /// **INV-CAS-IDEMPOTENCY — distinct request_ids, same blob**.
    ///
    /// When N retries arrive with N DISTINCT `client_request_id`s but the
    /// same body, we keep one R2 object + one D1 row but accumulate N
    /// audit_outbox rows. This is the canonical "dedup is per
    /// (request_id, event_type), NOT per blob_meta PK" semantic.
    #[test]
    fn prop_cas_idempotency_distinct_request_ids_distinct_audit_rows(
        tid in any::<u128>(),
        body_seed in any::<u64>(),
        body_len in 1usize..=4096usize,
        retries in 2u8..=8u8,
    ) {
        let body_vec: Vec<u8> = (0..body_len)
            .map(|i| (body_seed.rotate_left(i as u32 & 63) ^ (i as u64)) as u8)
            .collect();
        let body = Bytes::from(body_vec);
        let digest = Digest::compute(&body);

        let tdk = fixture_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(tid), Region::Wnam);

        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let reconciler = NoopOrphanReconciler;
        let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);

        rt().block_on(async {
            for i in 0..retries {
                let request_id = format!("req-distinct-{i}");
                let plan = make_plan(
                    &ctx,
                    body.clone(),
                    digest,
                    &request_id,
                    1_700_000_000_000 + u64::from(i),
                );
                let _ = orch.commit_put(plan).await.unwrap();
            }
        });

        prop_assert_eq!(backend.len(), 1, "R2 still holds one object");
        prop_assert_eq!(meta.row_count(), 1, "D1 still holds one row");
        prop_assert_eq!(
            meta.outbox_snapshot().len(),
            usize::from(retries),
            "distinct request_ids produce distinct audit rows"
        );
    }
}

// ---------------------------------------------------------------------------
// Concrete boundary + adversarial tests (not proptest!; cover edge cases the
// generators above explicitly truncate at 1..=4096 bytes per codex round-1
// P2 finding — tested once each here at concrete sizes so the boundary
// behavior is locked in regardless of generator coverage).
// ---------------------------------------------------------------------------

/// Empty body (size_bytes == 0) is rejected by the meta layer's
/// `CHECK (size_bytes > 0)` constraint AFTER the body has been verified
/// AND R2 has stored it. The current orchestrator semantic is:
///
/// - `VerifiedBody::new` accepts a 0-byte body (BLAKE3 over empty input
///   has a well-defined output: `af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262`).
/// - `R2Writer::put` accepts the 0-byte body (under the 5 MiB cap).
/// - `MetaStore::commit_put` rejects with `MetaError::Backend(...)`
///   because the schema CHECK forbids size 0.
/// - The orchestrator then fires the `OrphanReconciler` (R2 stored;
///   meta failed → fast-path rollback).
///
/// We assert the contract end-to-end: the call returns
/// `Err(OrchestratorError::Meta(_))` and the audit_outbox stays empty.
/// The R2 object DOES land in the in-memory backend (the noop
/// reconciler does not delete) — but that is the documented behavior of
/// `NoopOrphanReconciler`; production wires `R2DeleteReconciler` (S-06
/// integration adds the delete trait method) so the orphan is reconciled
/// via the GC sweep within ≤ 24h. This test pins the contract.
#[tokio::test]
async fn empty_body_is_rejected_by_meta_size_check_not_silent_success() {
    let tdk = fixture_tdk();
    let backend = Arc::new(InMemoryR2::new());
    let meta = InMemoryMetaStore::new();
    let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);

    let body = Bytes::new(); // 0 bytes
    let digest = Digest::compute(&body);

    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let reconciler = NoopOrphanReconciler;
    let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);

    let plan = make_plan(&ctx, body, digest, "req-empty", 1_700_000_000_000);
    let err = orch.commit_put(plan).await.unwrap_err();

    // Meta-layer error (CHECK constraint), NOT silent Fresh / Idempotent.
    assert!(
        matches!(err, OrchestratorError::Meta(_)),
        "empty body must surface a Meta-layer error, got: {err:?}"
    );
    // No audit row staged (rollback at the meta-side prevents emission).
    assert_eq!(meta.outbox_snapshot().len(), 0);
    assert_eq!(meta.row_count(), 0);

    // Pin the orphan-R2-after-meta-rejection contract: under
    // `NoopOrphanReconciler` the R2 PUT *is* committed (R2 is the
    // step-2 gate; the meta CHECK is step-4). Production wires
    // `R2DeleteReconciler` (S-06 sweep companion) so the orphan is
    // reconciled within ≤ 24h. If a future refactor moves the
    // size-0 check earlier (e.g. into `VerifiedBody::new` or the
    // orchestrator's pre-flight) the R2 backend would NOT carry the
    // orphan; this assertion locks in the current ordering so any
    // such refactor is a deliberate spec change with a paired WI
    // bump rather than an undocumented behavior drift.
    assert_eq!(
        backend.len(),
        1,
        "current contract: R2 stores the empty body (orphan) before \
         meta CHECK rejects it; reconciler is responsible for cleanup"
    );
}

/// 5 MiB exactly — the canonical single-blob upper bound
/// (`SINGLE_BLOB_LIMIT_BYTES` in the worker crate). Must succeed Fresh.
#[tokio::test]
async fn boundary_blob_at_exactly_5_mib_succeeds_e2e() {
    use corelink_cas::r2_storage::SINGLE_BLOB_LIMIT_BYTES;

    let tdk = fixture_tdk();
    let backend = Arc::new(InMemoryR2::new());
    let meta = InMemoryMetaStore::new();
    let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);

    // 5 MiB body filled with deterministic but non-trivial bytes.
    let body_vec: Vec<u8> = (0..SINGLE_BLOB_LIMIT_BYTES)
        .map(|i| (i & 0xFF) as u8)
        .collect();
    let body = Bytes::from(body_vec);
    let digest = Digest::compute(&body);

    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let reconciler = NoopOrphanReconciler;
    let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);

    let plan = make_plan(&ctx, body.clone(), digest, "req-5mib", 1_700_000_000_000);
    let out = orch.commit_put(plan).await.unwrap();
    assert_eq!(out.outcome, CasPutOutcome::Fresh);

    // Round-trip via the reader.
    let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
    let got = reader.get(&ctx, &digest).await.unwrap();
    assert_eq!(got.len(), SINGLE_BLOB_LIMIT_BYTES);
    assert_eq!(got, body);
    assert_eq!(meta.row_count(), 1);
    assert_eq!(meta.outbox_snapshot().len(), 1);
}

/// 5 MiB + 1 byte — over the canonical single-blob upper bound. Must
/// surface `OrchestratorError::R2(_::BlobTooLarge)`. R2 + D1 untouched.
#[tokio::test]
async fn boundary_blob_just_over_5_mib_rejects_e2e() {
    use corelink_cas::r2_storage::SINGLE_BLOB_LIMIT_BYTES;

    let tdk = fixture_tdk();
    let backend = Arc::new(InMemoryR2::new());
    let meta = InMemoryMetaStore::new();
    let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);

    let body_vec: Vec<u8> = (0..=SINGLE_BLOB_LIMIT_BYTES)
        .map(|i| (i & 0xFF) as u8)
        .collect();
    let body = Bytes::from(body_vec);
    let digest = Digest::compute(&body);

    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let reconciler = NoopOrphanReconciler;
    let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);

    let plan = make_plan(&ctx, body, digest, "req-5mib-plus-1", 1_700_000_000_000);
    let err = orch.commit_put(plan).await.unwrap_err();
    // Must specifically be BlobTooLarge — not just any R2 error class —
    // so the test traps drift to a different size-limit code path
    // (e.g. RegionMismatch, Backend) without re-spec'ing the contract.
    match err {
        OrchestratorError::R2(corelink_cas::r2_storage::R2Error::BlobTooLarge {
            size,
            limit,
        }) => {
            assert_eq!(
                size,
                corelink_worker::storage::r2::SINGLE_BLOB_LIMIT_BYTES + 1
            );
            assert_eq!(limit, corelink_worker::storage::r2::SINGLE_BLOB_LIMIT_BYTES);
        }
        other => panic!("expected R2(BlobTooLarge), got: {other:?}"),
    }
    assert_eq!(backend.len(), 0);
    assert_eq!(meta.row_count(), 0);
    assert_eq!(meta.outbox_snapshot().len(), 0);
}

// ---------------------------------------------------------------------------
// Concurrent retry-storm property test (codex round-1 P1: idempotency was
// only exercised serially). Drives 16 tokio tasks each constructing a
// fresh per-task `CasWriteOrchestrator` borrowing the SAME shared
// `(R2Writer, MetaStore, OrphanReconciler)` Arcs, all firing `commit_put`
// with the SAME (tenant, digest, body, request_id). Asserts that the
// backends settle to a single R2 object + single D1 row + single
// audit_outbox row regardless of task ordering. The (R2-Fresh,
// Meta-Inserted) pair may *split* across tasks under true concurrency —
// see the property's rustdoc for why we assert `fresh_count <= 1`
// rather than `fresh_count == 1`.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(256),
        max_shrink_iters: 16,
        ..ProptestConfig::default()
    })]

    /// **INV-CAS-IDEMPOTENCY — concurrent retry-storm**.
    ///
    /// 16 tokio tasks each construct a fresh `CasWriteOrchestrator`
    /// borrowing the SAME shared `(R2Writer, MetaStore, OrphanReconciler)`
    /// (held behind `Arc`s and threaded through every task) and fire
    /// `commit_put` with the SAME `(ctx, body, digest, request_id)` —
    /// modeling N gRPC clients that retry the same upload concurrently
    /// against one server replica. The orchestrator is a stateless
    /// per-RPC value (it borrows references only); the load-bearing
    /// shared state is the trio of backend stores. The in-memory backends
    /// (`InMemoryR2`, `InMemoryMetaStore`) protect their state behind a
    /// `Mutex`, mirroring D1's batch-as-atomic-transaction semantics +
    /// R2's `If-None-Match: *` race-free contract. The invariants the
    /// storm must satisfy:
    ///
    /// - Every task's `commit_put` returns `Ok(_)` (no
    ///   `AuditIdempotencyConflict`, no `Backend` error).
    /// - Backends settle to: exactly one R2 object, exactly one D1 row,
    ///   exactly one audit_outbox row.
    /// - The aggregate outcome bag `(Fresh, Idempotent)` partitions all
    ///   16 tasks; we do NOT assert "exactly one Fresh" because under
    ///   true concurrency the (R2-Fresh, Meta-Inserted) pair can be
    ///   *split* across two distinct tasks: task X may win the R2 PUT
    ///   (→ R2 Fresh) while task Y wins the meta INSERT (→ Meta
    ///   Inserted); each then sees its own pair as (Fresh, AlreadyExists)
    ///   or (Duplicate, Inserted) respectively, and the orchestrator
    ///   blend yields Idempotent for both. Reporting Fresh from "the"
    ///   first arrival is a serial-only property; the load-bearing
    ///   property under concurrency is the durable single-row outcome
    ///   on every backend, which we assert below.
    ///
    /// Iter count: 256 cases (vs the 10 000 of the serial properties);
    /// each case spawns 16 concurrent tasks, so the property executes
    /// `256 * 16 = 4 096` total `commit_put` task instances across the
    /// whole property (i.e. across all cases combined; *not* per case).
    /// The WI §AC budget (≤ 30 s combined) leaves only single-digit
    /// seconds for this adversarial regime: smaller case count,
    /// deeper per-case fanout.
    #[test]
    fn prop_cas_idempotency_concurrent_retry_storm(
        tid in any::<u128>(),
        body_seed in any::<u64>(),
        body_len in 1usize..=512usize,
    ) {
        const TASK_COUNT: u32 = 16;

        let body_vec: Vec<u8> = (0..body_len)
            .map(|i| (body_seed.rotate_left(i as u32 & 63) ^ (i as u64)) as u8)
            .collect();
        let body = Bytes::from(body_vec);
        let digest = Digest::compute(&body);

        let tdk = fixture_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = Arc::new(InMemoryMetaStore::new());
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(tid), Region::Wnam);
        let writer = Arc::new(R2Writer::new(Region::Wnam, Arc::clone(&backend)));
        let reconciler = Arc::new(NoopOrphanReconciler);

        let multi_rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .unwrap();

        let outcomes: Vec<_> = multi_rt.block_on(async {
            let mut handles = Vec::with_capacity(TASK_COUNT as usize);
            for i in 0..TASK_COUNT {
                let writer = Arc::clone(&writer);
                let meta = Arc::clone(&meta);
                let reconciler = Arc::clone(&reconciler);
                let body = body.clone();
                let now_ms = 1_700_000_000_000u64 + u64::from(i);
                handles.push(tokio::spawn(async move {
                    let orch = CasWriteOrchestrator::new(
                        writer.as_ref(),
                        meta.as_ref(),
                        reconciler.as_ref(),
                    );
                    let plan = make_plan(
                        &ctx,
                        body,
                        digest,
                        "req-shared-concurrent",
                        now_ms,
                    );
                    orch.commit_put(plan).await.unwrap()
                }));
            }
            let mut out = Vec::with_capacity(TASK_COUNT as usize);
            for h in handles {
                out.push(h.await.unwrap());
            }
            out
        });

        // Every task succeeded.
        prop_assert_eq!(outcomes.len(), TASK_COUNT as usize);
        let fresh_count = outcomes.iter()
            .filter(|o| matches!(o.outcome, CasPutOutcome::Fresh))
            .count();
        let idempotent_count = outcomes.iter()
            .filter(|o| matches!(o.outcome, CasPutOutcome::Idempotent))
            .count();
        // The two outcome bins partition all tasks (no third bin exists
        // in the type system, so this is a sanity sum).
        prop_assert_eq!(fresh_count + idempotent_count, TASK_COUNT as usize);
        // Under serial retry, fresh_count == 1 strictly. Under concurrent
        // retry the (R2-Fresh, Meta-Inserted) pair can split across tasks
        // and yield 0 Fresh + 16 Idempotent (see rustdoc); both regimes
        // are correct as long as the durable single-row outcome holds.
        prop_assert!(fresh_count <= 1,
            "concurrent storm must NEVER produce more than one Fresh outcome \
             (would imply two successful R2-fresh + meta-inserted pairs)");

        prop_assert_eq!(backend.len(), 1, "exactly one R2 object after storm");
        prop_assert_eq!(meta.row_count(), 1, "exactly one D1 row after storm");
        prop_assert_eq!(
            meta.outbox_snapshot().len(),
            1,
            "exactly one audit_outbox row after storm"
        );

        // Pin the refcount = 1 invariant after the concurrent retry storm
        // (S-01 first-write contract: `commit_put` sets refcount=1 on
        // first insert and is a `INSERT OR IGNORE` no-op on every
        // subsequent retry; refcount mutation lands in WI-S04-001 / S-06).
        let row = multi_rt
            .block_on(meta.get(&BlobMetaKey::new(Uuid::from_u128(tid), digest)))
            .unwrap()
            .expect("D1 row must exist after the storm");
        prop_assert_eq!(row.refcount, 1, "refcount must stay at 1 after concurrent storm");
    }
}
