//! [`TimingPaddingLayer`] — Tower `Layer` that applies the timing-
//! padding policy to wrapped services.
//!
//! Split from monolith `middleware/timing_padding.rs` (wave-33 stage
//! 2.PRE-A.3).

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use tower_layer::Layer;

use super::config::TimingPaddingConfig;
use super::padding::generate_server_secret;
use super::policy::JitterPolicy;
use super::predicate::PredicateKind;
use super::service::TimingPaddingService;

/// Tower [`Layer`] applying the timing-padding policy to wrapped services.
///
/// See module-level rustdoc for the full design rationale.
pub struct TimingPaddingLayer {
    pub(super) config: TimingPaddingConfig,
    pub(super) policy: JitterPolicy,
    /// Per-layer server-side secret mixed into every request seed —
    /// generated once via SysRng at layer construction so the same
    /// `x-request-id` does NOT yield a deterministic seed across
    /// deploys / processes (codex round-1 P1 fix). When SysRng fails
    /// (pathological host), the fallback is `0` and the layer
    /// degrades to id-only seeding — this is documented + still
    /// correlation-resistant via the per-call counter.
    pub(super) server_secret: u64,
    /// Monotonic counter used for cross-call independence + as the
    /// absent-`x-request-id` fallback entropy source. Two consecutive
    /// requests with the same id (intentional or attacker-controlled)
    /// therefore still pick distinct seeds.
    pub(super) call_counter: Arc<AtomicU64>,
    /// Predicate selected for downstream services. Codex round-2 P2
    /// fix — earlier draft hard-coded `Any` at `Layer::layer`, leaving
    /// callers no way to select a stricter predicate at the layer
    /// boundary; selecting at the *service* boundary required a
    /// `with_predicate` chain that the canonical
    /// `tonic::Server::builder().layer(...)` and
    /// `axum::Router::layer(...)` pipelines cannot express.
    pub(super) predicate_kind: PredicateKind,
}

impl TimingPaddingLayer {
    /// New layer with the [`TimingPaddingConfig::canonical`] config,
    /// production [`JitterPolicy::Seeded`] policy, and the canonical
    /// [`PredicateKind::Any`] predicate (matches HTTP 404 ∪ gRPC
    /// `grpc-status: 5` ∪ [`super::policy::MissMarker`] extension).
    #[must_use]
    pub fn canonical() -> Self {
        Self::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded)
    }

    /// New layer with explicit config + jitter policy. Predicate
    /// defaults to [`PredicateKind::Any`]; override with
    /// [`Self::with_predicate`] for pure-HTTP / pure-gRPC stacks.
    #[must_use]
    pub fn new(config: TimingPaddingConfig, policy: JitterPolicy) -> Self {
        let server_secret = generate_server_secret();
        Self {
            config,
            policy,
            server_secret,
            call_counter: Arc::new(AtomicU64::new(0)),
            predicate_kind: PredicateKind::Any,
        }
    }

    /// Select the [`PredicateKind`] at the layer boundary so the
    /// canonical mounting calls (`tonic::Server::builder().layer(...)`,
    /// `axum::Router::layer(...)`) can be written directly without
    /// chaining `service.with_predicate(...)` after `Layer::layer`.
    /// Codex round-2 P2 fix.
    #[must_use]
    pub const fn with_predicate(mut self, predicate_kind: PredicateKind) -> Self {
        self.predicate_kind = predicate_kind;
        self
    }

    /// View the config (read-only — runtime mutation flips the
    /// statistical-indistinguishability proof window).
    #[must_use]
    pub const fn config(&self) -> &TimingPaddingConfig {
        &self.config
    }

    /// View the policy (read-only).
    #[must_use]
    pub const fn policy(&self) -> &JitterPolicy {
        &self.policy
    }

    /// View the predicate (read-only).
    #[must_use]
    pub const fn predicate_kind(&self) -> PredicateKind {
        self.predicate_kind
    }
}

impl Clone for TimingPaddingLayer {
    fn clone(&self) -> Self {
        Self {
            config: self.config,
            policy: self.policy.clone(),
            server_secret: self.server_secret,
            call_counter: Arc::clone(&self.call_counter),
            predicate_kind: self.predicate_kind,
        }
    }
}

impl core::fmt::Debug for TimingPaddingLayer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TimingPaddingLayer")
            .field("config", &self.config)
            .field("policy", &self.policy)
            .field("predicate_kind", &self.predicate_kind)
            // server_secret intentionally redacted — disclosing it
            // hands the attacker the deterministic per-request seed.
            .field("server_secret", &"<REDACTED>")
            .finish()
    }
}

impl Default for TimingPaddingLayer {
    fn default() -> Self {
        Self::canonical()
    }
}

impl<S> Layer<S> for TimingPaddingLayer {
    type Service = TimingPaddingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimingPaddingService::__new_from_layer(
            inner,
            self.config,
            self.policy.clone(),
            self.server_secret,
            Arc::clone(&self.call_counter),
            self.predicate_kind,
        )
    }
}
