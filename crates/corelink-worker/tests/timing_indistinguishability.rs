//! WI-S02-004 §10.4.1 + §6.1.3 — adversarial 3-arm 404 timing
//! indistinguishability gate.
//!
//! ## Test plan (per WI §6.1.3 + ADR-0023 §3)
//!
//! 1. Three handler arms simulate the canonical 404 [`MissReason`]
//!    paths from `corelink-reapi::read::MissReason`:
//!    - `NeverExisted` — fast (KV negative-cache hit + AuthZ check) ≈ 50 ms.
//!    - `Tombstoned`   — medium (D1 row + soft-delete check) ≈ 60 ms.
//!    - `R2OrphanRow`  — slow (D1 row + AuthZ + R2 GET round-trip) ≈ 80 ms.
//!
//!    Each arm has a small Gaussian-style tail (uniform jitter ±15 ms)
//!    so the per-handler distributions are not delta functions —
//!    closer to production where R2/D1 RTT scatter is real.
//! 2. The padded latency observed by the client = padding middleware
//!    target ± jitter (200 ms ± 10 %, per [`TimingPaddingConfig::canonical`]).
//! 3. We collect 10 000 samples per arm for each of 3 independent
//!    trials (different per-trial seeds; CI flake mitigation).
//! 4. Mann-Whitney U pairwise (3 pairs / trial × 3 trials = 9 tests).
//! 5. Acceptance gate (codex round-1 P1 fix — earlier draft accepted
//!    a strictly weaker form):
//!    - **Strict per-test gate**: `p > sidak_per_test_alpha(0.05, 9)`
//!      ≈ 0.005 685 8. ALL 9 tests must pass; combined familywise α
//!      stays at 0.05 target via Šidák correction.
//!    - **Practical-equivalence gate** on every pair's bootstrap
//!      95 % CI on `|Δmedian|`:
//!      - `point_estimate ≤ 1 ms` AND
//!      - `ci_upper ≤ 1 ms` (the load-bearing claim — a CI whose
//!        upper bound stays inside 1 ms is the strict equivalence
//!        evidence; `ci_lower` of `|·|` is trivially ≥ 0 and was
//!        redundant in earlier drafts).
//! 6. **Negative-control trial × 3** (codex round-1 P1 fix): the
//!    handler-only baseline is asserted to **fail the SAME 9-test
//!    gate** (i.e. `≥ 1` of 9 pair-tests rejects via Šidák) — proving
//!    the padding is the load-bearing defense, not a property of the
//!    test fixture.
//!
//! ## Why simulated time
//!
//! Wall-clock latencies on a busy CI runner produce a high-variance
//! background distribution that swamps any 1 ms practical-equivalence
//! threshold. We use `tokio::time::pause` + virtual `Instant::now()`
//! advancement (`#[tokio::test(start_paused = true)]`) so the
//! recorded latencies reflect ONLY the padding middleware's behaviour
//! against the simulated handler arms — the test exercises the math,
//! not CI runner noise. Production validation runs in S-09 chaos
//! (real Cloudflare deploy + Mann-Whitney over 7-day production
//! sample) per `chaos_experiments.md`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::print_stderr,
    clippy::type_complexity,
    reason = "test code: panics surface as test failures by design; eprintln! emits informational stats to the test log; type complexity in run_gate's tuple return reflects 9-test gate semantics"
)]

use std::convert::Infallible;
use std::sync::Arc;

use http::{Request, Response, StatusCode};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha20Rng;
use tower::{Service, ServiceExt};
use tower_layer::Layer;

use corelink_worker::middleware::{
    bootstrap_median_ci, mann_whitney_u_p_value, sidak_per_test_alpha, JitterPolicy, MissArm,
    MissMarker, PredicateKind, TimingPaddingConfig, TimingPaddingLayer,
};

/// Body type used everywhere in this test — concrete unit body keeps
/// the Service bounds simple (no generic `B: Default + Send` plumbing
/// that confuses tower's ready() inference).
type TestBody = ();

// Sample-size and acceptance-gate constants — keep in sync with
// WI-S02-004 §10.4.1 and ADR-0023 §3.
const SAMPLES_PER_ARM: usize = 10_000;
const TRIALS: usize = 3;
const ALPHA: f64 = 0.05;
const PER_TEST_TOTAL: usize = TRIALS * 3; // 3 trials × 3 pairs
const MEDIAN_DIFF_LIMIT_MS: f64 = 1.0;
const BOOTSTRAP_ITERATIONS: usize = 200;

#[derive(Clone, Copy, Debug)]
enum Arm {
    NeverExisted,
    Tombstoned,
    R2OrphanRow,
}

/// Simulated 404 handler — different baseline + spread per arm so the
/// **unpadded** distributions are statistically distinguishable. The
/// timing-padding middleware has to flatten them into a single
/// distribution.
fn simulated_handler_resolution_ms(arm: Arm, rng: &mut ChaCha20Rng) -> f64 {
    let (base, spread) = match arm {
        Arm::NeverExisted => (50.0, 15.0),
        Arm::Tombstoned => (60.0, 15.0),
        Arm::R2OrphanRow => (80.0, 15.0),
    };
    base + rng.random_range(0.0..spread)
}

/// Inner Tower service: sleep for the per-arm simulated handler
/// duration, then return a 404 response with empty body.
#[derive(Clone)]
struct FakeHandler {
    arm: Arc<std::sync::Mutex<Option<Arm>>>,
    rng: Arc<std::sync::Mutex<ChaCha20Rng>>,
}

impl FakeHandler {
    fn new(seed: u64) -> Self {
        Self {
            arm: Arc::new(std::sync::Mutex::new(None)),
            rng: Arc::new(std::sync::Mutex::new(ChaCha20Rng::seed_from_u64(seed))),
        }
    }
    fn set_arm(&self, arm: Arm) {
        *self.arm.lock().unwrap() = Some(arm);
    }
}

impl Service<Request<TestBody>> for FakeHandler {
    type Response = Response<TestBody>;
    type Error = Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<TestBody>) -> Self::Future {
        let arm = self
            .arm
            .lock()
            .unwrap()
            .expect("arm must be set before call");
        let mut rng = self.rng.lock().unwrap();
        let resolution_ms = simulated_handler_resolution_ms(arm, &mut rng);
        drop(rng);
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_micros(
                (resolution_ms * 1000.0) as u64,
            ))
            .await;
            let mut resp = Response::new(());
            *resp.status_mut() = StatusCode::NOT_FOUND;
            // Defense-in-depth: the production handler inserts the
            // marker on every 404; mirror that here so the layer's
            // `PredicateKind::Any` path is exercised on both signals.
            // Insert per-arm MissMarker so the timing-padding emit
            // hook can attribute the padded latency to the canonical
            // `MissArm` per the S-09 aggregation contract.
            let m_arm = match arm {
                Arm::NeverExisted => MissArm::NeverExisted,
                Arm::Tombstoned => MissArm::Tombstoned,
                Arm::R2OrphanRow => MissArm::R2OrphanRow,
            };
            resp.extensions_mut().insert(MissMarker::for_arm(m_arm));
            Ok(resp)
        })
    }
}

async fn run_arm_padded(
    handler: FakeHandler,
    layer: TimingPaddingLayer,
    arm: Arm,
    samples: usize,
    request_id_offset: u64,
) -> Vec<f64> {
    handler.set_arm(arm);
    let mut svc = layer.layer(handler);
    let mut latencies = Vec::with_capacity(samples);
    for i in 0..samples {
        let id = request_id_offset.wrapping_add(i as u64);
        let req = Request::builder()
            .header("x-request-id", format!("req-{id:016x}"))
            .body(())
            .unwrap();
        let start = tokio::time::Instant::now();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        latencies.push(elapsed.as_secs_f64() * 1000.0);
    }
    latencies
}

async fn run_arm_unpadded(handler: FakeHandler, arm: Arm, samples: usize) -> Vec<f64> {
    handler.set_arm(arm);
    let mut svc = handler;
    let mut latencies = Vec::with_capacity(samples);
    for _ in 0..samples {
        let req = Request::builder().body(()).unwrap();
        let start = tokio::time::Instant::now();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        latencies.push(elapsed.as_secs_f64() * 1000.0);
    }
    latencies
}

/// Run all 3 arms × `samples` for one trial, returning per-arm
/// vectors of padded latency samples (in milliseconds).
async fn one_trial_padded(
    layer: TimingPaddingLayer,
    handler_seed: u64,
    samples: usize,
) -> [Vec<f64>; 3] {
    let h_a = FakeHandler::new(handler_seed.wrapping_add(0xA1));
    let h_b = FakeHandler::new(handler_seed.wrapping_add(0xB2));
    let h_c = FakeHandler::new(handler_seed.wrapping_add(0xC3));
    let arm_ne = run_arm_padded(
        h_a,
        layer.clone(),
        Arm::NeverExisted,
        samples,
        handler_seed << 32,
    )
    .await;
    let arm_tb = run_arm_padded(
        h_b,
        layer.clone(),
        Arm::Tombstoned,
        samples,
        (handler_seed << 32) | 1,
    )
    .await;
    let arm_or = run_arm_padded(
        h_c,
        layer,
        Arm::R2OrphanRow,
        samples,
        (handler_seed << 32) | 2,
    )
    .await;
    [arm_ne, arm_tb, arm_or]
}

async fn one_trial_unpadded(handler_seed: u64, samples: usize) -> [Vec<f64>; 3] {
    let h_a = FakeHandler::new(handler_seed.wrapping_add(0xA1));
    let h_b = FakeHandler::new(handler_seed.wrapping_add(0xB2));
    let h_c = FakeHandler::new(handler_seed.wrapping_add(0xC3));
    let arm_ne = run_arm_unpadded(h_a, Arm::NeverExisted, samples).await;
    let arm_tb = run_arm_unpadded(h_b, Arm::Tombstoned, samples).await;
    let arm_or = run_arm_unpadded(h_c, Arm::R2OrphanRow, samples).await;
    [arm_ne, arm_tb, arm_or]
}

/// Run the 9-test gate on a per-trial × per-pair latency matrix.
/// Returns `(p_values, ci_per_pair, n_failed_strict, n_failed_practical)`.
fn run_gate(
    trials: &[[Vec<f64>; 3]],
    bootstrap_seed_offset: u64,
) -> (
    Vec<(usize, usize, f64)>,
    Vec<(usize, usize, corelink_worker::middleware::BootstrapMedianCi)>,
    usize,
    usize,
) {
    let alpha_prime = sidak_per_test_alpha(ALPHA, PER_TEST_TOTAL).unwrap();
    let mut p_values = Vec::with_capacity(PER_TEST_TOTAL);
    let mut cis = Vec::with_capacity(PER_TEST_TOTAL);
    let mut n_failed_strict = 0usize;
    let mut n_failed_practical = 0usize;
    for (trial_idx, arms) in trials.iter().enumerate() {
        let pairs: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];
        for (i, (a, b)) in pairs.iter().copied().enumerate() {
            let p = mann_whitney_u_p_value(&arms[a], &arms[b]).expect("non-empty arms");
            let ci = bootstrap_median_ci(
                &arms[a],
                &arms[b],
                BOOTSTRAP_ITERATIONS,
                bootstrap_seed_offset
                    .wrapping_add(trial_idx as u64 * 7)
                    .wrapping_add(i as u64),
            )
            .expect("non-empty arms");
            p_values.push((trial_idx, i, p));
            cis.push((trial_idx, i, ci));
            // Strict gate: per-test α' (Šidák).
            if p <= alpha_prime {
                n_failed_strict += 1;
            }
            // Practical-equivalence gate.
            if ci.point_estimate > MEDIAN_DIFF_LIMIT_MS || ci.ci_upper > MEDIAN_DIFF_LIMIT_MS {
                n_failed_practical += 1;
            }
        }
    }
    (p_values, cis, n_failed_strict, n_failed_practical)
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn three_arm_indistinguishability_with_padding() {
    let layer = TimingPaddingLayer::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded);

    let mut all_arms = Vec::with_capacity(TRIALS);
    for trial in 0..TRIALS {
        let arms = one_trial_padded(
            layer.clone(),
            (trial as u64).wrapping_mul(0xA5A5_A5A5_A5A5_A5A5),
            SAMPLES_PER_ARM,
        )
        .await;
        all_arms.push(arms);
    }

    let alpha_prime = sidak_per_test_alpha(ALPHA, PER_TEST_TOTAL).unwrap();
    let (p_values, cis, n_failed_strict, n_failed_practical) = run_gate(&all_arms, 0xC1A0_BEEF);

    eprintln!("Šidák per-test α' for k={PER_TEST_TOTAL}: {alpha_prime:.6} (the strict gate)");
    for (trial, pair, p) in &p_values {
        eprintln!("trial {trial} pair {pair} p = {p:.6}");
    }
    for (trial, pair, ci) in &cis {
        eprintln!(
            "trial {trial} pair {pair} |Δmedian| point = {:.4} ms; CI = [{:.4}, {:.4}] ms",
            ci.point_estimate, ci.ci_lower, ci.ci_upper
        );
    }

    // Acceptance gate 1: strict p > α' (Šidák), all 9 tests.
    assert_eq!(
        n_failed_strict, 0,
        "{n_failed_strict} of 9 Mann-Whitney pair-tests rejected at α' = {alpha_prime:.6}; \
         distributions are NOT statistically indistinguishable",
    );
    // Acceptance gate 2: practical-equivalence — every pair's
    // bootstrap CI on |Δmedian| stays within the 1 ms limit.
    assert_eq!(
        n_failed_practical, 0,
        "{n_failed_practical} of 9 pair bootstrap CIs exceeded {MEDIAN_DIFF_LIMIT_MS} ms; \
         distributions are NOT practically equivalent",
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn three_arm_distinguishability_without_padding_baseline() {
    // Negative-control: the SAME 9-test gate must REJECT on the
    // unpadded baseline. Codex round-1 P1 fix — earlier draft only
    // checked one pair in one trial. Here we run the full 3 trials ×
    // 3 pairs = 9 pair-tests, and assert that ≥ 1 of them rejects.
    let mut all_arms = Vec::with_capacity(TRIALS);
    for trial in 0..TRIALS {
        let arms = one_trial_unpadded(
            (trial as u64).wrapping_mul(0xC0DE_F00D_BAAD_BEEF),
            SAMPLES_PER_ARM,
        )
        .await;
        all_arms.push(arms);
    }
    let alpha_prime = sidak_per_test_alpha(ALPHA, PER_TEST_TOTAL).unwrap();
    let (_p_values, _cis, n_failed_strict, n_failed_practical) = run_gate(&all_arms, 0xBAAD_F00D);
    eprintln!(
        "negative-control α' = {alpha_prime:.6}; strict failures = {n_failed_strict}; \
         practical-equivalence failures = {n_failed_practical}"
    );
    // Without padding, the simulated arms are ≥ 10 ms apart in
    // median. Both gates MUST fail on every pair (strict: 9/9
    // reject; practical: 9/9 exceed the 1 ms |Δmedian| ceiling).
    assert_eq!(
        n_failed_strict, PER_TEST_TOTAL,
        "unpadded distributions should be distinguishable on every pair; \
         strict failures = {n_failed_strict}/{PER_TEST_TOTAL}",
    );
    assert_eq!(
        n_failed_practical, PER_TEST_TOTAL,
        "unpadded |Δmedian| should exceed 1 ms on every pair; \
         practical failures = {n_failed_practical}/{PER_TEST_TOTAL}",
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn padding_skipped_for_200_ok() {
    let layer = TimingPaddingLayer::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded);
    let inner = tower::service_fn(|_: Request<()>| async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let mut resp = Response::new(());
        *resp.status_mut() = StatusCode::OK;
        Ok::<_, Infallible>(resp)
    });
    let mut svc = layer.layer(inner);
    let req = Request::builder().body(()).unwrap();
    let start = tokio::time::Instant::now();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        elapsed.as_millis() < 100,
        "200 response should NOT be padded to canonical 200 ms target; got {} ms",
        elapsed.as_millis()
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn padding_skipped_for_403_pat_scope() {
    let layer = TimingPaddingLayer::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded);
    let inner = tower::service_fn(|_: Request<()>| async move {
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        let mut resp = Response::new(());
        *resp.status_mut() = StatusCode::FORBIDDEN;
        Ok::<_, Infallible>(resp)
    });
    let mut svc = layer.layer(inner);
    let req = Request::builder().body(()).unwrap();
    let start = tokio::time::Instant::now();
    let resp = svc.ready().await.unwrap().call(req).await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(
        elapsed.as_millis() < 100,
        "403 should NOT be padded; got {} ms",
        elapsed.as_millis()
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn padding_skipped_for_500_internal() {
    let layer = TimingPaddingLayer::canonical();
    let inner = tower::service_fn(|_: Request<()>| async move {
        let mut resp = Response::new(());
        *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        Ok::<_, Infallible>(resp)
    });
    let mut svc = layer.layer(inner);
    let req = Request::builder().body(()).unwrap();
    let start = tokio::time::Instant::now();
    let _ = svc.ready().await.unwrap().call(req).await.unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 100,
        "500 should NOT be padded; got {} ms",
        elapsed.as_millis()
    );
}

/// gRPC-shaped response: HTTP 200 + `grpc-status: 5` initial header
/// (canonical tonic encoding of `Err(Status::not_found(...))`). With
/// the default `PredicateKind::Any` the layer MUST pad this — codex
/// round-1 P0 fix asserting parity between HTTP 404 and gRPC NOT_FOUND.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn padding_applied_for_grpc_not_found() {
    let layer = TimingPaddingLayer::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded);
    let inner = tower::service_fn(|_: Request<()>| async move {
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        let mut resp = Response::new(());
        *resp.status_mut() = StatusCode::OK; // gRPC: HTTP 200 always
        resp.headers_mut()
            .insert("grpc-status", "5".parse().unwrap());
        Ok::<_, Infallible>(resp)
    });
    let mut svc = layer.layer(inner);
    let req = Request::builder()
        .header("x-request-id", "grpc-probe")
        .body(())
        .unwrap();
    let start = tokio::time::Instant::now();
    let _ = svc.ready().await.unwrap().call(req).await.unwrap();
    let elapsed = start.elapsed().as_millis();
    // 60 ms handler + padding to 200 ms ± 10 % ⇒ [180, 220] ms.
    assert!(
        (180..=220).contains(&(elapsed as u64)),
        "gRPC NOT_FOUND should be padded to canonical window; got {elapsed} ms",
    );
}

/// gRPC `grpc-status: 0` (OK) MUST NOT be padded — the gRPC happy
/// path stays unpadded.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn padding_skipped_for_grpc_ok() {
    let layer = TimingPaddingLayer::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded);
    let inner = tower::service_fn(|_: Request<()>| async move {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let mut resp = Response::new(());
        *resp.status_mut() = StatusCode::OK;
        resp.headers_mut()
            .insert("grpc-status", "0".parse().unwrap());
        Ok::<_, Infallible>(resp)
    });
    let mut svc = layer.layer(inner);
    let req = Request::builder().body(()).unwrap();
    let start = tokio::time::Instant::now();
    let _ = svc.ready().await.unwrap().call(req).await.unwrap();
    let elapsed = start.elapsed().as_millis();
    assert!(
        elapsed < 100,
        "gRPC OK should NOT be padded; got {elapsed} ms",
    );
}

/// `with_predicate(GrpcNotFound)` excludes HTTP 404 from padding —
/// useful for stacks that ONLY serve gRPC and want strictly typed
/// padding (defense-in-depth: a leaky HTTP 404 from an upstream
/// reverse-proxy stays unpadded since this stack doesn't emit it).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn predicate_grpc_only_excludes_http_404() {
    let layer = TimingPaddingLayer::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded);
    let inner = tower::service_fn(|_: Request<()>| async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let mut resp = Response::new(());
        *resp.status_mut() = StatusCode::NOT_FOUND;
        Ok::<_, Infallible>(resp)
    });
    let svc_padding = layer.layer(inner);
    let mut svc = svc_padding.with_predicate(PredicateKind::GrpcNotFound);
    let req = Request::builder().body(()).unwrap();
    let start = tokio::time::Instant::now();
    let _ = svc.ready().await.unwrap().call(req).await.unwrap();
    let elapsed = start.elapsed().as_millis();
    assert!(
        elapsed < 100,
        "GrpcNotFound predicate should ignore HTTP 404; got {elapsed} ms",
    );
}
