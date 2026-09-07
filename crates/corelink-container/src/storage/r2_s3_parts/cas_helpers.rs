/// Elapsed microseconds since `started`, saturating.
///
/// `Instant::elapsed` yields a `Duration`; `as_micros` is a `u128` that
/// cannot fit `u64` only after ~584 000 years, so the saturating cast is
/// a formality that keeps the call sites free of a `#[allow]`.
fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Compute the per-tenant 16-char R2 key prefix (layer 5 of
/// `INV-TENANT-ISOLATION`).
///
/// The production handlers are ALWAYS constructed with a secret TDK
/// (`build_r2_*_handler_from_env` fail closed otherwise — F1/F2), so
/// the live path always takes the `Some(tdk)` arm and HMACs the FULL
/// tenant id under the secret key: an unpredictable, ~96-bit,
/// collision-resistant namespace.
///
/// The public, predictable raw-padded fallback (a 16-char prefix of
/// the tenant string) exists ONLY for unit-test fixtures whose tenant
/// is a simple non-UUID string (e.g. `"t1"`); it is gated behind
/// `#[cfg(test)]` and is unreachable in production.
///
/// # Errors
///
/// Returns `Err(String)` on the production path when the prefix is NOT
/// derivable — the tenant id is not a canonical UUID, or the handler
/// was somehow constructed without a TDK. The callers
/// ([`R2CasHandler::r2_key`] / `r2_list_prefix` and their AC twins)
/// propagate this as a 500/`Internal` so the op NEVER touches R2 under
/// a degraded/empty prefix that would collapse every non-derivable
/// tenant into one SHARED keyspace (cross-tenant read/overwrite/
/// delete/list). Fail CLOSED, never silent co-residence.
/// Fixed, reserved sentinel UUID for the public shared-dedup namespace
/// ([`crate::adapter_cache::PUBLIC_NAMESPACE`] = `_public`). `_public` is NOT a
/// tenant UUID — it's the intentional cross-tenant namespace for public,
/// deterministic content (Homebrew bottles, public npm/PyPI; the network-effect
/// moat). It has NO per-tenant isolation requirement (the content is public),
/// but it MUST get a STABLE prefix so every caller storing the same public blob
/// dedups to the same R2 key. We derive it from this fixed sentinel under the
/// SAME secret TDK: TDK-keyed (not a predictable raw prefix), reserved so it can
/// never collide with a real (random v4/v7) tenant's HMAC prefix, and identical
/// across callers. Hex spells `__public` in the leading bytes. Without this,
/// `_public` storage writes fail CLOSED (non-derivable) and the public bottle /
/// package cache is non-functional (the brew 502 root cause, 2026-06-21).
const PUBLIC_NAMESPACE_UUID: Uuid = Uuid::from_u128(0x5f5f_7075_626c_6963_0000_0000_0000_0001);

/// The stable, TDK-keyed R2 key prefix for the `_public` shared-dedup namespace
/// (see [`PUBLIC_NAMESPACE_UUID`]).
///
/// SINGLE SOURCE OF TRUTH for the public sentinel derivation. The `_public` CAS
/// **write** path ([`tenant_prefix`] below) and the public-revocation **eraser**
/// (`routes::public_revoke`, F3.2 B1b) MUST address the identical prefix — a
/// drift between the two would leave a revoked public blob's bytes physically
/// un-erasable (the exact BLOCKER-1 the revocation path exists to close). Both
/// go through this one function so the write key and the erase key are equal by
/// construction.
pub(crate) fn public_namespace_prefix(tdk: &TenantDerivationKey) -> String {
    derive_prefix(tdk, PUBLIC_NAMESPACE_UUID).to_string()
}

fn tenant_prefix(tdk: Option<&TenantDerivationKey>, tenant: &str) -> Result<String, String> {
    match tdk {
        // Public shared-dedup namespace: stable, TDK-keyed, reserved sentinel
        // prefix (see PUBLIC_NAMESPACE_UUID). Scoped to the EXACT `_public`
        // string — real UUID tenants are unaffected, other non-UUID tenants
        // still fail CLOSED below.
        Some(tdk) if tenant == crate::adapter_cache::PUBLIC_NAMESPACE => {
            Ok(public_namespace_prefix(tdk))
        }
        Some(tdk) => match Uuid::try_parse(tenant) {
            Ok(uid) => Ok(derive_prefix(tdk, uid).to_string()),
            // Non-UUID tenant: in test builds the simple raw-padded
            // fixture prefix is allowed; on the production path it is a
            // SEV-class invariant violation that must fail CLOSED (see
            // `derive_tenant_prefix_strict`).
            #[cfg(test)]
            Err(_) => Ok(raw_padded_prefix(tenant)),
            #[cfg(not(test))]
            Err(_) => derive_tenant_prefix_strict(Some(tdk), tenant),
        },
        // No TDK is only reachable under `#[cfg(test)]`: the production
        // builders fail closed when `R2_TDK_HEX` is unset, so the real
        // handler is never constructed with `tdk = None` (F1/F2).
        #[cfg(test)]
        None => Ok(raw_padded_prefix(tenant)),
        #[cfg(not(test))]
        None => derive_tenant_prefix_strict(None, tenant),
    }
}

/// AC action digests are part of the R2 keyspace, so canonicalization belongs
/// at the storage boundary as well as at HTTP routing. This prevents direct
/// handler callers (and future adapters) from creating duplicate uppercase
/// key slots for one digest.
#[inline]
fn is_canonical_ac_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Production-strict tenant-prefix derivation — the single fail-CLOSED
/// authority for the live storage path.
///
/// The ONLY non-degraded outcome is a secret-keyed `derive_prefix(tdk,
/// uuid)` over a canonical UUID tenant under a present TDK. Every other
/// input is non-derivable and returns `Err`: there is NO empty/public/
/// predictable fallback. An empty prefix would key objects under
/// `<region>//<digest>`, collapsing every non-derivable tenant into one
/// SHARED keyspace (cross-tenant read/overwrite/delete/list) — so the
/// op MUST fail before it ever reaches R2 (INV-TENANT-ISOLATION).
///
/// This function is NOT `#[cfg(test)]`-gated (unlike the
/// `raw_padded_prefix` fixture path) so the production fail-closed
/// contract is directly covered by the regression suite.
fn derive_tenant_prefix_strict(
    tdk: Option<&TenantDerivationKey>,
    tenant: &str,
) -> Result<String, String> {
    let Some(tdk) = tdk else {
        tracing::error!(
            "R2 storage handler constructed without a TDK on the production path; \
             refusing to derive a tenant prefix (INV-TENANT-ISOLATION)"
        );
        return Err("missing TDK on production storage path (INV-TENANT-ISOLATION)".to_owned());
    };
    match Uuid::try_parse(tenant) {
        Ok(uid) => Ok(derive_prefix(tdk, uid).to_string()),
        Err(_) => {
            tracing::error!(
                "tenant id is not a canonical UUID on the production storage path; \
                 refusing to derive a tenant prefix (INV-TENANT-ISOLATION)"
            );
            Err("non-derivable tenant prefix (INV-TENANT-ISOLATION)".to_owned())
        }
    }
}

/// Public raw-padded 16-char prefix — TEST FIXTURES ONLY.
///
/// Truncates/pads the raw tenant string to exactly 16 chars. This is a
/// PUBLIC, predictable namespace and must NEVER be used on the
/// production path (see F1/F2); it lets unit tests use simple non-UUID
/// tenant ids (`"t1"`, `"tenant-abc"`) without a real TDK.
#[cfg(test)]
fn raw_padded_prefix(tenant: &str) -> String {
    let mut p = tenant.to_owned();
    p.truncate(16);
    while p.len() < 16 {
        p.push('0');
    }
    p
}

/// Enforce the CAS content-addressing invariant
/// (`INV-CAS-INTEGRITY`): the supplied `bytes` MUST hash to
/// `claimed_hash` under the **keyspace's canonical digest function**,
/// selected explicitly by `algo`:
///
/// - [`DigestAlgo::Blake3`] — native CAS + sccache (the BLAKE3 keyspace).
/// - [`DigestAlgo::Sha256`] — the Bazel REAPI v2 `bazel/sha256/` keyspace.
///
/// The function is threaded as an EXPLICIT [`DigestAlgo`] (never inferred
/// from hash-string length — that would be a silent gate). The durable gate
/// is therefore surface-PARTITIONED, not literally BLAKE3-only-everywhere:
/// each keyspace is single-function and the two never mix within one key, so
/// the read-path re-verification (bitrot) always re-applies the SAME function
/// the blob was admitted under (Option A, ADR-0044). A native/sccache caller
/// always passes `Blake3` (behaviour unchanged); only the Bazel adapter
/// passes `Sha256`.
///
/// Returns `Ok(())` on a match, or `Err(actual_hex)` carrying the hash
/// actually computed from the bytes (under `algo`) so the caller can build
/// the `HashMismatch` error and the `CorrectnessViolation` audit event.
///
/// A malformed `claimed_hash` (not canonical 64-char lowercase hex) is
/// itself a mismatch — the durable store never persists/serves bytes
/// under a digest it cannot validate.
///
/// This is the single enforcement point for content-addressing on the
/// durable path. Every CAS write/read surface — the native
/// `PUT/GET /v1/cas/...` route, the Bazel REAPI v2 bridge, and sccache
/// — funnels through `R2CasHandler`, so this one gate closes the
/// cache-poisoning hole across all of them. (The in-memory handler
/// enforces the same invariant for dev/test.)
pub(crate) fn verify_content_hash(
    algo: DigestAlgo,
    claimed_hash: &str,
    bytes: &[u8],
) -> Result<(), String> {
    match algo {
        DigestAlgo::Blake3 => {
            let actual = Digest::compute(bytes);
            match Digest::from_hex(claimed_hash) {
                Ok(claimed) if claimed.verify_constant_time(&actual) => Ok(()),
                _ => Err(actual.to_hex()),
            }
        }
        DigestAlgo::Sha256 => {
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            let actual_hex = hex::encode(hasher.finalize());
            let claimed_lower = claimed_hash.to_ascii_lowercase();
            // Constant-time compare (same posture as the BLAKE3 path's
            // `verify_constant_time`): a malformed/short claim simply does not
            // match — never admitted.
            if actual_hex.len() == claimed_lower.len()
                && actual_hex
                    .as_bytes()
                    .ct_eq(claimed_lower.as_bytes())
                    .unwrap_u8()
                    == 1
            {
                Ok(())
            } else {
                Err(actual_hex)
            }
        }
    }
}
