//! Erasure-attestation signing on `VerifiedComplete` (WI-S11-008 Wave 1, G3).
//!
//! When the 24h verify sweep lands [`ErasureDecision::VerifiedComplete`] for a
//! DSR, this module signs an Ed25519 erasure attestation
//! ([`corelink_erasure_attestation`]) and persists it to D1 so the existing
//! public verifier (`GET /v1/public/keys/erasure/{region}.pub` +
//! `GET /v1/public/attestation/{request_id}`) can serve it. The customer /
//! auditor verifies the signature offline against the per-region public key.
//!
//! ## What is signed (binds REAL per-backend verification — finding #1)
//!
//! CoreLink's launch erasure is a real D1 / R2 / Stripe delete-set (NOT a BYOK
//! KMS crypto-erase), so the attestation records the *mechanism truthfully*:
//! `kms_provider = "corelink_d1r2_erase"` and `kms_key_id = "dsr:{dsr_id}"`.
//!
//! The signed `evidence_hash` is bound to the ACTUAL per-backend verification
//! results carried by the orchestrator's [`ErasureDecision::VerifiedComplete`]
//! decision (`Vec<BackendCompletion>`), NOT a synthetic self-referential
//! constant. The [`EvidenceBundle::audit_chain_segment_ids`] carries one
//! segment per canonical backend = `"{backend}:{outcome}:{hex(verification_hash)}"`,
//! so the SHA-256 is a function of every backend's re-fingerprint outcome; an
//! auditor recomputes it from the per-backend ledger. The hash is built via
//! [`EvidenceBundle::validated_hash`], so the "empty mandatory field" guard is
//! load-bearing (an empty completion set is a hard refuse, not a vacuous pass).
//!
//! ## Fail-CLOSED signing gate (finding #1)
//!
//! The signer emits a proof ONLY when it is bound to real, complete deletion
//! evidence. It REFUSES to sign (no attestation row written; the erasure is
//! still done + audited + ledgered) when ANY of:
//!
//! - the completion set is not the canonical 12 (a partial/short decision);
//! - any backend's outcome is not successful;
//! - any backend's `verification_hash` is not [`CANONICAL_EMPTY_TENANT_HASH`]
//!   (the re-fingerprint did not prove the backend empty / fully-redacted —
//!   includes the Stripe re-verify mismatch and any unverified arm);
//! - the seed secret is unset/malformed; or
//! - no trustworthy signing region can be resolved (see below).
//!
//! (The Stripe arm is now a REAL re-fingerprint — see `adapter_stripe.rs`,
//! finding #1 option a — so a `CANONICAL_EMPTY_TENANT_HASH` from Stripe means a
//! genuine live-verified redaction, not a hardcoded no-op.)
//!
//! ## Region key (no silent mis-attribution — finding #1)
//!
//! The per-region signing key is reproduced deterministically from a write-only
//! secret seed ([`ErasureSigningKey::from_seed`]) keyed by the tenant's region.
//! At `VerifiedComplete` the `tenant` row is normally already deleted, so the
//! authoritative `tenant.primary_region` lookup usually misses. We MUST NOT
//! silently fall back to the deployment home region (that would let an EU
//! tenant's erasure be signed with the wrong key). The env region
//! (`ERASURE_ATTESTATION_REGION`) is honoured ONLY when the operator has
//! EXPLICITLY asserted this deployment is single-region
//! (`ERASURE_ATTESTATION_SINGLE_REGION` truthy) — an audited operator decision,
//! NOT a silent default. Otherwise the region is treated as unavailable and the
//! attestation is withheld (fail-CLOSED). We ALSO upsert the matching public
//! key into `erasure_public_keys` so the public key endpoint serves the exact
//! key that signed.
//!
//! ## Non-blocking, idempotent
//!
//! Attestation is an *additional evidence artifact on top of* a completed +
//! audited + ledgered erasure — it MUST NOT fail the verify response (the
//! erasure already happened and is recorded in `audit_outbox`). It is therefore
//! non-blocking, but it fails CLOSED on the EVIDENCE: rather than emit a proof
//! that proves nothing, it logs and skips. Persistence is `INSERT OR IGNORE`
//! keyed on `request_id == dsr_id`, so a re-sweep of the same DSR never
//! double-writes.

use std::sync::Arc;

use serde_json::json;
use zeroize::Zeroizing;

use corelink_erasure_attestation::{
    ErasureAttestationPayload, ErasureAttestationSigner, ErasureSigningKey, EvidenceBundle, Region,
};
use corelink_privacy_erasure_worker::backends::CANONICAL_EMPTY_TENANT_HASH;
use corelink_privacy_erasure_worker::event::{BackendCompletion, BACKEND_COUNT};

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

/// Whether the operator has EXPLICITLY asserted this deployment is
/// single-region via `ERASURE_ATTESTATION_SINGLE_REGION` (truthy). This makes
/// `ERASURE_ATTESTATION_REGION` an audited operator decision rather than a
/// silent default for the (live) case where the tenant row is already deleted.
fn single_region_asserted() -> bool {
    std::env::var("ERASURE_ATTESTATION_SINGLE_REGION")
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Resolve the attestation [`Region`] for a tenant WITHOUT silent
/// mis-attribution (finding #1, region).
///
/// 1. Authoritative: `tenant.primary_region`, IF the row still exists (rare
///    partial-state / re-sweep ordering before the D1 adapter deleted it). When
///    present this is always correct.
/// 2. Tenant row gone (the live path): there is NO pre-deletion region source
///    in this layer (`dsr_requested` / `DsrVerifyV1` carry none — see the card).
///    We do NOT silently default to the deployment home region. The env region
///    is used ONLY when the operator has EXPLICITLY asserted single-region; that
///    assertion is the only thing that makes attributing the erasure to
///    `ERASURE_ATTESTATION_REGION` correct.
/// 3. Otherwise → `None` (caller withholds the attestation, fail-CLOSED — the
///    erasure is still done + audited; we just do not sign with a possibly-wrong
///    region).
fn resolve_region(d1: &Arc<D1HttpClient>, tenant_id: &str) -> Option<Region> {
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
    if let Some(region) = from_d1 {
        return Some(region);
    }
    // Tenant row gone: only an explicit single-region operator assertion makes
    // the env region a correct (non-silent) attribution.
    if !single_region_asserted() {
        return None;
    }
    std::env::var("ERASURE_ATTESTATION_REGION")
        .ok()
        .and_then(|s| Region::parse(s.trim()))
}

/// Build the signed `evidence_hash` segments from the REAL per-backend
/// verification outcomes carried by the `VerifiedComplete` decision — or `None`
/// (refuse to sign) unless EVERY canonical backend is provably verified-empty.
///
/// Fail-CLOSED conditions (each ⇒ `None`, attestation withheld):
/// - the completion set is not the canonical 12 (a partial/short decision);
/// - any backend's outcome is not successful; or
/// - any backend's `verification_hash` is not [`CANONICAL_EMPTY_TENANT_HASH`]
///   (the re-fingerprint did NOT prove the backend empty / fully-redacted —
///   includes a Stripe re-verify mismatch or any unverified arm).
///
/// On success returns one canonical segment per backend
/// (`"{backend}:{outcome}:{hex(verification_hash)}"`) in completion order, to be
/// hashed into the signed `evidence_hash` so the proof binds the actual
/// per-backend evidence.
fn verified_evidence_segments(completions: &[BackendCompletion]) -> Option<Vec<String>> {
    if completions.len() != BACKEND_COUNT {
        return None;
    }
    let mut segments = Vec::with_capacity(completions.len());
    for c in completions {
        // A complete proof requires every backend to be a SUCCESSFUL outcome
        // AND to have re-fingerprinted to the canonical verified-empty sentinel.
        if !c.outcome.is_successful() {
            return None;
        }
        if c.verification_hash != CANONICAL_EMPTY_TENANT_HASH {
            return None;
        }
        segments.push(format!(
            "{backend}:{outcome}:{vhash}",
            backend = c.backend,
            outcome = c.outcome.as_str(),
            vhash = hex::encode(c.verification_hash),
        ));
    }
    Some(segments)
}

/// Sign + persist an erasure attestation for a `VerifiedComplete` DSR.
///
/// Non-blocking but fail-CLOSED on the evidence: returns silently (no row
/// written) on any misconfiguration OR when the per-backend verification
/// evidence is incomplete/unverified OR when no trustworthy region resolves.
/// The caller invokes this ONLY on the `VerifiedComplete` arm and passes that
/// decision's real per-backend `completions`.
pub(super) fn sign_and_persist(
    d1: &Arc<D1HttpClient>,
    dsr_id: &str,
    tenant_id: &str,
    verified_at_ms: u64,
    completions: &[BackendCompletion],
) {
    // Fail-CLOSED evidence gate (finding #1): bind the proof to the REAL
    // per-backend verification results, and refuse to sign unless every
    // canonical backend is provably verified-empty.
    let Some(evidence_segments) = verified_evidence_segments(completions) else {
        tracing::warn!(
            dsr_id = %dsr_id,
            backend_completions = completions.len(),
            "dsr/verify: per-backend verification evidence incomplete/unverified \
             — NOT signing attestation (fail-CLOSED)"
        );
        return;
    };
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
            "dsr/verify: no trustworthy attestation region (tenant row gone + no \
             explicit ERASURE_ATTESTATION_SINGLE_REGION assertion) — NOT signing \
             attestation (fail-CLOSED; never a possibly-wrong region)"
        );
        return;
    };
    let kid = key_id();

    // Deterministic key from the seed (stable public key across restarts).
    let signing_key = ErasureSigningKey::from_seed(kid, region, verified_at_ms, 0, *seed);
    let public_key = signing_key.public_key();
    let signer = ErasureAttestationSigner::new(signing_key);

    // Evidence bundle binds the REAL per-backend verification results (one
    // segment per canonical backend = "{backend}:{outcome}:{hex(verification_hash)}")
    // — NOT a synthetic self-referential constant. `validated_hash` makes the
    // "empty mandatory field" guard load-bearing (an empty segment set is a hard
    // refuse). The "destroy ts" is the verify instant; key id is the dsr-scoped
    // sentinel.
    let kms_key_id = format!("dsr:{dsr_id}");
    let bundle = EvidenceBundle {
        audit_chain_segment_ids: evidence_segments,
        kms_destroy_ts: verified_at_ms,
        kms_key_id: kms_key_id.clone(),
        tenant_id: tenant_id.to_string(),
    };
    let evidence_hash = match bundle.validated_hash() {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(
                dsr_id = %dsr_id,
                error = %e,
                "dsr/verify: evidence bundle failed validation — NOT signing attestation (fail-CLOSED)"
            );
            return;
        }
    };
    let payload = ErasureAttestationPayload {
        tenant_id: tenant_id.to_string(),
        request_id: dsr_id.to_string(),
        destroyed_ts: verified_at_ms,
        kms_provider: ERASE_MECHANISM.to_string(),
        kms_key_id,
        evidence_hash,
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
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]
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

    // ---- finding #1: fail-CLOSED evidence gate ----

    use corelink_privacy_erasure_worker::backends::CANONICAL_EMPTY_TENANT_HASH;
    use corelink_privacy_erasure_worker::event::{
        canonical_backend_kinds, BackendCompletion, BackendErasureOutcome, BackendKind, BACKEND_COUNT,
    };
    use uuid::Uuid;

    fn completion(backend: BackendKind, outcome: BackendErasureOutcome, vhash: [u8; 32]) -> BackendCompletion {
        BackendCompletion {
            dsr_id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            subject_id_hash: [0u8; 32],
            backend,
            outcome,
            idempotency_key: "k".to_string(),
            started_at_ms: 0,
            completed_at_ms: 1,
            retry_count: 0,
            verification_hash: vhash,
        }
    }

    /// Canonical 12, every backend successful + verified-empty → segments bind
    /// the real per-backend evidence (one segment per backend).
    fn all_verified_completions() -> Vec<BackendCompletion> {
        canonical_backend_kinds()
            .iter()
            .map(|k| {
                let outcome = if k.is_effective() {
                    BackendErasureOutcome::Erased { records_deleted: 0 }
                } else {
                    BackendErasureOutcome::Pseudonymized { records_redacted: 0 }
                };
                completion(*k, outcome, CANONICAL_EMPTY_TENANT_HASH)
            })
            .collect()
    }

    #[test]
    fn evidence_segments_bind_real_hashes_when_all_verified() {
        let segments = super::verified_evidence_segments(&all_verified_completions())
            .expect("all-verified must produce segments");
        assert_eq!(segments.len(), BACKEND_COUNT);
        // Each segment carries the real per-backend verification hash (hex).
        let empty_hex = hex::encode(CANONICAL_EMPTY_TENANT_HASH);
        assert!(segments.iter().all(|s| s.contains(&empty_hex)));
        // NOT the old synthetic constant.
        assert!(segments.iter().all(|s| !s.contains("dsr.erasure.completed.v1")));
    }

    #[test]
    fn evidence_refuses_when_a_backend_hash_is_not_empty_sentinel() {
        // Stripe re-verify mismatch (non-sentinel hash) MUST refuse the proof.
        let comps: Vec<BackendCompletion> = canonical_backend_kinds()
            .iter()
            .map(|k| {
                if *k == BackendKind::Stripe {
                    completion(
                        BackendKind::Stripe,
                        BackendErasureOutcome::Pseudonymized { records_redacted: 1 },
                        [0x11; 32], // non-sentinel mismatch
                    )
                } else {
                    completion(*k, BackendErasureOutcome::Erased { records_deleted: 0 }, CANONICAL_EMPTY_TENANT_HASH)
                }
            })
            .collect();
        assert!(
            super::verified_evidence_segments(&comps).is_none(),
            "an unverified arm must withhold the proof"
        );
    }

    #[test]
    fn evidence_refuses_on_unsuccessful_outcome() {
        let first = *canonical_backend_kinds().first().expect("12 kinds");
        let comps: Vec<BackendCompletion> = canonical_backend_kinds()
            .iter()
            .map(|k| {
                if *k == first {
                    completion(*k, BackendErasureOutcome::Failed { retry_after_seconds: 60 }, CANONICAL_EMPTY_TENANT_HASH)
                } else {
                    completion(*k, BackendErasureOutcome::Erased { records_deleted: 0 }, CANONICAL_EMPTY_TENANT_HASH)
                }
            })
            .collect();
        assert!(super::verified_evidence_segments(&comps).is_none());
    }

    #[test]
    fn evidence_refuses_on_wrong_completion_count() {
        assert!(super::verified_evidence_segments(&[]).is_none());
        let one = vec![completion(
            *canonical_backend_kinds().first().expect("12 kinds"),
            BackendErasureOutcome::Erased { records_deleted: 0 },
            CANONICAL_EMPTY_TENANT_HASH,
        )];
        assert!(super::verified_evidence_segments(&one).is_none());
    }
}
