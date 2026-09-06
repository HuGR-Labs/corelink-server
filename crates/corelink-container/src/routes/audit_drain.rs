//! Internal S-09 audit-chain drain endpoint.
//!
//! `POST /_internal/audit/drain` — seals the live `audit_outbox` trail into the
//! BLAKE3 tamper-evident hash chain. Until this drain runs, `audit_outbox` rows
//! are PLAIN, UNCHAINED CloudEvents envelopes (`emitted_at IS NULL`; see
//! `routes/dsr/audit.rs`), so the live audit trail is **not** tamper-evident —
//! a D1 writer could alter or delete a row undetected. The BLAKE3
//! [`HashChainBuilder`] (`corelink_audit_chain::chain`) is real + property-tested
//! but had no live producer; this is that producer.
//!
//! ## What it does
//!
//! For each `(tenant_id, region)` partition that has pending rows
//! (`emitted_at IS NULL`):
//!
//! 1. **Resume** the chain head. The authoritative resume point is the durable
//!    sealed-rows tail (the sealed row with the MAX `sequence_number`); the
//!    `audit_chain_head` checkpoint is the fast path. When the sealed tail is
//!    AHEAD of the checkpoint — a prior drain crashed after sealing rows but
//!    before advancing the checkpoint — we trust the sealed rows. This makes the
//!    drain crash-safe (the rows, not the checkpoint, are the source of truth).
//!    The builder is seeded via [`HashChainBuilder::resume`] (or
//!    [`HashChainBuilder::new`] at GENESIS).
//! 2. **Seal** each pending row IN ORDER (`enqueued_at, id`): compute the
//!    RFC-8785 JCS-canonical bytes of the payload, link it into the chain
//!    (`chain_hash = BLAKE3(prev_hash || canonical_jcs)` via
//!    [`link_chain_hash_from_canonical`]), and atomically write the sealed
//!    columns + flip `emitted_at`. The row UPDATE is guarded by
//!    `emitted_at IS NULL`, so a re-run never double-seals (IDEMPOTENT). The
//!    computation is fully deterministic, so two concurrent drains compute
//!    byte-identical seals — overlapping row writes are identical, never a fork.
//! 3. **Advance** the `audit_chain_head` checkpoint with a compare-and-set on the
//!    value we resumed from (SINGLE-WRITER anti-fork): if the stored head drifted
//!    (a concurrent drain advanced it), abort the partition rather than fork the
//!    chain. The rows we sealed are deterministic/identical to the concurrent
//!    drain's, so they remain safe; we simply do not double-advance.
//!
//! ## CF-6 — keyed (Ed25519-SIGNED) chain head (enterprise-DD finding #4)
//!
//! The BLAKE3 chain above is UNKEYED: an insider with D1 write can rewrite the
//! sealed `audit_outbox` rows, recompute a self-consistent chain + head, overwrite
//! the `audit_chain_head` checkpoint to match, and the unkeyed verifier still
//! passes. To make the head un-forgeable without a secret, every checkpoint
//! ADVANCE (step 3) ALSO Ed25519-SIGNS the canonical head tuple
//! `(tenant_id, region, head_hash, next_sequence)` — RFC-8785 JCS bytes via
//! [`canonical_head_bytes`] — and persists `head_signature` (base64) +
//! `head_signed_at_ms` + `signing_key_id` (migration 0080).
//!
//! On RESUME ([`check_head_on_resume`]), once a seed is configured (the signing
//! regime is ACTIVE), the resumed head MUST carry a signature that verifies under
//! the CURRENT key id against the canonical tuple + the seed-derived public key.
//! Anything else is TAMPER → the drain refuses to extend the chain (fail-CLOSED,
//! SEV-1): a verify failure, a NULL signature, a foreign `signing_key_id`, or a
//! signed head with no seed to verify it. This closes the laundering hole where an
//! insider with D1 write (but no seed) STRIPS the signature — they cannot re-sign,
//! and the honest drain no longer re-signs an unverifiable head for them.
//!
//! Legitimate pre-0080 legacy heads and coordinated seed/key ROTATIONS are handled
//! by an EXPLICIT, loudly-logged operator migration window
//! (`AUDIT_CHAIN_TRUST_UNSIGNED_RESUME=1`, default OFF) that temporarily tolerates
//! NULL / foreign-key-id heads while the fleet is re-signed — never a silent
//! tolerance. (Full D1-write-insider resistance additionally needs an external
//! anchor — Rekor / R2 Object-Lock — and signed key-rotation records; both remain
//! roadmap.)
//!
//! ### Key source (REUSED — no new secret)
//!
//! The signing key is the per-region erasure-attestation Ed25519 key
//! ([`ErasureSigningKey::from_seed`], seed `ERASURE_ATTESTATION_SEED_HEX`,
//! key id `ERASURE_ATTESTATION_KEY_ID`). The Ed25519 keypair is PURELY
//! seed-derived (`key.rs::from_seed` ignores region/timestamps for key material),
//! so one seed signs every partition and the partition's region is bound in the
//! SIGNED tuple — not in the key. An optional dedicated `AUDIT_CHAIN_SIGNING_SEED_HEX`
//! / `AUDIT_CHAIN_SIGNING_KEY_ID` takes precedence for operators who want key
//! separation. When NO seed is configured the head is advanced UNSIGNED
//! (legacy/tolerated) — preserving drain liveness in dev/CI.
//!
//! ## Why `link_chain_hash_from_canonical` and not `HashChainBuilder::append`
//!
//! `HashChainBuilder::append` takes a typed `corelink_audit_chain::AuditEvent` and
//! re-canonicalizes it. The `audit_outbox.payload_json` rows are GENERIC
//! CloudEvents JSON written by several sinks (`routes/dsr/audit.rs` etc.), NOT the
//! crate's `AuditEvent` struct, so `append` cannot consume them. Instead we
//! canonicalize the raw payload with `serde_jcs` and drive
//! `link_chain_hash_from_canonical` directly on those bytes — which is exactly the
//! verifier path the crate documents (the verifier reads the persisted JCS bytes
//! off the row and recomputes BLAKE3, never re-canonicalizing). The builder is
//! still used to seed `(head, next_sequence)` honestly via `resume`/`new`.
//!
//! Gated by the internal-auth shared secret (constant-time), mirroring
//! [`crate::routes::dsr`] byte-for-byte. Env-gated mount in [`crate::main`]
//! (unmounted in dev/CI without the erase key + D1).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer as _, Verifier as _};
use serde::Serialize;
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use corelink_audit_chain::{link_for_epoch, ChainEpoch, ChainHash, HashChainBuilder, LinkKey};
use corelink_erasure_attestation::{ErasureSigningKey, Region};

use crate::storage::d1_http::D1HttpClient;

// B126-M2 REANCHOR MANIFEST (routes/audit_drain).
// Ordered include fragments below are the sole composition point; this keeps
// the module namespace/API and execution order unchanged while preventing
// recomposition into a god-file. Symbols moved: INTERNAL_AUTH_HEADER, AuditDrainState, load_signing_seed, resolve_seed, signing_key_id_from_env, resolve_key_id, region_for_key, CanonicalAuditHead, CanonicalAuditHeadV2, canonical_head_bytes, canonical_head_v2_bytes, sign_head, sign_head_v2, verify_head, verify_head_v2, internal_auth_ok, build_state_from_env, router, now_ms, AUDIT_DRAIN_LEASE_TTL_MS, new_lease_holder, should_fence, FencedSeal, run_fenced_seal_loop, acquire_lease, release_lease, chain_hash_from_hex, checkpoint_nullable_u64, checkpoint_nullable_text, parse_sealed_epoch_metadata, HeadCheckpoint, SealedRow, PartitionOutcome, seal_rows, seal_rows_for_epoch, resolve_resume, HeadResumeCheck, check_head_on_resume, reject_unwired_epoch_checkpoint, read_pending_partitions, read_unsigned_head_partitions, read_checkpoint, read_sealed_tail, read_pending_rows, write_seal, advance_head_cas, resign_unsigned_head, converge_unsigned_heads, drain_partition, drain_partition_inner, handle_drain, fn, DrainOutcome, build_drain_response_body.
include!("audit_drain/b126_m2_impl_01.rs");
include!("audit_drain/b126_m2_impl_02.rs");
include!("audit_drain/b126_m2_impl_03.rs");

#[cfg(test)]
mod b126_m2_reanchor {
    #[test]
    fn implementation_fragments_are_wired() {
        let _ = [
            super::B126_M2_IMPL_1_REANCHOR,
            super::B126_M2_IMPL_2_REANCHOR,
            super::B126_M2_IMPL_3_REANCHOR,
        ];
    }
}
