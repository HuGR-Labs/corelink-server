use corelink_billing_stripe_materializer::{
    InMemoryRunnersEntitlementResolver, InMemoryTierSelector, RunnersEntitlement,
};
use corelink_tier_selection::tier::TierKind;

/// Decide whether a missing native PAT gate is a FATAL boot condition
/// (red-team finding #7, HIGH).
///
/// The native data planes (CAS / AC / Bazel / Turbo) mount with the
/// container-side Argon2id possession backstop ONLY when
/// `adapter_pat::PatVerifier::from_env()` (and thus
/// `native_pat_gate_from_env()`) builds. That builder returns `None` not only
/// in dev/CI (benign) but ALSO in prod when a `PAT_SIGNING_KEY` rotation
/// sibling (`PAT_SIGNING_KEY_PREV` / `_NEW`) is PRESENT-but-malformed — a
/// one-character typo at deploy. In that case the planes would silently mount
/// WITHOUT the backstop fleet-wide: a silent security downgrade.
///
/// This pure predicate isolates the policy so it is unit-testable (the caller
/// does the `std::process::exit(1)`): in prod (`prod == true`) a missing gate
/// (`gate_present == false`) is fatal; otherwise (dev/CI, or the gate is
/// present) it is not.
#[must_use]
pub(super) const fn should_fatal_on_missing_gate(prod: bool, gate_present: bool) -> bool {
    prod && !gate_present
}

/// Whether the enterprise BYOK revocation scheduler may start.
///
/// A production image is compiled with a real-provider feature so BYOK
/// operations can fail closed, but that feature alone must not make customer
/// KMS credentials a readiness dependency of the cache data plane. Only an
/// explicit operator opt-in enables the background scheduler.
#[must_use]
#[cfg(any(
    test,
    feature = "byok-aws-real",
    feature = "byok-gcp-real",
    feature = "byok-azure-real",
    feature = "byok-vault-real"
))]
pub(super) fn byok_revocation_scheduler_enabled(value: Option<&str>) -> bool {
    value.is_some_and(|raw| matches!(raw.trim().to_ascii_lowercase().as_str(), "1" | "true"))
}

/// Decide whether a missing/empty `EMAIL_HASH_SALT` is a FATAL boot condition
/// (CAA-360 MEDIUM — email-hash salt fail-fast).
///
/// The CTRL-PRIV-001 `email_hash::hash_email` helper HMAC-SHA256s the normalized
/// email under `EMAIL_HASH_SALT` when it is set+non-empty, but silently falls
/// back to a rainbow-table-reversible plain `SHA-256(email)` when it is
/// unset/empty. The salt is now SET on every prod target, so a future deploy
/// that DROPPED it must NOT be allowed to silently regress to the unsalted
/// scheme. This pure predicate isolates the policy so it is unit-testable (the
/// caller does the `std::process::exit(1)`): in prod (`prod == true`) a missing
/// salt (`salt_present == false`) is fatal; outside prod (dev/CI) or when the
/// salt is present it is not — so non-prod behavior is unchanged.
#[must_use]
pub(super) const fn email_hash_salt_missing_in_prod(prod: bool, salt_present: bool) -> bool {
    prod && !salt_present
}

/// The REVENUE path's must-arm control. Each of the four cache-tier
/// `STRIPE_PRICE_ID_*` env vars ([`TIER_PRICE_ENV_TABLE`]) must resolve to a real
/// Stripe price id in prod. When one is unset/empty, [`build_tier_selector_from`]
/// silently registers the literal `plan_{tier}` placeholder (F-001 back-compat)
/// — which a real Stripe `price_live_…` event can NEVER match, so the container
/// webhook materializer resolves `UnknownPlan` → 422, Stripe stops retrying, and
/// a PAYING customer is stranded on the free serving path with NO alarm. Solo
/// ($30/mo) is the primary self-serve SMB tier, so this is launch-critical.
/// Returns the env-var names still unset/empty in prod (empty ⇒ armed). Pure and
/// injectable so the policy is unit-tested without the process environment
/// (mirrors [`email_hash_salt_missing_in_prod`]). Keep the container's map
/// identical to the signup-worker's reverse map (`apps/signup-worker/src/webhooks/
/// stripe.ts`) — divergence re-opens the same `UnknownPlan` → 422 seam.
pub(super) fn cache_tier_price_ids_missing_in_prod<F>(prod: bool, lookup: F) -> Vec<&'static str>
where
    F: Fn(&str) -> Option<String>,
{
    if !prod {
        return Vec::new();
    }
    TIER_PRICE_ENV_TABLE
        .iter()
        .filter(|(env_name, _fallback, _tier)| {
            lookup(env_name).is_none_or(|v| v.trim().is_empty())
        })
        .map(|(env_name, _, _)| *env_name)
        .collect()
}

/// Canonical `(env-var-name, literal-fallback-key, tier)` table for the
/// container Stripe-webhook tier reconciliation (F-001).
///
/// Each tuple says: the live Stripe price id is read from the env var
/// `0`; if that env var is unset/empty we fall back to the literal
/// placeholder key `1` (back-compat with pre-price-id deployments and
/// the historical test fixtures). Both keys map to tier `2`.
///
/// Only the four Stripe-checkout tiers (`Solo/Starter/Pro/Max`) have a
/// [`TierKind`] variant; `team` is a legacy operator-assigned SKU and
/// `enterprise` is a contact-sales route (neither flows through this
/// container path), so they are intentionally absent.
pub(super) const TIER_PRICE_ENV_TABLE: &[(&str, &str, TierKind)] = &[
    ("STRIPE_PRICE_ID_SOLO", "plan_solo", TierKind::Solo),
    ("STRIPE_PRICE_ID_STARTER", "plan_starter", TierKind::Starter),
    ("STRIPE_PRICE_ID_PRO", "plan_pro", TierKind::Pro),
    ("STRIPE_PRICE_ID_MAX", "plan_max", TierKind::Max),
];

/// Runners-tier price → `(max_concurrency, max_vcpu_h)` mapping (a SEPARATE
/// axis from the cache tiers above). Owner-ratified loss-proof ladder
/// (`corelink-runners docs/product/pricing.md §2`, 2026-06-16) on the real
/// ~$0.10/vCPU-h Cloudflare-Containers basis: Starter 20/100 · Pro 40/240 ·
/// Team 80/600 · Scale 160/1200 · Max 320/2400. Keyed on the live
/// `STRIPE_PRICE_ID_RUNNER_*` price id; unset env ⇒ that tier is dormant (no
/// literal fallback — a Runners price must be a real Stripe id, never guessed).
pub(super) const RUNNER_PRICE_ENV_TABLE: &[(&str, u32, u32)] = &[
    ("STRIPE_PRICE_ID_RUNNER_STARTER", 20, 100),
    ("STRIPE_PRICE_ID_RUNNER_PRO", 40, 240),
    ("STRIPE_PRICE_ID_RUNNER_TEAM", 80, 600),
    ("STRIPE_PRICE_ID_RUNNER_SCALE", 160, 1200),
    ("STRIPE_PRICE_ID_RUNNER_MAX", 320, 2400),
];

/// Build the container tier mapping from the live `STRIPE_PRICE_ID_*`
/// env values, falling back to the literal `plan_{tier}` keys when an
/// env var is unset/empty (F-001 fix).
pub(super) fn build_tier_selector() -> InMemoryTierSelector {
    build_tier_selector_from(|name| std::env::var(name).ok())
}

/// Build the Runners entitlement resolver from the live `STRIPE_PRICE_ID_RUNNER_*`
/// env, or `None` when NONE are set (the Runners seed path stays dormant →
/// every subscription is a cache-tier event, exactly as before the price IDs
/// exist). Mirrors [`build_tier_selector`] but with NO literal fallback (a
/// Runners price must be a real Stripe id) and returns `Option` so the caller
/// only wires the resolver when at least one Runners price is configured.
pub(super) fn build_runners_resolver() -> Option<InMemoryRunnersEntitlementResolver> {
    build_runners_resolver_from(|name| std::env::var(name).ok())
}

pub(super) fn build_runners_resolver_from<F>(
    lookup: F,
) -> Option<InMemoryRunnersEntitlementResolver>
where
    F: Fn(&str) -> Option<String>,
{
    let mut resolver = InMemoryRunnersEntitlementResolver::new();
    for (env_name, max_concurrency, max_vcpu_h) in RUNNER_PRICE_ENV_TABLE {
        if let Some(price_id) = lookup(env_name) {
            if !price_id.trim().is_empty() {
                resolver = resolver.with_price(
                    price_id.trim(),
                    RunnersEntitlement {
                        max_concurrency: *max_concurrency,
                        max_vcpu_h: *max_vcpu_h,
                    },
                );
            }
        }
    }
    if resolver.is_empty() {
        None
    } else {
        Some(resolver)
    }
}

/// Pure mapping builder: `lookup` resolves an env-var name to its value
/// (injected so the policy is unit-testable without touching the
/// process environment).
///
/// For each row: if `lookup(env_name)` yields a non-empty value, map
/// that real price id → tier; otherwise map the literal `plan_{tier}`
/// placeholder → tier so test fixtures and pre-price-id deployments
/// still classify. When the env var IS set, the real price id is the
/// authoritative key and the literal placeholder is NOT registered
/// (the real Stripe event never carries it).
pub(super) fn build_tier_selector_from<F>(lookup: F) -> InMemoryTierSelector
where
    F: Fn(&str) -> Option<String>,
{
    let selector = InMemoryTierSelector::new();
    for (env_name, literal_fallback, tier) in TIER_PRICE_ENV_TABLE {
        match lookup(env_name) {
            Some(price_id) if !price_id.trim().is_empty() => {
                selector.register(price_id.trim(), *tier);
            }
            _ => {
                selector.register(literal_fallback, *tier);
            }
        }
    }
    selector
}
