//! Erasure-evidence gate on `VerifiedComplete` (WI-S11-008 Wave 1, G3).
//!
//! When the 24h verify sweep lands [`ErasureDecision::VerifiedComplete`] for a
//! DSR, [`sign_and_persist`] evaluates the per-backend verification evidence and
//! logs the outcome. The erasure itself is already done + audited + ledgered
//! (`audit_outbox`) by the time this runs; anything here is an *additional*
//! evidence artifact on top of that completed deletion.
//!
//! ## ⚠️ Signed + served attestation is DEFERRED (brutal-review H1)
//!
//! This module does NOT currently emit a cryptographic proof — it is an
//! *evidence digest gate*, not a certificate of erasure. An earlier version
//! computed an Ed25519 signature over the payload and then PERSISTED a D1 row
//! carrying only the UNSIGNED `evidence_hash`: the `signature_ed25519` and the
//! `canonical_payload_jcs` were discarded (migration 0032 has no columns for
//! them) and no R2 object was ever written (so the persisted `r2_key` pointed at
//! a non-existent object). That row therefore masqueraded as a cryptographic
//! certificate of erasure while being a forgeable, unsigned digest with no
//! verifier. Per the brutal review (finding H1) we now FAIL CLOSED honestly: we
//! do NOT persist that theater. We evaluate the evidence gate and log; the real
//! signed + served attestation is a deferred feature (see the DEFERRED block
//! below).
//!
//! ## Evidence gate (finding #1) — still evaluated
//!
//! [`verified_evidence_segments`] binds the decision to the ACTUAL per-backend
//! verification results carried by [`ErasureDecision::VerifiedComplete`]
//! (`Vec<BackendCompletion>`) — one segment per canonical backend =
//! `"{backend}:{outcome}:{hex(verification_hash)}"`. It REFUSES (returns `None`,
//! logged fail-CLOSED) when ANY of:
//!
//! - the completion set is not the canonical 12 (a partial/short decision);
//! - any backend's outcome is not successful; or
//! - any backend's `verification_hash` is not [`CANONICAL_EMPTY_TENANT_HASH`]
//!   (the re-fingerprint did not prove the backend empty / fully-redacted —
//!   includes the Stripe re-verify mismatch and any unverified arm).
//!
//! So an incomplete or unverified erasure is logged as fail-CLOSED, never
//! treated as verified. (The Stripe arm is a REAL re-fingerprint — see
//! `adapter_stripe.rs` — so a `CANONICAL_EMPTY_TENANT_HASH` from Stripe means a
//! genuine live-verified redaction, not a hardcoded no-op.)
//!
//! ## DEFERRED — what a REAL served attestation needs
//!
//! A genuine, verifiable "certificate of erasure" (NOT yet built) requires, end
//! to end:
//! - migration columns on `erasure_attestations` for `signature_ed25519` AND
//!   `canonical_payload_jcs` (0032 has neither today);
//! - persisting BOTH of those (not just `evidence_hash`) alongside the row;
//! - writing the signed JSON to the R2 audit object the `r2_key` points at
//!   (today no R2 object is ever written, so `r2_key` is a dangling pointer);
//! - the public verifier endpoints `GET /v1/public/attestation/{request_id}`
//!   and `GET /v1/public/keys/erasure/{region}.pub` (neither route exists);
//! - the `verify.rs` payload-binding so a served attestation's signature is
//!   checked against its canonical payload (brutal-review finding H2, fixed
//!   separately);
//! - re-wiring the per-region signing-key derivation + fail-CLOSED region
//!   resolution + public-key upsert (removed here with the theater; preserved in
//!   git history for the un-defer).
//!
//! Until ALL of the above land, this module must NOT pretend to hold a proof.

use std::sync::Arc;

use corelink_privacy_erasure_worker::backends::CANONICAL_EMPTY_TENANT_HASH;
use corelink_privacy_erasure_worker::event::{BackendCompletion, BACKEND_COUNT};

use crate::storage::d1_http::D1HttpClient;

/// `kms_provider` that a (DEFERRED) attestation would record — truthful:
/// CoreLink's launch erasure is a D1/R2/Stripe delete-set, not a BYOK KMS
/// crypto-erase. Retained for the deferred signed-attestation path and the
/// crypto-contract tests below; unused on the live (gate-only) path.
#[allow(dead_code)]
const ERASE_MECHANISM: &str = "corelink_d1r2_erase";

/// Build the evidence-digest segments from the REAL per-backend
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
/// (`"{backend}:{outcome}:{hex(verification_hash)}"`) in completion order. (The
/// signed proof that would consume these into a served `evidence_hash` is
/// DEFERRED — see the module-level DEFERRED block.)
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

/// Evaluate the erasure-evidence gate for a `VerifiedComplete` DSR and log the
/// outcome.
///
/// **This does NOT emit a cryptographic proof.** The signed + served erasure
/// attestation is DEFERRED (brutal-review H1; see the module docs). This
/// function deliberately refuses to persist an UNSIGNED row that would
/// masquerade as a certificate of erasure — it evaluates the fail-CLOSED
/// evidence gate and logs, nothing more.
///
/// Non-blocking: the erasure itself is already complete + audited + ledgered
/// (`audit_outbox`) before this runs. The caller invokes this ONLY on the
/// `VerifiedComplete` arm and passes that decision's real per-backend
/// `completions`. The unused params (`_d1`, `_tenant_id`, `_verified_at_ms`) are
/// retained so the call site is stable for when the signed-attestation path is
/// un-deferred.
pub(super) fn sign_and_persist(
    _d1: &Arc<D1HttpClient>,
    dsr_id: &str,
    _tenant_id: &str,
    _verified_at_ms: u64,
    completions: &[BackendCompletion],
) {
    // Fail-CLOSED evidence gate (finding #1): bind to the REAL per-backend
    // verification results; treat anything short of every canonical backend
    // provably verified-empty as fail-CLOSED.
    let Some(_evidence_segments) = verified_evidence_segments(completions) else {
        tracing::warn!(
            dsr_id = %dsr_id,
            backend_completions = completions.len(),
            "dsr/verify: per-backend verification evidence incomplete/unverified \
             — no attestation (fail-CLOSED)"
        );
        return;
    };

    // DEFERRED (brutal-review H1): the signed + served erasure attestation is
    // NOT implemented. We refuse to persist an UNSIGNED evidence row that would
    // masquerade as a cryptographic proof — migration 0032 has no
    // `signature_ed25519` / `canonical_payload_jcs` columns, the signature was
    // discarded, and no R2 object is ever written (so the persisted `r2_key`
    // would be a dangling pointer). The erasure itself is already complete +
    // audited + ledgered; emitting a forgeable "certificate" is worse than
    // emitting none. See the module-level DEFERRED block for the full list of
    // what a real served attestation requires before this can persist again.
    tracing::warn!(
        dsr_id = %dsr_id,
        backend_completions = completions.len(),
        "dsr/verify: erasure verified-complete; signed erasure-attestation \
         persistence is DEFERRED (brutal-review H1) — NOT writing an unsigned \
         attestation row; the erasure is complete + audited"
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

    /// Crypto-contract coverage for the `corelink_erasure_attestation` crate: a
    /// payload signed with a seed-derived key verifies against the public key
    /// derived from the SAME seed. NOTE: the signed + served attestation path is
    /// DEFERRED (brutal-review H1) — this module no longer signs or persists
    /// attestations / public keys, so there is NO D1 round-trip and NO route
    /// integration test; this covers only the offline signature contract that a
    /// future served attestation would rely on.
    #[test]
    fn signed_attestation_verifies_against_derived_public_key() {
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
