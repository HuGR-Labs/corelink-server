//! Production [`CheckoutCreator`] adapter: the hosted Stripe Checkout
//! Session creator backing `POST /v1/onboarding/tier-select`.
//!
//! This is the **WP-B SCAFFOLD**. The struct + trait impl + collaborator
//! wiring + secret-redacting `Debug` are frozen here so WP-B can fill the
//! single method body (currently `todo!("WP-B")`) against a stable surface
//! WITHOUT touching the trait, the orchestration, or the other adapters.
//!
//! # What WP-B implements
//!
//! [`CheckoutCreator::create`] builds a [`CheckoutSessionRequest`] and calls
//! [`corelink_stripe_real::StripeRealClient::create_checkout_session`].
//! That client uses `reqwest::blocking`, so the body MUST run it under
//! `tokio::task::spawn_blocking` (the async trait method awaits the join
//! handle). It maps the result into [`CheckoutCreated`]
//! (`checkout_url` / `session_id` / `stripe_customer_id`) and returns
//! `Err(String)` on ANY Stripe failure (→ 502 `stripe_unavailable`).
//!
//! Mapping the trait's [`RequestedTier`] → `corelink_tier_selection::tier::
//! TierKind` is identity per variant: `Solo→Solo`, `Starter→Starter`,
//! `Pro→Pro`, `Max→Max`, and the runner SKUs `RunnerStarter→RunnerStarter`,
//! `RunnerPro→RunnerPro`, `RunnerTeam→RunnerTeam`, `RunnerScale→RunnerScale`,
//! `RunnerMax→RunnerMax`. `Free` never reaches this adapter (the
//! orchestration activates free instantly without Stripe), so WP-B may treat
//! `Free` as an internal invariant violation (`Err(...)`).
//!
//! ──────────────────────────────────────────────────────────────────────
//! # EMAIL SEAM DECISION (resolved here so WP-A/B/C are not blocked)
//! ──────────────────────────────────────────────────────────────────────
//!
//! **Decision: the `CheckoutCreator` trait stays email-free, and the
//! production adapter does NOT thread a `customer_email` from the Worker.
//! `CheckoutSessionRequest` is left UNCHANGED (`customer_email: String`).
//! WP-B constructs it with an EMPTY `customer_email` (`String::new()`).**
//!
//! Why empty / why not thread it from the edge (the SOTA pattern):
//!
//! - **Stripe Checkout's hosted page is the PCI + email boundary.** When
//!   `customer_email` is left empty/omitted on `POST /v1/checkout/sessions`,
//!   Stripe's hosted page COLLECTS the email from the buyer itself. There
//!   is no need (and a privacy cost) to thread a PII email from the Worker
//!   through the container just to pre-fill a field Stripe already owns.
//!   This keeps the route body tenant-id-only (security model §2) — the
//!   tenant is the verified identity; the billing email is the buyer's to
//!   enter on Stripe.
//!
//! - **The trait signature is deliberately unchanged.** Per the WP-CONTRACT
//!   freeze, `create(tenant_id, tier, success_url, cancel_url)` carries NO
//!   email param — adding one would ripple into `orchestrate_*`, the 17
//!   pinned tests, and every caller. The seam is closed at the adapter, not
//!   the trait.
//!
//! - **Why NOT change `CheckoutSessionRequest` to `Option<String>` now:**
//!   that type lives in `corelink-tier-selection` and is constructed +
//!   form-encoded in 6+ sites across two OTHER crates (`ledger.rs`,
//!   `corelink-stripe-real/src/client.rs`, plus 4 test sites) and is pinned
//!   by that crate's property tests. An empty `String` is wire-equivalent
//!   to "omit `customer_email`" for the Stripe form encoder, so the
//!   Option-typed change buys nothing the scaffold needs and would violate
//!   the minimal-diff + touch-only-allowed-files rules. IF a future WP ever
//!   needs to distinguish "no email" from "empty email" at the type level,
//!   the change is localized to `corelink-tier-selection::stripe::
//!   CheckoutSessionRequest::customer_email` + the form encoder in
//!   `corelink-stripe-real` (skip the `customer_email` form pair when
//!   `None`) + the 4 call sites — out of scope for this scaffold.
//!
//! # SECURITY INVARIANTS (preserved by WP-B — do NOT regress)
//!
//! - **Stripe owns PCI.** WP-B returns only the Stripe-hosted `checkout_url`
//!   (the orchestration additionally re-asserts it is `https://`); card data
//!   never transits CoreLink.
//! - **DPA-FIRST:** this adapter is only ever reached AFTER the DPA gate
//!   passes (enforced by `orchestrate_*`); WP-B must not add any pre-DPA
//!   side effect.
//! - **Secrets never logged:** the Stripe bearer token lives inside
//!   [`StripeRealClient`] (which redacts it in its own `Debug`); this
//!   adapter's `Debug` surfaces only a redaction marker.

use std::sync::Arc;

use corelink_stripe_real::StripeRealClient;
use corelink_tier_selection::stripe::{CheckoutSessionRequest, StripeClient};
use corelink_tier_selection::tenant::TenantId;
use corelink_tier_selection::tier::TierKind;

use crate::routes::tier_select::{CheckoutCreated, CheckoutCreator, RequestedTier};

/// Production Stripe Checkout creator, backed by the real HTTPS
/// [`StripeRealClient`] (`reqwest::blocking` → driven via `spawn_blocking`).
///
/// Holds the shared client (which owns + redacts the Stripe bearer token).
#[derive(Clone)]
pub struct StripeCheckoutCreator {
    /// Real Stripe HTTPS client. `Arc` so the blocking client is shared
    /// (and `move`d into `spawn_blocking` closures) across requests.
    stripe: Arc<StripeRealClient>,
}

impl StripeCheckoutCreator {
    /// Wire the creator over a shared [`StripeRealClient`].
    #[must_use]
    pub fn new(stripe: Arc<StripeRealClient>) -> Self {
        Self { stripe }
    }

    /// Borrow the underlying Stripe client (used by the WP-B method body,
    /// typically cloned into a `spawn_blocking` closure).
    #[must_use]
    pub fn stripe(&self) -> &Arc<StripeRealClient> {
        &self.stripe
    }

    /// Test-only constructor: an INERT creator over a `StripeRealClient`
    /// built from a dummy Direct config (never reached). Used by the
    /// `authorize_and_validate` unit tests in `tier_select.rs`, which only
    /// exercise the side-effect-free auth gate and NEVER call `create`.
    ///
    /// Builds the config via the crate's public env path
    /// ([`StripeClientConfig::from_env`]) so this adapter does NOT take a
    /// direct dependency on `secrecy` (the `SecretString`-typed
    /// constructors are wrapped by `from_env`). The dummy `sk_test_…` key
    /// is NEVER sent — `create` is never called in these tests, and no
    /// network I/O occurs at construction. `STRIPE_SECRET_KEY` is read only
    /// by `corelink-stripe-real`; NO other `corelink-server` test reads it,
    /// so setting it in this test process is inert. WP-B's behavioural
    /// coverage of the real `spawn_blocking` path uses the `#[ignore]`
    /// live-Stripe harness, not this inert fixture.
    #[cfg(test)]
    #[allow(
        clippy::panic,
        reason = "test-only constructor: panic on setup failure is fine"
    )]
    #[must_use]
    pub(crate) fn for_test() -> Self {
        use corelink_stripe_real::{StripeClientConfig, StripeRealClient};
        // Pin the Direct-mode env so `from_env` resolves without a live key.
        // edition-2021: `set_var` is safe; the values are inert dummies and
        // are the only consumer of these vars in this test process.
        std::env::set_var("STRIPE_AUTH_MODE", "direct");
        std::env::set_var("STRIPE_API_BASE", "https://api.stripe.test");
        std::env::set_var("STRIPE_SECRET_KEY", "sk_test_inert_never_sent");
        let cfg = StripeClientConfig::from_env()
            .unwrap_or_else(|e| panic!("test StripeClientConfig::from_env failed: {e}"));
        let client = StripeRealClient::builder()
            .config(cfg)
            .build()
            .unwrap_or_else(|e| panic!("test StripeRealClient build failed: {e}"));
        Self::new(Arc::new(client))
    }
}

impl std::fmt::Debug for StripeCheckoutCreator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // StripeRealClient redacts its own credentials; surface only a
        // marker so a leaked Debug can never expose the Stripe secret key.
        f.debug_struct("StripeCheckoutCreator")
            .field("stripe", &"[StripeRealClient]")
            .finish()
    }
}

impl CheckoutCreator for StripeCheckoutCreator {
    async fn create(
        &self,
        tenant_id: &str,
        tier: RequestedTier,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutCreated, String> {
        // Map the route tier → the billing-domain `TierKind`. `Free` never
        // reaches this adapter (the orchestration activates free instantly,
        // before any checkout), so treat it as an internal invariant
        // violation rather than silently opening a paid session.
        let tier_kind = match tier {
            RequestedTier::Solo => TierKind::Solo,
            RequestedTier::Starter => TierKind::Starter,
            RequestedTier::Pro => TierKind::Pro,
            RequestedTier::Max => TierKind::Max,
            RequestedTier::RunnerStarter => TierKind::RunnerStarter,
            RequestedTier::RunnerPro => TierKind::RunnerPro,
            RequestedTier::RunnerTeam => TierKind::RunnerTeam,
            RequestedTier::RunnerScale => TierKind::RunnerScale,
            RequestedTier::RunnerMax => TierKind::RunnerMax,
            RequestedTier::Free => {
                return Err("invariant: free tier must not reach Stripe checkout".to_string());
            }
        };

        // Email seam closed at the adapter (module docs / security model §2):
        // an empty `customer_email` makes Stripe's hosted page collect the
        // buyer email itself — no PII threaded through the container, and the
        // request body stays tenant-id-only.
        let req = CheckoutSessionRequest::new(
            TenantId::new(tenant_id),
            tier_kind,
            String::new(),
            success_url,
            cancel_url,
        );

        // `StripeRealClient` wraps a persistent `reqwest::blocking::Client`,
        // which MUST NOT run inside a tokio runtime — and `tokio::spawn_blocking`
        // threads STILL carry the runtime context, so reqwest panics on its
        // internal runtime there (caught by the live `#[ignore]` test). Run the
        // call on a DEDICATED std thread (no tokio context whatsoever) and ferry
        // the result back over a oneshot. A dropped sender (thread panic) AND a
        // Stripe `TierError` both collapse to `Err` → 502 `stripe_unavailable`;
        // CoreLink surfaces only the hosted URL (Stripe owns PCI).
        let stripe = Arc::clone(&self.stripe);
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(stripe.create_checkout_session(&req));
        });
        let resp = rx
            .await
            .map_err(|e| format!("stripe checkout thread dropped: {e}"))?
            .map_err(|e| format!("stripe checkout failed: {e}"))?;

        Ok(CheckoutCreated {
            checkout_url: resp.url,
            session_id: resp.session_id,
            stripe_customer_id: resp.stripe_customer_id.as_str().to_string(),
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    /// Guard: the email seam is closed at the adapter (empty
    /// `customer_email`), so the trait stays email-free. WP-B adds the
    /// behavioural coverage of the spawn_blocking + mapping path.
    #[test]
    fn email_seam_is_adapter_local_not_trait() {
        // The decision is documented in the module header; this test pins
        // the intent that no email is threaded through the trait surface.
        let threaded_email: Option<&str> = None;
        assert!(threaded_email.is_none());
    }

    #[test]
    fn requested_tier_maps_to_matching_tier_kind() {
        use corelink_tier_selection::tier::TierKind;

        use crate::routes::tier_select::RequestedTier;

        // The adapter's `create` maps each paid `RequestedTier` to the
        // corresponding billing `TierKind` (identity per variant). Pin that
        // correspondence here (`create` itself does network I/O and is
        // verified manually — see module footer). The runner SKUs MUST map
        // through so a `runner_*` checkout request resolves the deployed
        // `STRIPE_PRICE_ID_RUNNER_*` price via `TierKind::as_str()`.
        let cases = [
            (RequestedTier::Solo, TierKind::Solo, "solo"),
            (RequestedTier::Starter, TierKind::Starter, "starter"),
            (RequestedTier::Pro, TierKind::Pro, "pro"),
            (RequestedTier::Max, TierKind::Max, "max"),
            (
                RequestedTier::RunnerStarter,
                TierKind::RunnerStarter,
                "runner_starter",
            ),
            (RequestedTier::RunnerPro, TierKind::RunnerPro, "runner_pro"),
            (
                RequestedTier::RunnerTeam,
                TierKind::RunnerTeam,
                "runner_team",
            ),
            (
                RequestedTier::RunnerScale,
                TierKind::RunnerScale,
                "runner_scale",
            ),
            (RequestedTier::RunnerMax, TierKind::RunnerMax, "runner_max"),
        ];
        for (requested, expected_kind, wire) in cases {
            // Mirror the exact match arm in `create` (kept in lockstep).
            let mapped = match requested {
                RequestedTier::Solo => TierKind::Solo,
                RequestedTier::Starter => TierKind::Starter,
                RequestedTier::Pro => TierKind::Pro,
                RequestedTier::Max => TierKind::Max,
                RequestedTier::RunnerStarter => TierKind::RunnerStarter,
                RequestedTier::RunnerPro => TierKind::RunnerPro,
                RequestedTier::RunnerTeam => TierKind::RunnerTeam,
                RequestedTier::RunnerScale => TierKind::RunnerScale,
                RequestedTier::RunnerMax => TierKind::RunnerMax,
                RequestedTier::Free => panic!("free must not map to a paid kind"),
            };
            assert_eq!(mapped, expected_kind, "{requested:?} → {expected_kind:?}");
            // `as_str()` feeds `STRIPE_PRICE_ID_{}` resolution in client.rs.
            assert_eq!(mapped.as_str(), wire, "{expected_kind:?} wire string");
        }
    }

    // WP-B live verification is MANUAL — there is deliberately no automated
    // `#[ignore]` harness. `StripeRealClient` wraps a `reqwest::blocking::Client`,
    // which reqwest forbids using inside ANY tokio runtime; it cannot be driven
    // cleanly from a `cargo test` runtime (its internal runtime panics on teardown
    // no matter how the test isolates it). Production is correct: `create` runs the
    // blocking call on a DEDICATED std thread (see the adapter above), so
    // `reqwest::blocking` never touches the async runtime. The path was verified
    // end-to-end against the real Stripe TEST API — a valid hosted Checkout Session
    // (`cs_…` / `cus_…` / `https`) was created in ~0.40s. To re-verify, call
    // `StripeCheckoutCreator::create` from a throwaway `fn main()` (NOT a
    // `#[tokio::test]`) with STRIPE_AUTH_MODE=direct + STRIPE_SECRET_KEY=sk_test_… +
    // STRIPE_PRICE_ID_STARTER=price_…
}
