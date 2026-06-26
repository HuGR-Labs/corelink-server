---
type: "ComplianceControl"
title: "DSR / right-to-erasure pipeline"
description: "How a GDPR Art.17 data-subject erasure request is intaked, MFA-gated, orchestrated across the live data planes, CAS-tombstoned, and attested in CoreLink."
source_files:
  - docs/security/2026-06-23-secreview-gdpr-residency.md
  - crates/corelink-container/src/routes/dsr.rs
  - crates/corelink-container/src/routes/customer.rs
  - crates/corelink-container/src/routes/cas_erase.rs
  - crates/corelink-container/src/routes/cas.rs
  - crates/corelink-dsr/src/endpoint.rs
  - crates/corelink-dsr/src/store.rs
  - crates/corelink-dsr/src/mfa.rs
  - crates/corelink-dsr/src/receipt.rs
  - crates/corelink-dsr/src/event.rs
  - crates/corelink-dsr/src/lib.rs
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["dsr", "gdpr", "lgpd", "erasure", "right-to-erasure", "compliance", "mfa", "attestation"]
timestamp: "2026-06-26T00:00:00Z"
---

CoreLink is a multi-tenant content-addressable cache sold self-serve to SMBs, so it holds tenant PII and content and must honour data-subject erasure (GDPR Art.17 / LGPD Art.18). The pipeline has two halves: a pure-logic **self-service DSR endpoint** (`corelink-dsr`) that intakes a request, gates destructive arms behind step-up MFA, and issues a signed receipt; and the **live Wave-1 erasure engine** in the container (`routes/dsr.rs`) that actually deletes real bytes across the configured backends and is re-confirmed by a 24h verify sweep. A go-live security review found erasure "complete-by-construction for the live data planes (CAS + AC + D1 + Stripe)" with no launch-blocking finding (`docs/security/2026-06-23-secreview-gdpr-residency.md:12-18`). The whole pipeline is regulatory-grade fail-CLOSED: every state mutation is preceded by a durable audit row, and an erasure is never attested complete while subject bytes survive.

# Role

This control implements the data-subject right-to-erasure (and the sibling DSR rights: access, portability, rectification, restriction, objection). It is the compliance seam that converts a customer's "delete my account/data" demand — whether self-served or originated from a Clerk `user.deleted` webhook — into a verified, audited, attested deletion across CoreLink's storage backends, with constant-time cross-tenant confidentiality and a customer-held proof-of-submission receipt.

# How it works

- **Intake (self-service endpoint).** `InMemoryDsrEndpoint::submit` runs a serial pipeline per request: it first fires the `request_received` audit row BEFORE any state mutation, then an idempotency lookup, then the MFA gate, then receipt-issue + store-insert (`crates/corelink-dsr/src/endpoint.rs:299-418`).
- **MFA step-up on destructive arms only.** Erasure and Rectification require a non-empty step-up token; without one the orchestrator returns `MfaRequired` after a `mfa_step_up_required` audit, with no store insert; read/policy arms skip the gate (`crates/corelink-dsr/src/endpoint.rs:351-379`).
- **The MFA verifier contract.** `MfaStepUpVerifier::verify` is fail-CLOSED: `None` token → `Required`, empty bytes / bad binding → `Invalid`, expired → `Expired`; production wiring binds it to a 5-min WebAuthn step-up token scoped to `op_class = "dsr_destructive"` (`crates/corelink-dsr/src/mfa.rs:80-102`).
- **Idempotency by `(tenant_id, request_id)`.** A prior non-terminal ticket short-circuits to the original receipt + SLA deadline with no second audit/insert; a replay with a divergent `(data_subject_id, request_kind, jurisdiction)` tuple is a SEV-1 forensic anomaly surfaced as `DivergentPayload` (`crates/corelink-dsr/src/store.rs:133-158`).
- **Receipt + SLA.** On accept, a JWT (RS256) receipt is issued with a 90-day anti-replay expiry cap and an SLA deadline derived per jurisdiction (LGPD 15d / GDPR 30d / CCPA 45d) by the `pub const fn sla_for` enforcer (`crates/corelink-dsr/src/receipt.rs:136-149`, `crates/corelink-dsr/src/event.rs:403`, `crates/corelink-dsr/src/lib.rs:116-119`).
- **Status poll is constant-time confidential.** `poll_status` returns the ticket status on a hit, but a miss OR a cross-tenant lookup both surface as `RequestRejected{IdentityVerificationFailed}` — never disclosing whether a `request_id` exists for another tenant (`crates/corelink-dsr/src/endpoint.rs:420-457`).
- **Live erasure intake (container).** `POST /_internal/dsr/erase` deserialises the `dsr.queued.v1` message (forwarded by the signup-worker queue consumer for the Clerk `user.deleted` path), maps it to a canonical `ErasureRequest`, and drives the 12-backend orchestrator (`crates/corelink-container/src/routes/dsr.rs:369-410`).
- **Self-serve account-delete is DESIGNED to enqueue, but fail-closes today.** A self-serve account delete routes through `routes/customer.rs::handle_account_delete` (`crates/corelink-container/src/routes/customer.rs:940`), which is DESIGNED to build a `dsr.queued.v1` message, `INSERT OR IGNORE` a `dsr_requested` anchor row, and hand erasure to the `DsrErasureSink` trait — enqueueing to a queue (async), not a synchronous same-worker drive (the only in-tree sink impl is a test `MockSink`). TODAY, however, the container route fail-closes **503 "account deletion not configured"** whenever the `account_deletion` requester is unwired (it is `None` outside `#[cfg(test)]`), so the self-serve path is not LIVE until the sink is wired (`crates/corelink-container/src/routes/customer.rs:959`). The LIVE erasure engine is the webhook-originated `/_internal/dsr/erase` path above.
- **The 12 backends, honestly reconciled.** `build_d1_worker` wires REAL transports for the 4 live planes — D1 erase-set, R2 CAS, R2 AC, and Stripe pseudonymisation — and reconciles the other 8 (Neon/KV/Loki/audit/legal-hold/evidence, not shipped in prod) to `NotApplicable` with a documented reason each, a truthful GDPR record not a silent skip (`crates/corelink-container/src/routes/dsr.rs:189-244`).
- **CAS tombstone / per-hash erase.** `POST /_internal/cas/:tenant/:hash/erase` deletes the blob from R2 and writes a durable `cas_tombstone` (migration 0067) so subsequent reads return 410 Gone, never 404 or 200 (`crates/corelink-container/src/routes/cas_erase.rs:1-8`, `crates/corelink-container/src/routes/cas_erase.rs:70`).
- **Bloom-gated tombstone reads.** `BloomTombstoneStore::is_tombstoned` answers a definitely-absent fast-path only against a bloom seeded from the authoritative D1 set, falling through to D1 on maybe-present or a just-reloaded epoch (`crates/corelink-container/src/routes/cas_erase.rs:830-850`).
- **24h verify sweep + attestation.** `POST /_internal/dsr/verify` re-fingerprints every backend; on a fully `VerifiedComplete` decision it signs and persists an Ed25519 erasure attestation (best-effort, fail-OPEN since the erasure is already done and audited) (`crates/corelink-container/src/routes/dsr.rs:443-446`).

# Invariants

- **Audit-before-mutation, fail-CLOSED.** Every decision arm fires its canonical audit row BEFORE the state-mutating step; an audit failure aborts the request and propagates `DsrError::Audit` with no mutation past that point (`crates/corelink-dsr/src/endpoint.rs:312-318`).
- **MFA mandatory for Erasure + Rectification, never for read/policy arms.** The gate is structural in the orchestrator pipeline (`crates/corelink-dsr/src/endpoint.rs:354-379`); the verifier impl rejects `None`/empty tokens (`crates/corelink-dsr/src/mfa.rs:146-154`); token expiry is in the trait contract (`crates/corelink-dsr/src/mfa.rs:80-102`).
- **Tenant isolation is structural.** The ticket store is keyed `(tenant_id, request_id)` leftmost, so cross-tenant reads are impossible by construction (`crates/corelink-dsr/src/store.rs:45`).
- **Erasure intake is legitimacy-gated (anti mass-erase).** Every erase binds to a D1-authenticated `dsr_requested` row; a forged body-asserted `tenant_id` with the shared internal key lands as `Rejected` → HTTP 422, no fan-out, no bytes deleted; a D1 fault also fails CLOSED (`crates/corelink-container/src/routes/dsr.rs:397-398`, `crates/corelink-container/src/routes/dsr.rs:180-184`).
- **The destructive route is mounted only with a ≥32-char dedicated key.** `build_state_from_env` prefers `CORELINK_ERASE_AUTH_KEY`, falls back to the shared internal key, and refuses to mount below the 32-char floor (`crates/corelink-container/src/routes/dsr.rs:256-272`); the check itself is constant-time, length-folded (`crates/corelink-container/src/routes/dsr.rs:126-143`).
- **Cannot attest "complete" while data survives.** The attestation is signed only on the `VerifiedComplete` arm, which the orchestrator emits iff every backend's verification hash is the canonical-empty sentinel — surviving CAS/AC bytes yield a non-canonical hash → `VerifiedPartial` → no signature (`crates/corelink-container/src/routes/dsr.rs:443-446`, `docs/security/2026-06-23-secreview-gdpr-residency.md:75-85`).
- **A tombstoned hash never resurrects.** A read of a tombstoned digest returns 410 on the native CAS route — the read-side enforcer short-circuits `Ok(true) => return (StatusCode::GONE, "erased")` BEFORE the R2 GET (`crates/corelink-container/src/routes/cas.rs:665-676`, tombstone-WRITE side `crates/corelink-container/src/routes/cas_erase.rs:1-8`), a re-PUT is refused, and a gate transport fault fails CLOSED 503; the bloom is re-seeded from D1 on every reload to close the cross-writer false-negative window (`crates/corelink-container/src/routes/cas_erase.rs:797-825`).
- **The receipt caps customer-side replay at 90 days.** `DsrReceipt::is_expired` enforces the `submitted + 90d` cap independent of DB state (`crates/corelink-dsr/src/receipt.rs:153-154`).

# Gotchas

- The `corelink-dsr` crate is the **pure-logic skeleton** (trait + in-memory fakes) for the self-service DSR API; its production CF Worker / RS256-from-KMS / WebAuthn / Neon `dsr_tickets` wiring is deferred to WI-S11-008 (`crates/corelink-dsr/src/lib.rs:1-15`). The bytes-actually-deleted path that is LIVE today is the container `routes/dsr.rs` Wave-1 engine, not this crate's `InMemoryErasureWorker`.
- In the container, an unconfigured `StorageEnv` makes `build_d1_worker` return `None`; the erase route then falls back to an all-placeholder worker whose EMPTY legitimacy store rejects EVERY request — deliberately fail-CLOSED, never an allow-all on a route-mountable path (`crates/corelink-container/src/routes/dsr.rs:151-166`). A self-serve account-delete with no real worker fails CLOSED 503 rather than silently "succeeding".
- Attestation is fail-OPEN by design: a missing signature does NOT mean the erasure failed — the erasure is already complete and audited; the Ed25519 attestation is an extra evidence artifact (see the sibling `compliance/erasure-attestation` concept). Do not treat absence of an attestation as a deletion gap.
- LATENT (not a live hole): the R2Cas adapter does not sweep the separate `corelink-chunk-*`/`corelink-manifest-*` multipart buckets, but the container has zero chunk-bucket write-sites today, so no tenant bytes land there. This MUST be fixed before the multipart path ships or chunked content would survive a "complete" erasure (`docs/security/2026-06-23-secreview-gdpr-residency.md:126-150`).
- The verify sweep zeroes the salt and sets `subject_id == tenant_id`: the per-DSR erasure salt is not retained post-erasure and `verify_erasure` re-fingerprints by tenant, so do not expect the salt to be reusable after the fact.

# Citations

- Verdict that erasure is complete-by-construction for the live planes, no launch-blocker: `docs/security/2026-06-23-secreview-gdpr-residency.md:12-18`.
- Centralized 410 tombstone gate at the shared CAS seam: `docs/security/2026-06-23-secreview-gdpr-residency.md:53-64`.
- Attestation cannot sign "complete" while bytes survive: `docs/security/2026-06-23-secreview-gdpr-residency.md:75-85`.
- LATENT multipart chunk-bucket erase gap: `docs/security/2026-06-23-secreview-gdpr-residency.md:126-150`.
- Self-service submit pipeline + audit-before-mutation: `crates/corelink-dsr/src/endpoint.rs:299-418`.
- MFA gate (destructive arms only): `crates/corelink-dsr/src/endpoint.rs:351-379`.
- Constant-time confidential status poll: `crates/corelink-dsr/src/endpoint.rs:420-457`.
- MFA verifier fail-CLOSED contract: `crates/corelink-dsr/src/mfa.rs:80-102`, `crates/corelink-dsr/src/mfa.rs:146-154`.
- Idempotency + divergent-payload SEV-1: `crates/corelink-dsr/src/store.rs:133-158`; store keyed `(tenant,request)`: `crates/corelink-dsr/src/store.rs:45`.
- Receipt 90d cap + SLA per jurisdiction (enforcer `sla_for`): `crates/corelink-dsr/src/receipt.rs:136-149`, `crates/corelink-dsr/src/event.rs:403`, `crates/corelink-dsr/src/lib.rs:116-119`.
- CAS per-hash erase module + tombstone-WRITE side: `crates/corelink-container/src/routes/cas_erase.rs:1-8`, `crates/corelink-container/src/routes/cas_erase.rs:70`, `crates/corelink-container/src/routes/cas_erase.rs:830-850`; read-side 410 enforcer: `crates/corelink-container/src/routes/cas.rs:665-676`.
