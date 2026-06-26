---
title: "OKF full-sweep — Wave B+C HIGH/MED remediation worklist"
type: "Audit"
status: "TO APPLY"
tags: ["okf","audit","sweep","remediation"]
---
# Wave B+C remediation (transcribe; confirm each enforcer line in this worktree before citing). After: okf_index.py + validate_okf.py GREEN at 146. checkpoint_sha unchanged.

## A. Crate-cluster UNDECLARED ENFORCERS (add the enforcing submodule to source_files + repoint the invariant cite from the lib.rs //!/barrel to the submodule impl line). For each: open the named submodule, find the enforcer, cite it.
1. crates/auth-pat.md: add corelink-pat/src/sig.rs (verify_hmac_sig), corelink-pat/src/argon.rs (verify_argon2id + dummy_verify). Repoint HMAC-fast-fail + Argon2id + dummy-pad invariants from corelink-pat/src/lib.rs barrels to those.
2. crates/billing-commerce.md: add corelink-billing-stripe/src/signature.rs (verify_stripe_signature), .../idempotency.rs (derive_idempotency_key), corelink-tier-selection/src/dpa.rs (DpaAcceptanceGate). Repoint INV-BILLING-NO-DUP / webhook-verify / DPA-first from the lib.rs //! to those.
3. crates/privacy-compliance.md: add corelink-dsr/src/mfa.rs, .../endpoint.rs, .../audit.rs (FailingDsrAuditSink). Repoint MFA-gate / fail-closed-audit / audit-before-store from dsr/lib.rs //!.
4. crates/audit-analytics.md: add corelink-audit-chain/src/chain.rs (HashChainBuilder/link_chain_hash), .../verifier.rs (ChainVerifier). Repoint chain-integrity/verifier invariants from audit/lib.rs + audit-chain/lib.rs //!.
5. crates/operations.md: add corelink-eviction/src/reachable.rs + blob_meta.rs (soft_delete), corelink-gc/src/worker.rs (GcWorker), the ratelimit token-bucket submodule. Repoint soft-delete-first / race-aware / gc-pause / per-tenant from the lib.rs //!.
6. crates/adapter-hosts.md: add corelink-bazel-bridge/src/{digest.rs,find_missing.rs,adapter.rs}. Repoint digest/find-missing invariants from bazel-bridge/lib.rs //!/const.
7. crates/container-platform.md: add corelink-config-do/src/{validation.rs,hash.rs} + corelink-cf-bindings/src/r2_real.rs. Repoint config-singleton CAS + CTRL-PRIV-001 from the //! docs.

## B. OVERSTATEMENT hedges (make intro/Invariants consistent with the concept's own gotcha / the code reality; lead-confirmed unwired):
8. storage/byok-envelope-encryption.md: BYOK production KMS provider is FEATURE-GATED (byok-aws-real) and NOT wired into the live R2 storage path (default = in-memory fake; no KmsProvider in storage.rs/r2_s3.rs). Reword intro "container wires the production provider / encrypts at-rest" → BYOK is the DESIGNED envelope-encryption layer; at-rest encryption is not yet on the live write path.
9. crates/cas-ac-core.md: CasWriteOrchestrator has ZERO refs in corelink-container (only consumer was the REMOVED gRPC handler). Reword "no transport may bypass CasWriteOrchestrator over every surface" → it is the DESIGNED correctness spine, not yet wired into the live path; the live native CAS enforces integrity via VerifiedBody (verified_body.rs:33-44), not the orchestrator.
10. storage/r2-ac-regional.md: How-it-works "synthetic probe per region every 5 min confirms presence" → match the gotcha: the CRR probe loop is DEFERRED (r2_crr.rs:16 ships only trait boundary + metric constants).
11. compliance/erasure-attestation.md [HIGH]: the crate is PURE-LOGIC Ed25519 signing only — NOT the KMS-destroy, NOT the R2(7y) persistence, NOT the /v1/public/keys serving endpoint (per the sibling launch/go-live-readiness: "never persisted, no serving endpoint, a non-functional stub"). Reword intro/Invariants to scope to the signing primitive + mark persistence/endpoint/KMS-destroy as the WI-S11-008 wiring (not live). Resolve the contradiction with go-live-readiness.
12. compliance/audit-chain.md: "R2 Object Lock enforces 7-year append-only at storage level" + "daily verifier walks" present-tense → match the gotcha: R2-Object-Lock append-only is INFRA wired at WI-S09-007 (no code enforcer); the daily verifier is the DO cron wired there. Repoint the verifier invariant cite from audit-chain/lib.rs:83-88 //! to verifier.rs:167-191 (impl).
13. compliance/data-residency.md [LOW]: "refuses ANY request whose region mismatches" → hedge: an absent region header returns Allow (disclosed in gotcha F-015/L-2).
14. launch/go-live-readiness.md [LOW]: add a "SUPERSEDED" note — this transcribes the 2026-06-15 NO-GO audit; those 2 CRITICALs were since closed/deployed (see /security/pentest-learnings) — so it is not the current verdict.
15. launch/money-path.md [LOW]: MAX_BATCH_RECORDS invariant cite const :119-121 → enforcer billing_ingest.rs:479.
