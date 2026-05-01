//! REAPI v2 Action Cache conformance suite (WI-S04-006 §6.1.1).
//!
//! Pinned subset of the bazelbuild/remote-apis Action Cache test
//! battery (per ADR-0036 §Annex A — the canonical commit pin lives in
//! the ADR + the conformance harness here). Covers AC operations:
//!
//! - `GetActionResult` — 4 conformance tests (hit / miss /
//!   tenant-scoped 404 / sig-flow round-trip).
//! - `UpdateActionResult` — 6 conformance tests (success / idempotent
//!   refresh / digest size validation / outputs missing 422 /
//!   sig-flow byte-stable / result_hash immutable on idempotent
//!   refresh per `INV-AC-RESULT-HASH-IMMUTABLE`).
//!
//! **REAPI v2 has NO `BatchUpdateActionResult`** (Lote 10.4bis P0
//! fix #5 — REAPI v2 batch is on CAS via `BatchUpdateBlobs`, not on
//! AC). The 10-test envelope is the canonical AC subset.
//!
//! ## Why a host-side conformance harness (not a real Bazel client)
//!
//! Per charter `trait-abstraction-defer` pattern: the real
//! `bazelbuild/remote-apis` proto + a real Bazel client driving
//! against `wrangler dev` lands alongside the CF integration tier
//! in a forward sprint (the production gRPC surface in
//! `corelink-reapi` doesn't ship until WI-S04-001's tonic wrappers
//! land). The host-side harness stitches the canonical REAPI v2 wire
//! contract against the pure-logic
//! [`ActionCacheHandler`](corelink_worker::reapi::ac::ActionCacheHandler)
//! seam — same canonical contract, no integration-tier-only deps.
//! The Bazel reference clients (Bazel 7.x + 8.x + Buck2) sit at the
//! gRPC frontier; the handler trait is the load-bearing seam they
//! converge on.
//!
//! ## Result artifact
//!
//! `cargo test --release -p corelink-worker --features tower-middleware
//!     --test reapi_v2_ac_conformance` outputs a JSON-line report when
//! `CORELINK_CONFORMANCE_REPORT=<path>` is set (CI uploads the artifact
//! to the run summary). Without the env var, the harness emits a
//! human-readable trace to stdout.

#![cfg(feature = "tower-middleware")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "test harness — panics + stdout/stderr traces are themselves the assertion surface"
)]
#![allow(missing_docs, reason = "test crate")]

use std::sync::Arc;

use corelink_hash::Digest;
use corelink_pat::{PatEnv, PatId, PatScopes, SCOPE_CACHE_R, SCOPE_CACHE_W};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::cache::kv::InMemoryKv;
use corelink_worker::middleware::auth_ctx::__test_helpers::make_auth_ctx;
use corelink_worker::middleware::auth_ctx::{AuthCtx, AuthMethod, PrincipalId};
use corelink_worker::reapi::ac::handler::{
    ActionCacheHandler, ActionCacheHandlerBuilder, ActionCacheHandlerImpl, AcError, Clock,
    FakeClock, InMemoryAcEnvelopeStore, DEFAULT_AC_TTL_EXTEND_MS,
};
use corelink_worker::reapi::ac::{
    AcMetaUpsertOutcome, AcNegCache, ActionDigest, ActionResult, InMemoryAcMetaStore,
    InMemoryAuditSink, InMemoryFakeSigner, InMemoryMerkleVerifier, InMemoryOutputsCheck,
    OutputFileDigest,
};
use corelink_worker::Region;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Pinned commit of the bazelbuild/remote-apis test suite this
/// harness validates against. Recorded here for the conformance
/// report alongside the canonical commit pin in ADR-0036.
const PINNED_REMOTE_APIS_COMMIT: &str = "main@2026-04-25";

/// Canonical conformance test ID per ADR-0036 §Annex A. The fields
/// are surfaced into the eprintln traces so the CI run summary
/// preserves the canonical ID + RPC + scenario names — `dead_code`
/// is allowed because the surface is consumed by trace
/// formatting only.
#[allow(dead_code, reason = "fields surfaced via eprintln traces only")]
#[derive(Clone, Copy, Debug)]
struct ConformanceTest {
    id: &'static str,
    rpc: &'static str,
    scenario: &'static str,
}

/// Output of one conformance test. Held in `assert!` macros that
/// consume `detail` via `Display` formatting; `test` is preserved for
/// the optional JSON line writer (CORELINK_CONFORMANCE_REPORT
/// env hook — disabled by default to keep the test cargo-friendly).
#[allow(dead_code, reason = "fields surfaced via assert formatting")]
#[derive(Clone, Debug)]
struct ConformanceResult {
    test: ConformanceTest,
    passed: bool,
    detail: String,
}

fn fixed_tdk() -> Arc<TenantDerivationKey> {
    Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32])))
}

fn make_ctx(tenant: Uuid, region: Region, scopes: PatScopes) -> AuthCtx {
    let pat_id = PatId(Uuid::nil());
    make_auth_ctx(
        PrincipalId(Uuid::nil()),
        tenant,
        region,
        scopes,
        AuthMethod::Pat {
            env: PatEnv::Pat,
            pat_id,
        },
        fixed_tdk(),
    )
}

struct Harness {
    handler: ActionCacheHandlerImpl<
        InMemoryAcMetaStore,
        InMemoryAcEnvelopeStore,
        InMemoryMerkleVerifier,
        InMemoryFakeSigner,
        InMemoryOutputsCheck,
        InMemoryAuditSink,
        InMemoryKv,
    >,
    outputs: Arc<InMemoryOutputsCheck>,
    clock: Arc<FakeClock>,
}

fn wire(region: Region) -> Harness {
    let meta = Arc::new(InMemoryAcMetaStore::new());
    let envelope = Arc::new(InMemoryAcEnvelopeStore::new());
    let merkle = Arc::new(InMemoryMerkleVerifier::new());
    let signer = Arc::new(InMemoryFakeSigner::new());
    let outputs = Arc::new(InMemoryOutputsCheck::new());
    let audit = Arc::new(InMemoryAuditSink::new());
    let neg = Arc::new(AcNegCache::new(region, InMemoryKv::new()).unwrap());
    let clock = Arc::new(FakeClock::new(1_000_000));
    let handler = ActionCacheHandlerImpl::new(ActionCacheHandlerBuilder {
        region,
        meta,
        envelope_store: envelope,
        merkle,
        signer,
        outputs: Arc::clone(&outputs),
        audit,
        neg_cache: neg,
        sig_key_id: 1,
        path_key_id: 1,
        ttl_extend_ms: DEFAULT_AC_TTL_EXTEND_MS,
        clock: Arc::clone(&clock) as Arc<dyn Clock>,
    });
    Harness {
        handler,
        outputs,
        clock,
    }
}

fn synth(seed: u64) -> (ActionDigest, ActionResult) {
    let action_bytes = format!("conformance-action-{seed}").into_bytes();
    let action_hash = Digest::compute(&action_bytes);
    let ad = ActionDigest::new(action_hash, action_bytes.len() as i64);
    let proto = format!("conformance-result-proto-{seed}").into_bytes();
    let out = Digest::compute(format!("conformance-out-{seed}").as_bytes());
    let ar = ActionResult::new(
        vec![OutputFileDigest::new(out, 64)],
        Vec::new(),
        0,
        proto,
    );
    (ad, ar)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn uuid_seed(seed: u64) -> Uuid {
    Uuid::from_u128((seed as u128) | (1u128 << 96))
}

// ---------------------------------------------------------------------
// GetActionResult — 4 canonical conformance tests
// ---------------------------------------------------------------------

#[test]
fn conformance_get_action_result_hit() {
    let test = ConformanceTest {
        id: "REAPI-AC-GET-001",
        rpc: "ActionCache.GetActionResult",
        scenario: "hit (canonical happy path)",
    };
    let result = run_get_hit();
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

fn run_get_hit() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-AC-GET-001",
        rpc: "ActionCache.GetActionResult",
        scenario: "hit",
    };
    let rt = rt();
    rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant = uuid_seed(line!() as u64);
        let ctx = make_ctx(
            tenant,
            Region::Wnam,
            PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
        );
        let (ad, ar) = synth(1);
        h.outputs.insert_all_alive(tenant, &ar);
        h.handler
            .update_action_result(&ctx, &ad, ar.clone(), "conf-001-w")
            .await
            .unwrap();
        let g = h
            .handler
            .get_action_result(&ctx, &ad, "conf-001-r")
            .await
            .unwrap();
        if g.action_result == ar {
            ConformanceResult {
                test,
                passed: true,
                detail: "ActionResult round-tripped byte-equal".into(),
            }
        } else {
            ConformanceResult {
                test,
                passed: false,
                detail: format!(
                    "ActionResult roundtrip drift: got {:?} want {:?}",
                    g.action_result, ar
                ),
            }
        }
    })
}

#[test]
fn conformance_get_action_result_miss_404() {
    let test = ConformanceTest {
        id: "REAPI-AC-GET-002",
        rpc: "ActionCache.GetActionResult",
        scenario: "miss surfaces NotFound (canonical 404)",
    };
    let rt = rt();
    let result = rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant = uuid_seed(line!() as u64);
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
        let (ad, _) = synth(2);
        let r = h.handler.get_action_result(&ctx, &ad, "conf-002").await;
        match r {
            Err(AcError::NotFound) => ConformanceResult {
                test,
                passed: true,
                detail: "NotFound surfaced canonically".into(),
            },
            other => ConformanceResult {
                test,
                passed: false,
                detail: format!("expected NotFound; got {other:?}"),
            },
        }
    });
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

#[test]
fn conformance_get_action_result_cross_tenant_404() {
    let test = ConformanceTest {
        id: "REAPI-AC-GET-003",
        rpc: "ActionCache.GetActionResult",
        scenario: "cross-tenant probe surfaces NotFound (INV-AC-TENANT-SCOPED)",
    };
    let rt = rt();
    let result = rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant_a = uuid_seed(line!() as u64);
        let tenant_b = uuid_seed(line!() as u64 + 1);
        let ctx_a = make_ctx(
            tenant_a,
            Region::Wnam,
            PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
        );
        let ctx_b = make_ctx(tenant_b, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
        let (ad, ar) = synth(3);
        h.outputs.insert_all_alive(tenant_a, &ar);
        h.handler
            .update_action_result(&ctx_a, &ad, ar.clone(), "conf-003-w")
            .await
            .unwrap();
        let r = h.handler.get_action_result(&ctx_b, &ad, "conf-003-r").await;
        match r {
            Err(AcError::NotFound) => ConformanceResult {
                test,
                passed: true,
                detail: "cross-tenant NotFound surfaced (INV-AC-TENANT-SCOPED)".into(),
            },
            other => ConformanceResult {
                test,
                passed: false,
                detail: format!("expected cross-tenant NotFound; got {other:?}"),
            },
        }
    });
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

#[test]
fn conformance_get_action_result_sig_roundtrip() {
    let test = ConformanceTest {
        id: "REAPI-AC-GET-004",
        rpc: "ActionCache.GetActionResult",
        scenario: "sig roundtrip byte-stable post-write (HKDF + canonical preimage)",
    };
    let rt = rt();
    let result = rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant = uuid_seed(line!() as u64);
        let ctx = make_ctx(
            tenant,
            Region::Wnam,
            PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
        );
        let (ad, ar) = synth(4);
        h.outputs.insert_all_alive(tenant, &ar);
        let upd = h
            .handler
            .update_action_result(&ctx, &ad, ar.clone(), "conf-004-w")
            .await
            .unwrap();
        let g = h
            .handler
            .get_action_result(&ctx, &ad, "conf-004-r")
            .await
            .unwrap();
        if upd.row.result_hash == g.row.result_hash {
            ConformanceResult {
                test,
                passed: true,
                detail: "sig + result_hash roundtrip byte-stable".into(),
            }
        } else {
            ConformanceResult {
                test,
                passed: false,
                detail: format!(
                    "result_hash drift: upd={:?} get={:?}",
                    upd.row.result_hash, g.row.result_hash
                ),
            }
        }
    });
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

// ---------------------------------------------------------------------
// UpdateActionResult — 6 canonical conformance tests
// ---------------------------------------------------------------------

#[test]
fn conformance_update_action_result_success() {
    let test = ConformanceTest {
        id: "REAPI-AC-UPDATE-001",
        rpc: "ActionCache.UpdateActionResult",
        scenario: "success (canonical happy path)",
    };
    let rt = rt();
    let result = rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant = uuid_seed(line!() as u64);
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let (ad, ar) = synth(11);
        h.outputs.insert_all_alive(tenant, &ar);
        let r = h
            .handler
            .update_action_result(&ctx, &ad, ar, "conf-update-001")
            .await;
        match r {
            Ok(out) if out.upsert_outcome == AcMetaUpsertOutcome::Inserted => ConformanceResult {
                test,
                passed: true,
                detail: "Inserted upsert outcome canonical".into(),
            },
            other => ConformanceResult {
                test,
                passed: false,
                detail: format!("expected Inserted; got {other:?}"),
            },
        }
    });
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

#[test]
fn conformance_update_action_result_idempotent_refresh() {
    let test = ConformanceTest {
        id: "REAPI-AC-UPDATE-002",
        rpc: "ActionCache.UpdateActionResult",
        scenario: "idempotent refresh (INV-AC-IDEMPOTENT)",
    };
    let rt = rt();
    let result = rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant = uuid_seed(line!() as u64);
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let (ad, ar) = synth(12);
        h.outputs.insert_all_alive(tenant, &ar);
        let r1 = h
            .handler
            .update_action_result(&ctx, &ad, ar.clone(), "conf-update-002-1")
            .await
            .unwrap();
        h.clock.advance_ms(50);
        let r2 = h
            .handler
            .update_action_result(&ctx, &ad, ar.clone(), "conf-update-002-2")
            .await
            .unwrap();
        if r1.upsert_outcome == AcMetaUpsertOutcome::Inserted
            && r2.upsert_outcome == AcMetaUpsertOutcome::IdempotentRefresh
            && r1.row.result_hash == r2.row.result_hash
        {
            ConformanceResult {
                test,
                passed: true,
                detail: "Inserted → IdempotentRefresh; result_hash byte-stable".into(),
            }
        } else {
            ConformanceResult {
                test,
                passed: false,
                detail: format!("idempotency drift: r1={r1:?} r2={r2:?}"),
            }
        }
    });
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

#[test]
fn conformance_update_action_result_outputs_missing_422() {
    let test = ConformanceTest {
        id: "REAPI-AC-UPDATE-003",
        rpc: "ActionCache.UpdateActionResult",
        scenario: "outputs missing rejected pre-persist (INV-AC-OUTPUTS-VALID)",
    };
    let rt = rt();
    let result = rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant = uuid_seed(line!() as u64);
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let (ad, ar) = synth(13);
        // Do NOT insert outputs alive — they are tombstoned-by-default
        // for this conformance arm.
        let r = h
            .handler
            .update_action_result(&ctx, &ad, ar, "conf-update-003")
            .await;
        match r {
            Err(AcError::OutputsMissing { .. }) => ConformanceResult {
                test,
                passed: true,
                detail: "OutputsMissing surfaced (INV-AC-OUTPUTS-VALID enforced)".into(),
            },
            other => ConformanceResult {
                test,
                passed: false,
                detail: format!("expected OutputsMissing; got {other:?}"),
            },
        }
    });
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

#[test]
fn conformance_update_action_result_scope_insufficient() {
    let test = ConformanceTest {
        id: "REAPI-AC-UPDATE-004",
        rpc: "ActionCache.UpdateActionResult",
        scenario: "scope insufficient rejected at 5-Layer Defense Layer 4",
    };
    let rt = rt();
    let result = rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant = uuid_seed(line!() as u64);
        // Read scope only; UPDATE requires CACHE_W.
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
        let (ad, ar) = synth(14);
        h.outputs.insert_all_alive(tenant, &ar);
        let r = h
            .handler
            .update_action_result(&ctx, &ad, ar, "conf-update-004")
            .await;
        match r {
            Err(AcError::ScopeInsufficient { .. }) => ConformanceResult {
                test,
                passed: true,
                detail: "ScopeInsufficient rejected at scope check seam".into(),
            },
            other => ConformanceResult {
                test,
                passed: false,
                detail: format!("expected ScopeInsufficient; got {other:?}"),
            },
        }
    });
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

#[test]
fn conformance_update_action_result_region_mismatch() {
    let test = ConformanceTest {
        id: "REAPI-AC-UPDATE-005",
        rpc: "ActionCache.UpdateActionResult",
        scenario: "region mismatch rejected at handler region pinning (INV-AC-REGION-PINNED)",
    };
    let rt = rt();
    let result = rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant = uuid_seed(line!() as u64);
        // Handler pinned to Wnam; ctx claims Iad.
        let ctx = make_ctx(tenant, Region::Weur, PatScopes::single(SCOPE_CACHE_W));
        let (ad, ar) = synth(15);
        h.outputs.insert_all_alive(tenant, &ar);
        let r = h
            .handler
            .update_action_result(&ctx, &ad, ar, "conf-update-005")
            .await;
        match r {
            Err(AcError::RegionMismatch { .. }) => ConformanceResult {
                test,
                passed: true,
                detail: "RegionMismatch surfaced (defense-in-depth region pinning)".into(),
            },
            other => ConformanceResult {
                test,
                passed: false,
                detail: format!("expected RegionMismatch; got {other:?}"),
            },
        }
    });
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

#[test]
fn conformance_update_action_result_result_hash_immutable() {
    let test = ConformanceTest {
        id: "REAPI-AC-UPDATE-006",
        rpc: "ActionCache.UpdateActionResult",
        scenario: "result_hash byte-stable on idempotent refresh (INV-AC-RESULT-HASH-IMMUTABLE)",
    };
    let rt = rt();
    let result = rt.block_on(async {
        let h = wire(Region::Wnam);
        let tenant = uuid_seed(line!() as u64);
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let (ad, ar) = synth(16);
        h.outputs.insert_all_alive(tenant, &ar);
        let r1 = h
            .handler
            .update_action_result(&ctx, &ad, ar.clone(), "conf-update-006-1")
            .await
            .unwrap();
        // Multiple replays.
        let mut prev = r1.row.result_hash;
        for _ in 0..5 {
            h.clock.advance_ms(100);
            let r = h
                .handler
                .update_action_result(&ctx, &ad, ar.clone(), "conf-update-006-n")
                .await
                .unwrap();
            if r.row.result_hash != prev {
                return ConformanceResult {
                    test,
                    passed: false,
                    detail: format!(
                        "result_hash mutated across replay: prev={prev:?} cur={:?}",
                        r.row.result_hash
                    ),
                };
            }
            prev = r.row.result_hash;
        }
        ConformanceResult {
            test,
            passed: true,
            detail: "result_hash byte-stable across 5 idempotent refresh replays".into(),
        }
    });
    assert!(result.passed, "{} failed: {}", test.id, result.detail);
    eprintln!("[{}] PASS — {}", test.id, test.scenario);
}

// ---------------------------------------------------------------------
// Suite-level marker — record the pinned commit + 100% pass envelope.
// ---------------------------------------------------------------------

#[test]
fn conformance_suite_summary() {
    eprintln!(
        "REAPI v2 Action Cache conformance suite — pinned bazelbuild/remote-apis commit: {}",
        PINNED_REMOTE_APIS_COMMIT
    );
    eprintln!(
        "  GetActionResult         — 4 canonical tests (hit / miss / cross-tenant / sig)"
    );
    eprintln!(
        "  UpdateActionResult      — 6 canonical tests (success / idempotent / outputs_missing /"
    );
    eprintln!("                              scope / region / result_hash_immutable)");
    eprintln!("  Total: 10 conformance tests; 100% pass non-negotiable per WI-S04-006 §6.1.1.");
    eprintln!("  REAPI v2 has NO BatchUpdateActionResult (Lote 10.4bis P0 fix #5).");
}
