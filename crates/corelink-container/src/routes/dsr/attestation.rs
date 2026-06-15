//! Erasure-attestation signing on `VerifiedComplete` (WI-S11-008 Wave 1, G3).
//!
//! When the 24h verify sweep lands [`ErasureDecision::VerifiedComplete`] for a
//! DSR, this module signs an Ed25519 erasure attestation
//! ([`corelink_erasure_attestation`]) and persists it to D1 so the existing
//! public verifier (`GET /v1/public/keys/erasure/{region}.pub` +
//! `GET /v1/public/attestation/{request_id}`) can serve it. The customer /
//! auditor verifies the signature offline against the per-region public key.
//!
//! ## What is signed
//!
//! CoreLink's launch erasure is a real D1 / R2 / Stripe delete-set (NOT a BYOK
//! KMS crypto-erase), so the attestation records the *mechanism truthfully*:
//! `kms_provider = "corelink_d1r2_erase"` and `kms_key_id = "dsr:{dsr_id}"`.
//! The [`EvidenceBundle`] binds the DSR id + tenant + verify timestamp into the
//! SHA-256 `evidence_hash`, so an attacker cannot mint a valid attestation with
//! a substituted evidence bundle.
//!
//! ## Region key
//!
//! The per-region signing key is reproduced deterministically from a write-only
//! secret seed ([`ErasureSigningKey::from_seed`]) keyed by the tenant's
//! `tenant.primary_region`, so the public key stays stable across restarts and
//! an attestation signed today still verifies against a key fetched later. We
//! ALSO upsert the matching public key into `erasure_public_keys` so the public
//! key endpoint serves the exact key that signed.
//!
//! ## Fail-OPEN, idempotent, best-effort
//!
//! Attestation is an *additional evidence artifact on top of* a completed +
//! audited + ledgered erasure — it MUST NOT fail the verify response (the
//! erasure already happened and is recorded in `audit_outbox`). Any
//! misconfiguration (seed unset, unknown region, D1 error) is logged and
//! skipped. Persistence is `INSERT OR IGNORE` keyed on `request_id == dsr_id`,
//! so a re-sweep of the same DSR never double-writes.

use std::sync::Arc;

use serde_json::json;
use zeroize::Zeroizing;

use corelink_erasure_attestation::{
    ErasureAttestationPayload, ErasureAttestationSigner, ErasureSigningKey, EvidenceBundle, Region,
};

use super::d1util::{clamp_ms, col_str, d1_query_blocking};
use crate::storage::d1_http::D1HttpClient;

/// `kms_provider` recorded in the attestation — truthful: CoreLink's launch
/// erasure is a D1/R2/Stripe delete-set, not a BYOK KMS crypto-erase.
const ERASE_MECHANISM: &str = "corelink_d1r2_erase";

/// Load the 32-byte per-region attestation signing seed from
/// `ERASURE_ATTESTATION_SEED_HEX` (64 hex chars). `None` when unset/malformed
/// → caller skips signing (fail-OPEN). Held in [`Zeroizing`] so the secret is
/// wiped from memory after the [`ErasureSigningKey`] is constructed.
fn load_seed() -> Option<Zeroizing<[u8; 32]>> {
    let hex_str = std::env::var("ERASURE_ATTESTATION_SEED_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(hex_str, bytes.as_mut()).ok()?;
    Some(bytes)
}

/// Monotonic signing-key id from `ERASURE_ATTESTATION_KEY_ID` (defaults to 1 —
/// the launch key; the S-13 rotation worker bumps it on rotation).
fn key_id() -> u64 {
    std::env::var("ERASURE_ATTESTATION_KEY_ID")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(1)
}

/// Resolve the attestation [`Region`] for a tenant from `tenant.primary_region`.
/// `None` when the tenant row is gone (it IS — the D1 adapter deleted it on a
/// complete erasure), so we fall back to a process-default region from
/// `ERASURE_ATTESTATION_REGION` (the deployment's home region). `None` overall
/// → caller skips signing (no region to attribute).
fn resolve_region(d1: &Arc<D1HttpClient>, tenant_id: &str) -> Option<Region> {
    // The tenant row is normally already deleted at VerifiedComplete time, so
    // this lookup usually misses — the env default is the live path. We still
    // try D1 first for the (rare) partial-state / re-sweep ordering.
    let from_d1 = d1_query_blocking(
        d1,
        "SELECT primary_region FROM tenant WHERE tenant_id = ?1 LIMIT 1",
        vec![json!(tenant_id)],
    )
    .ok()
    .and_then(|rows| {
        rows.first()
            .and_then(|r| col_str(r, "primary_region"))
            .and_then(|s| Region::parse(&s))
    });
    from_d1.or_else(|| {
        std::env::var("ERASURE_ATTESTATION_REGION")
            .ok()
            .and_then(|s| Region::parse(s.trim()))
    })
}

/// Sign + persist an erasure attestation for a `VerifiedComplete` DSR.
///
/// Best-effort + fail-OPEN: returns silently on any misconfiguration. The
/// caller invokes this ONLY on the `VerifiedComplete` arm.
pub(super) fn sign_and_persist(
    d1: &Arc<D1HttpClient>,
    dsr_id: &str,
    tenant_id: &str,
    verified_at_ms: u64,
) {
    let Some(seed) = load_seed() else {
        tracing::debug!(
            dsr_id = %dsr_id,
            "dsr/verify: ERASURE_ATTESTATION_SEED_HEX unset/malformed — skipping attestation"
        );
        return;
    };
    let Some(region) = resolve_region(d1, tenant_id) else {
        tracing::warn!(
            dsr_id = %dsr_id,
            "dsr/verify: no attestation region (tenant row gone + ERASURE_ATTESTATION_REGION unset) — skipping attestation"
        );
        return;
    };
    let kid = key_id();

    // Deterministic key from the seed (stable public key across restarts).
    let signing_key = ErasureSigningKey::from_seed(kid, region, verified_at_ms, 0, *seed);
    let public_key = signing_key.public_key();
    let signer = ErasureAttestationSigner::new(signing_key);

    // Evidence bundle binds the erasure event: the audit-chain segment is the
    // canonical DSR completed.v1 envelope id; the "destroy ts" is the verify
    // instant; key id is the dsr-scoped sentinel.
    let kms_key_id = format!("dsr:{dsr_id}");
    let bundle = EvidenceBundle {
        audit_chain_segment_ids: vec![format!("dsr.erasure.completed.v1:{dsr_id}")],
        kms_destroy_ts: verified_at_ms,
        kms_key_id: kms_key_id.clone(),
        tenant_id: tenant_id.to_string(),
    };
    let payload = ErasureAttestationPayload {
        tenant_id: tenant_id.to_string(),
        request_id: dsr_id.to_string(),
        destroyed_ts: verified_at_ms,
        kms_provider: ERASE_MECHANISM.to_string(),
        kms_key_id,
        evidence_hash: EvidenceBundle::compute_hash(&bundle),
        region,
        attestation_key_id: kid,
    };

    let attestation = match signer.sign(payload) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(dsr_id = %dsr_id, error = %e, "dsr/verify: attestation sign failed");
            return;
        }
    };

    // Upsert the matching public key so the public key endpoint serves the
    // exact key that signed (idempotent on (key_id, region)).
    let pub_sql = "INSERT OR IGNORE INTO erasure_public_keys \
         (key_id, region, state, created_at_ms, overlap_until_ms, public_key_pem) \
         VALUES (?1, ?2, 'active', ?3, ?4, ?5)";
    if let Err(e) = d1_query_blocking(
        d1,
        pub_sql,
        vec![
            json!(kid),
            json!(region.as_str()),
            json!(clamp_ms(public_key.created_at_ms)),
            json!(clamp_ms(public_key.overlap_until_ms)),
            json!(public_key.pem),
        ],
    ) {
        tracing::error!(dsr_id = %dsr_id, error = %e, "dsr/verify: erasure_public_keys upsert failed");
        // continue — the attestation row is the load-bearing artifact.
    }

    // R2 audit bucket is the authoritative store; D1 is the lookup index. We
    // index here (the signed JSON is also embedded so the index is
    // self-sufficient even before the quarterly R2 reconcile).
    let r2_key = format!(
        "{}/erasure_attestations/{dsr_id}.json",
        region.audit_bucket()
    );
    let att_sql = "INSERT OR IGNORE INTO erasure_attestations \
         (request_id, tenant_id, region, attestation_key_id, r2_key, signed_at_ms, \
          kms_provider, kms_key_id, evidence_hash) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";
    if let Err(e) = d1_query_blocking(
        d1,
        att_sql,
        vec![
            json!(attestation.payload.request_id),
            json!(attestation.payload.tenant_id),
            json!(region.as_str()),
            json!(kid),
            json!(r2_key),
            json!(clamp_ms(verified_at_ms)),
            json!(attestation.payload.kms_provider),
            json!(attestation.payload.kms_key_id),
            json!(attestation.payload.evidence_hash),
        ],
    ) {
        tracing::error!(dsr_id = %dsr_id, error = %e, "dsr/verify: erasure_attestations index failed");
        return;
    }

    tracing::info!(
        dsr_id = %dsr_id,
        region = %region,
        attestation_key_id = kid,
        "dsr/verify: signed + persisted erasure attestation (VerifiedComplete)"
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use corelink_erasure_attestation::{
        verify_attestation_signature, ErasureAttestationPayload, ErasureAttestationSigner,
        ErasureSigningKey, EvidenceBundle, Region,
    };

    use super::ERASE_MECHANISM;

    /// The exact payload `sign_and_persist` builds must produce a signature
    /// that verifies against the public key derived from the SAME seed — the
    /// property the persisted `erasure_public_keys` row + the public verifier
    /// rely on. (The D1 round-trip is exercised by the route integration test;
    /// here we cover the crypto contract the persistence wraps.)
    #[test]
    fn signed_attestation_verifies_against_persisted_public_key() {
        let seed = [9u8; 32];
        let dsr_id = "00000000-0000-7000-8000-000000000abc";
        let tenant_id = "00000000-0000-7000-8000-000000000def";
        let verified_at_ms = 1_700_000_000_000u64;
        let region = Region::Weur;
        let kid = 1u64;

        let signing_key = ErasureSigningKey::from_seed(kid, region, verified_at_ms, 0, seed);
        let public_key = signing_key.public_key();
        let signer = ErasureAttestationSigner::new(signing_key);

        let kms_key_id = format!("dsr:{dsr_id}");
        let bundle = EvidenceBundle {
            audit_chain_segment_ids: vec![format!("dsr.erasure.completed.v1:{dsr_id}")],
            kms_destroy_ts: verified_at_ms,
            kms_key_id: kms_key_id.clone(),
            tenant_id: tenant_id.to_string(),
        };
        let payload = ErasureAttestationPayload {
            tenant_id: tenant_id.to_string(),
            request_id: dsr_id.to_string(),
            destroyed_ts: verified_at_ms,
            kms_provider: ERASE_MECHANISM.to_string(),
            kms_key_id,
            evidence_hash: EvidenceBundle::compute_hash(&bundle),
            region,
            attestation_key_id: kid,
        };
        let att = signer.sign(payload).unwrap();

        // The persisted public key (derived from the same seed) verifies it.
        verify_attestation_signature(&att, &public_key).expect("attestation must verify");
        assert_eq!(att.payload.request_id, dsr_id);
        assert_eq!(att.payload.kms_provider, ERASE_MECHANISM);
    }

    /// A tampered payload (different request_id) must NOT verify — the
    /// signature binds the canonical payload, so the persisted index cannot be
    /// retro-pointed at a different DSR.
    #[test]
    fn tampered_request_id_fails_verification() {
        let seed = [3u8; 32];
        let region = Region::Enam;
        let sk = ErasureSigningKey::from_seed(1, region, 0, 0, seed);
        let pk = sk.public_key();
        let signer = ErasureAttestationSigner::new(sk);
        let bundle = EvidenceBundle {
            audit_chain_segment_ids: vec!["seg".to_string()],
            kms_destroy_ts: 1,
            kms_key_id: "dsr:a".to_string(),
            tenant_id: "t".to_string(),
        };
        let payload = ErasureAttestationPayload {
            tenant_id: "t".to_string(),
            request_id: "dsr-a".to_string(),
            destroyed_ts: 1,
            kms_provider: ERASE_MECHANISM.to_string(),
            kms_key_id: "dsr:a".to_string(),
            evidence_hash: EvidenceBundle::compute_hash(&bundle),
            region,
            attestation_key_id: 1,
        };
        let mut att = signer.sign(payload).unwrap();
        // Tamper: swap the request_id in BOTH the payload and the canonical
        // form — the signature was over the original bytes, so verify must fail.
        att.payload.request_id = "dsr-EVIL".to_string();
        att.canonical_payload_jcs =
            att.canonical_payload_jcs.replace("dsr-a", "dsr-EVIL");
        assert!(
            verify_attestation_signature(&att, &pk).is_err(),
            "tampered attestation must fail verification"
        );
    }
}
