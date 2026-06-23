//! Container-side resolution of a tenant's **resolved per-tier storage cap**
//! (bytes) for the OCI `/token` mint (WP #10 — OCI cap-on-downgrade residual).
//!
//! # Why this exists
//!
//! The native CAS/AC/Bazel/Turbo write path threads the tenant's resolved
//! per-tier storage cap into the byte-accounting reservation via the
//! Worker-set [`crate::byte_accounting::STORAGE_QUOTA_HEADER`] (the Worker is
//! the quota-resolution authority — it maps `tier → QUOTAS[tier].storageBytesMax`).
//! OCI is different: the Worker forwards `/v2/*` + `/token` **RAW** (it cannot
//! resolve the PAT scope for the two-leg flow, so it never sets that header),
//! and the data plane presents only the HMAC bearer — never the PAT. So an OCI
//! finalize-blob write reached [`crate::adapter_cache::MoatCache::put`] with the
//! cap hard-coded to `None`, and a **DOWNGRADED** tenant pushing exclusively
//! over OCI over-stored up to the stale cap until a native write reseeded the
//! `tenant_storage_state` row.
//!
//! This resolver closes that at the ONE seam where OCI knows the tenant — the
//! `/token` mint, after the full Option-B PAT re-verify. It ports the Worker's
//! tier→cap derivation minimally (the SAME `QUOTAS` numbers + the SAME
//! `tier_selections(active) → tenant.tier → free` lookup order) so the cap the
//! OCI bearer carries matches what the native plane would have resolved. The
//! resolved cap is embedded in the signed bearer and reserved against at
//! finalize (`MoatCache::put` → `CasWriteRequest::with_storage_quota_bytes`).
//!
//! # Fail-CLOSED
//!
//! A D1 error (cannot confirm the tier) resolves to `None` (indeterminate) — the
//! data plane then fails CLOSED on an unseeded tenant exactly as the native plane
//! does when the Worker omits the cap header on a D1-error tier. Absence is NEVER
//! treated as unlimited.

#![forbid(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;

use crate::storage::d1_http::D1HttpClient;

/// One GiB in bytes (`1024^3`) — the unit the per-tier caps are expressed in,
/// matching the Worker's `QUOTAS` table (`worker/src/lib/quota.ts`).
const GIB: i64 = 1_073_741_824;

/// The genuine-unlimited cap sentinel (`Some(0)`), matching the Worker's
/// `storageQuotaHeaderValue` returning `"0"` for `MAX_SAFE_INTEGER` tiers and
/// the byte-accounting `bytes_quota = 0` "uncapped" row semantics.
const UNLIMITED: i64 = 0;

/// Resolve a tier name to its storage cap (bytes), mirroring
/// `worker/src/lib/quota.ts` `QUOTAS[tier].storageBytesMax` →
/// `storageQuotaHeaderValue`:
///
/// - a finite tier → `Some(n)` (`n > 0`) — including `team`, whose Worker cap
///   is a FINITE 1 TiB (`worker/src/lib/quota.ts:88`), NOT unlimited;
/// - a genuinely-unlimited tier (`enterprise` only, whose Worker cap is
///   `MAX_SAFE_INTEGER`) → `Some(0)` (the deliberate unlimited sentinel);
/// - an UNKNOWN tier string → `free` (the Worker's `isValidTier` fallback +
///   default column `DEFAULT 'free'`), NOT unlimited — fail-safe.
///
/// Kept in lock-step with the Worker's table; the unit test pins every value.
#[must_use]
fn tier_to_cap_bytes(tier: &str) -> i64 {
    match tier {
        "free" => 10 * GIB,
        "solo" => 50 * GIB,
        "starter" => 150 * GIB,
        "pro" | "org" => 500 * GIB,
        "max" => 2_000 * GIB,
        // `team` is a FINITE 1 TiB (= 1024 GiB = 1_099_511_627_776) — the
        // Worker caps it at exactly that (`quota.ts:88`), NOT unlimited. (Its
        // `requestsPerMonthMax` is `MAX_SAFE_INTEGER`, which is what some Worker
        // comments loosely call "unlimited"; the STORAGE cap is finite.)
        "team" => 1024 * GIB,
        // enterprise is the ONLY genuinely-unlimited tier (Worker storage cap
        // `MAX_SAFE_INTEGER`).
        "enterprise" => UNLIMITED,
        // Unknown / unset tier → the `free` cap (Worker `isValidTier` fallback +
        // `tenant.tier DEFAULT 'free'`). Never unlimited.
        _ => 10 * GIB,
    }
}

/// Port: resolve a tenant's RESOLVED per-tier storage cap (bytes).
///
/// Returns:
/// - `Ok(Some(n))`, `n > 0` — a finite cap;
/// - `Ok(Some(0))` — a genuinely-unlimited tier;
/// - `Ok(None)` — INDETERMINATE (cap could not be confirmed → caller fails
///   CLOSED, mirroring the native plane on a D1-error tier).
///
/// Abstracted as a trait so the resolution logic is unit-testable without D1.
#[async_trait]
pub trait TenantCapResolver: std::fmt::Debug + Send + Sync {
    /// Resolve the effective storage cap for `tenant_id` (canonical UUID text).
    async fn resolve_storage_cap(&self, tenant_id: &str) -> Option<i64>;
}

/// Production [`TenantCapResolver`] over D1.
///
/// Mirrors `getTierForTenant`: `tier_selections` (ACTIVE subscription only) →
/// `tenant.tier` → hard-default `free`. A finite tier yields its byte cap; an
/// unlimited tier yields `Some(0)`; a D1 error on BOTH lookups yields `None`
/// (indeterminate → data-plane fail-closed), matching the Worker's
/// `d1Error ⇒ omit the cap header` posture.
#[derive(Debug)]
pub struct D1TenantCapResolver {
    client: Arc<D1HttpClient>,
}

impl D1TenantCapResolver {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }

    /// Read a tenant's tier via the canonical lookup order. `Ok(Some(tier))`
    /// when a confirmed row was found; `Ok(None)` when no row exists at either
    /// table (caller defaults to `free`); `Err` when BOTH queries errored
    /// (indeterminate — caller fails closed).
    async fn resolve_tier(&self, tenant_id: &str) -> Result<Option<String>, ()> {
        // 1. tier_selections — only an ACTIVE subscription grants its paid tier
        //    (a `pending_checkout` row must NOT yield paid quota; mirrors the
        //    Worker's `subscription_state = 'active'` filter).
        let mut sel_err = false;
        match self
            .client
            .query(
                "SELECT tier FROM tier_selections \
                 WHERE tenant_id = ?1 AND subscription_state = 'active' LIMIT 1",
                &[serde_json::Value::String(tenant_id.to_owned())],
            )
            .await
        {
            Ok(rows) => {
                if let Some(tier) = rows
                    .into_iter()
                    .next()
                    .and_then(|r| r.get("tier").and_then(|v| v.as_str().map(str::to_owned)))
                {
                    return Ok(Some(tier));
                }
            }
            Err(e) => {
                sel_err = true;
                tracing::warn!(error = %e, "oci-cap: tier_selections lookup failed; falling through");
            }
        }

        // 2. tenant.tier (DEFAULT 'free').
        let mut tenant_err = false;
        match self
            .client
            .query(
                "SELECT tier FROM tenant WHERE tenant_id = ?1 LIMIT 1",
                &[serde_json::Value::String(tenant_id.to_owned())],
            )
            .await
        {
            Ok(rows) => {
                if let Some(tier) = rows
                    .into_iter()
                    .next()
                    .and_then(|r| r.get("tier").and_then(|v| v.as_str().map(str::to_owned)))
                {
                    return Ok(Some(tier));
                }
            }
            Err(e) => {
                tenant_err = true;
                tracing::warn!(error = %e, "oci-cap: tenant.tier lookup failed");
            }
        }

        // BOTH queries errored ⇒ indeterminate (fail-closed). Otherwise no row
        // was found ⇒ the caller defaults to `free` (a real, confirmed default).
        if sel_err && tenant_err {
            Err(())
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl TenantCapResolver for D1TenantCapResolver {
    async fn resolve_storage_cap(&self, tenant_id: &str) -> Option<i64> {
        match self.resolve_tier(tenant_id).await {
            // Confirmed tier → its cap.
            Ok(Some(tier)) => Some(tier_to_cap_bytes(&tier)),
            // No row at either table → confirmed `free` default.
            Ok(None) => Some(tier_to_cap_bytes("free")),
            // Both lookups errored → indeterminate; fail CLOSED.
            Err(()) => None,
        }
    }
}

// F-017: the rate-limit layer's per-tenant tier resolver reuses this EXACT D1
// lookup (`tier_selections` ACTIVE → `tenant.tier` → `free`) — one source of
// truth for "what tier is this tenant", shared with the OCI storage-cap path.
// The layer maps the label to the RPS ladder (`refill_rate_for_tier`); a `None`
// (D1 error / unclassifiable) leaves the bucket on the team-default config —
// fail-SAFE on availability (never over-throttle a tenant we cannot classify),
// matching `resolve_storage_cap`'s own indeterminate→fail posture.
#[async_trait]
impl crate::routes::ratelimit_layer::TenantTierResolver for D1TenantCapResolver {
    async fn resolve_tier_label(&self, tenant_id: &str) -> Option<String> {
        self.resolve_tier(tenant_id).await.ok().flatten()
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
    use super::*;

    #[test]
    fn tier_caps_match_the_worker_table() {
        // Pinned against `worker/src/lib/quota.ts` QUOTAS.storageBytesMax. A
        // drift here means the OCI plane caps differently than the native plane.
        assert_eq!(tier_to_cap_bytes("free"), 10 * GIB);
        assert_eq!(tier_to_cap_bytes("solo"), 50 * GIB);
        assert_eq!(tier_to_cap_bytes("starter"), 150 * GIB);
        assert_eq!(tier_to_cap_bytes("pro"), 500 * GIB);
        assert_eq!(tier_to_cap_bytes("org"), 500 * GIB);
        assert_eq!(tier_to_cap_bytes("max"), 2_000 * GIB);
        // `team` is a FINITE 1 TiB (1024 GiB) — the Worker caps it at exactly
        // `1_099_511_627_776` (`quota.ts:88`). It is NOT unlimited; mapping it
        // to the `UNLIMITED` sentinel let `team` over-store unbounded on the
        // OCI plane (F-002). Pin both the GiB form and the literal byte value.
        assert_eq!(tier_to_cap_bytes("team"), 1024 * GIB);
        assert_eq!(tier_to_cap_bytes("team"), 1_099_511_627_776);
        // enterprise is the ONLY genuinely-unlimited tier → the `Some(0)`
        // sentinel.
        assert_eq!(tier_to_cap_bytes("enterprise"), UNLIMITED);
        // Unknown / unset tier → the `free` cap (never unlimited).
        assert_eq!(tier_to_cap_bytes("bogus"), 10 * GIB);
        assert_eq!(tier_to_cap_bytes(""), 10 * GIB);
    }
}
