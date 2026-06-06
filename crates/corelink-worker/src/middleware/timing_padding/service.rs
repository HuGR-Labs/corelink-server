//! [`TimingPaddingService`] — Tower `Service` wrapping an inner
//! HTTP/gRPC service to enforce the miss-response timing-padding
//! policy.
//!
//! Split from monolith `middleware/timing_padding.rs` (wave-33 stage
//! 2.PRE-A.3).

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use http::{Request, Response};
use tower_service::Service;

use super::config::TimingPaddingConfig;
use super::padding::{canonical_pad_target, compute_request_seed};
use super::policy::{JitterPolicy, MissMarker};
use super::predicate::PredicateKind;

/// Tower [`Service`] wrapping an inner HTTP/gRPC service to enforce
/// the miss-response timing-padding policy.
#[derive(Clone)]
pub struct TimingPaddingService<S> {
    inner: S,
    config: TimingPaddingConfig,
    policy: JitterPolicy,
    server_secret: u64,
    call_counter: Arc<AtomicU64>,
    predicate_kind: PredicateKind,
}

impl<S: core::fmt::Debug> core::fmt::Debug for TimingPaddingService<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TimingPaddingService")
            .field("inner", &self.inner)
            .field("config", &self.config)
            .field("policy", &self.policy)
            .field("predicate_kind", &self.predicate_kind)
            .field("server_secret", &"<REDACTED>")
            .finish()
    }
}

impl<S> TimingPaddingService<S> {
    /// Crate-internal constructor used by
    /// [`super::layer::TimingPaddingLayer::layer`]. Public API stays
    /// `with_predicate(...)`-based; this avoids exposing the internal
    /// (config, policy, secret, counter, predicate) tuple.
    #[doc(hidden)]
    pub(super) fn __new_from_layer(
        inner: S,
        config: TimingPaddingConfig,
        policy: JitterPolicy,
        server_secret: u64,
        call_counter: Arc<AtomicU64>,
        predicate_kind: PredicateKind,
    ) -> Self {
        Self {
            inner,
            config,
            policy,
            server_secret,
            call_counter,
            predicate_kind,
        }
    }

    /// Read-only access to the config.
    #[must_use]
    pub const fn config(&self) -> &TimingPaddingConfig {
        &self.config
    }

    /// Read-only access to the active [`PredicateKind`].
    #[must_use]
    pub const fn predicate_kind(&self) -> PredicateKind {
        self.predicate_kind
    }

    /// Replace the [`PredicateKind`] used to decide whether a
    /// response should be padded. Production wiring picks
    /// [`PredicateKind::Http404`] for the HTTP REST stack and
    /// [`PredicateKind::GrpcNotFound`] for the tonic gRPC stack
    /// (the latter inspects `grpc-status: 5` initial response
    /// header, the canonical tonic `Status::into_http` encoding for
    /// `Err(Status::not_found)`); the default after
    /// [`tower_layer::Layer::layer`] is [`PredicateKind::Any`] which
    /// fires on any of the three canonical signals.
    #[must_use]
    pub fn with_predicate(mut self, predicate_kind: PredicateKind) -> Self {
        self.predicate_kind = predicate_kind;
        self
    }
}

impl<S, ReqBody, RespBody> Service<Request<ReqBody>> for TimingPaddingService<S>
where
    S: Service<Request<ReqBody>, Response = Response<RespBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    ReqBody: Send + 'static,
    RespBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        // Per Tower's contract, `poll_ready` drives readiness on
        // `&mut self.inner`; `call` MUST consume that readied state
        // on the same `&mut S` (codex round-1 P1 fix — earlier draft
        // cloned `self.inner` and called the clone, bypassing
        // back-pressure on stateful inner services).
        //
        // The canonical fix per `tower::ServiceExt::ready` and the
        // `tower::Buffer` blueprint: swap the readied service into
        // the future via `core::mem::replace`. The fresh `not-ready`
        // clone left in `self.inner` will be re-polled on the next
        // `poll_ready` call, preserving the "exactly one `poll_ready`
        // per `call`" invariant for stateful inner services.
        let started = tokio::time::Instant::now();
        let request_id = compute_request_seed(&req, self.server_secret, &self.call_counter);
        let config = self.config;
        let policy = self.policy.clone();
        let predicate_kind = self.predicate_kind;
        // Swap the readied inner OUT and replace with a fresh clone
        // that the next `poll_ready` call will drive. This is the
        // canonical pattern from `tower-http::map_response::MapResponse`
        // + `tower::Buffer::poll_ready` + `tower::ServiceBuilder`
        // — it preserves Tower's readiness contract for stateful
        // inner services (codex round-1 P1 fix).
        let clone = self.inner.clone();
        let mut inner = core::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            let response = inner.call(req).await?;
            if predicate_kind.matches(&response) {
                let pre_pad_elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                let pad_target =
                    canonical_pad_target(config, &policy, request_id, started.elapsed());
                // Saturating add — `tokio::time::Instant` can in
                // theory overflow; surface the inner `started`
                // unchanged so the sleep collapses to a no-op rather
                // than panicking (the strict-lints crate forbids
                // panics).
                let deadline = started.checked_add(pad_target).unwrap_or(started);
                tokio::time::sleep_until(deadline).await;
                // Codex round-4 P1 fix + round-5 P1 fix — emit the
                // canonical observability hook with the per-arm
                // discriminator so S-09 aggregation can reconstruct
                // `corelink_cas_side_channel_timing_diff_ms` as
                // pairwise medians per arm. The
                // `MissMarker { arm: Option<MissArm> }` extension is
                // inserted by the handler before returning;
                // `miss_arm` is `unknown` when the handler did not
                // classify the arm (still padded, but without per-arm
                // attribution).
                let total_elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                // Codex round-5 P1: read the canonical arm
                // discriminator first from the `MissMarker` extension
                // (HTTP path; tonic does not propagate extensions
                // through `Status::into_http`), THEN from the
                // `x-corelink-miss-arm` response header (gRPC path
                // that emits the arm via Status metadata + tonic's
                // metadata-to-header transform). The fallback is
                // `unknown` (still padded; just unattributed).
                let miss_arm = response
                    .extensions()
                    .get::<MissMarker>()
                    .and_then(|m| m.arm)
                    .map_or_else(
                        || {
                            response
                                .headers()
                                .get("x-corelink-miss-arm")
                                .and_then(|v| v.to_str().ok())
                                .map(|s| s.trim().to_owned())
                                .unwrap_or_else(|| "unknown".to_owned())
                        },
                        |a| a.label().to_owned(),
                    );
                tracing::event!(
                    target: "corelink.cas.side_channel",
                    tracing::Level::DEBUG,
                    pre_pad_elapsed_ms = pre_pad_elapsed_ms,
                    pad_target_ms = pad_target.as_secs_f64() * 1000.0,
                    total_elapsed_ms = total_elapsed_ms,
                    target_p99_ms = config.target_p99_ms(),
                    jitter_pct = config.jitter_pct(),
                    request_id_seed = request_id,
                    miss_arm = miss_arm,
                    "corelink.cas.side_channel.timing_padded"
                );
            }
            Ok(response)
        })
    }
}
