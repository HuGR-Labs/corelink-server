---
type: "ComplianceControl"
title: "RFC-6962 audit / transparency chain"
description: "CoreLink's append-only BLAKE3 hash-chained audit log plus its post-hoc sigstore/Rekor public-witness submission seam."
source_files:
  - docs/internal/auth-event-taxonomy.md
  - crates/corelink-audit-chain/src/chain.rs
  - crates/corelink-audit-chain/src/event.rs
  - crates/corelink-audit-chain/src/verifier.rs
  - crates/corelink-audit-chain/src/lib.rs
  - crates/corelink-transparency-log/src/entry.rs
  - crates/corelink-transparency-log/src/submit.rs
  - crates/corelink-transparency-log/src/lib.rs
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: [compliance, audit, transparency, hash-chain, blake3, rekor, sigstore, cloudevents, tamper-evidence, soc2]
timestamp: "2026-06-26T00:00:00Z"
---

CoreLink needs an audit log that a customer (and an auditor) can trust even if CoreLink itself is compromised. It is **designed** around two layers (the chain logic is live + property-tested; the R2/Object-Lock persistence, the scheduled cron, and SIEM fan-out are deferred to the WI-S09-007 ship gate — see Gotchas): a per-tenant BLAKE3 hash chain that makes any retroactive tamper detectable at verify time (the RFC-6962-style Merkle/transparency discipline applied to CloudEvents audit events), and an optional public witness that submits a signed chain/attestation digest to the sigstore/Rekor transparency log so anyone can re-verify it without trusting CoreLink. The chain is the tamper-evidence; Rekor is the third-party notarization. This control underpins the SOC 2 CC7.2 audit-chain-integrity discipline and the 7-year append-only retention story.

# Role

Provide tamper-evident, append-only auditing for security-relevant events (auth, CAS/AC, GC, quota, abuse, tenant lifecycle). The hash chain gives internal, CoreLink-verifiable integrity; the Rekor submission seam gives an external, anyone-can-check public witness. Together they answer "has the audit trail been altered after the fact?" — the chain answers it cryptographically per-tenant, Rekor answers it without requiring trust in CoreLink at all.

# How it works

- Each audit event is a canonical CloudEvents 1.0 envelope (`specversion`/`type`/`source`/`subject`/`id`/`time_ms`/`datacontenttype`/`data` plus CoreLink extensions `tenant_id`/`region`/`sequence_number`/`prev_hash`), the same envelope the auth taxonomy emits into the outbox `docs/internal/auth-event-taxonomy.md:9-13` and materialized as the `AuditEvent` struct `crates/corelink-audit-chain/src/event.rs:308-369`.
- The chain link is `next_hash = BLAKE3(prev_hash_bytes || canonical_bytes(event))`, where the event is canonicalized with RFC 8785 JCS and streamed straight into the hasher `crates/corelink-audit-chain/src/chain.rs:152-162`, with canonical bytes computed via `serde_jcs::to_vec` `crates/corelink-audit-chain/src/chain.rs:97-99`.
- The 32-byte digest is a `ChainHash` newtype that serializes to lowercase hex on the wire `crates/corelink-audit-chain/src/event.rs:204-229`, and the genesis event uses the Bitcoin-style zero convention `prev_hash = [0u8; 32]` / `sequence_number = 0` `crates/corelink-audit-chain/src/event.rs:79-82`.
- `HashChainBuilder::append` enforces the link at write time: it rejects a wrong `sequence_number` and rejects an event whose `prev_hash` does not match the current chain head before advancing `crates/corelink-audit-chain/src/chain.rs:285-303`.
- The verifier walks an in-order `[start, end]` slice, recomputes every link with `link_chain_hash`, and advances the running head per event `crates/corelink-audit-chain/src/verifier.rs:186-191` (the *daily* cadence is the DO scheduler cron deferred to the WI-S09-007 ship gate — not yet in the deployed container — not this verify logic), comparing each event's claimed `prev_hash` to the recomputed head in constant time via `subtle::ConstantTimeEq` `crates/corelink-audit-chain/src/verifier.rs:167-185`.
- On the first divergence the verifier fails CLOSED — it emits the `chain_break_detected` meta-audit BEFORE returning the `ChainBreak` error carrying the break sequence `crates/corelink-audit-chain/src/verifier.rs:174-184`, and reports the result (count, last good hash, optional break seq) in a `VerifyOutcome` `crates/corelink-audit-chain/src/verifier.rs:53-63`.
- The verifier also guards tenant isolation (every event's `tenant_id` must match the chain's) and sequence monotonicity before checking the link `crates/corelink-audit-chain/src/verifier.rs:140-156`.
- For the public witness, an already-signed payload is wrapped as a `SignedEntry` (canonical payload bytes + Ed25519 signature + PEM public key) `crates/corelink-transparency-log/src/entry.rs:28-41` and only its SHA-256 digest is computed `crates/corelink-transparency-log/src/entry.rs:63-66`.
- `to_rekor_hashedrekord` builds the canonical Rekor `hashedrekord` v0.0.1 proposed entry — submitting `{content_digest, signature, public_key}` so the raw payload bytes never leave CoreLink `crates/corelink-transparency-log/src/entry.rs:71-91`.
- `witness_or_degrade` drives the `RekorSubmitter` seam `crates/corelink-transparency-log/src/submit.rs:24-39` post-hoc and fails OPEN: a transient transport fault degrades to `WitnessOutcome::Degraded` for out-of-band retry rather than erroring `crates/corelink-transparency-log/src/submit.rs:101-107`, and even a deterministic Rekor rejection degrades instead of coupling to the caller's write path `crates/corelink-transparency-log/src/submit.rs:108-118`.

# Invariants

- INV-OBS-AUDIT-CHAIN-INTEGRITY: the per-tenant hash chain is unbroken and the verifier asserts every link's recomputed hash matches the claimed hash, failing closed on the first divergence `crates/corelink-audit-chain/src/verifier.rs:167-191`.
- INV-AUDIT-APPEND-ONLY: 7-year append-only is an **infrastructure** control — R2 Object Lock Governance Mode deferred to the WI-S09-007 ship gate, not yet wired (there is no code enforcer for it in this crate); the chain is what catches any post-hoc tampering at verify time `crates/corelink-audit-chain/src/lib.rs:89-92`.
- Genesis is the zero-hash position by construction: `prev_hash == [0u8; 32]` and `sequence_number == 0`, detected by `is_genesis` `crates/corelink-audit-chain/src/event.rs:439-445`.
- Sequence numbers are strictly monotonic increasing by 1; the builder advances with `saturating_add(1)` only after both link checks pass `crates/corelink-audit-chain/src/chain.rs:299-302`.
- Tamper detection surfaces at the first divergent sequence and is fail-CLOSED: the verifier returns `ChainBreak` after emitting the SEV-0 meta-audit `crates/corelink-audit-chain/src/verifier.rs:181-184`.
- Hash comparison in the verifier is constant-time to avoid a timing oracle over which/where bytes differ `crates/corelink-audit-chain/src/verifier.rs:167-173`.
- The audit data payload must be already-redacted of raw PII before reaching chain emit — the chain layer is purely structural and does NOT re-redact `crates/corelink-audit-chain/src/event.rs:46-57`.
- The Rekor witness is best-effort enrichment, never a gate: the entry is already durable before submission and a Rekor outage must not block the write path `crates/corelink-transparency-log/src/lib.rs:31-38`.
- CoreLink is a submitter only — it does not operate an append-only log or gossip checkpoints; that public good is delegated to sigstore/Rekor `crates/corelink-transparency-log/src/lib.rs:26-29`.

# Gotchas

- The chain link input includes the `prev_hash` slot itself (Bitcoin block-header pattern), so flipping a single bit of `prev_hash` changes the recomputed link too — but it also means producer and verifier MUST agree byte-for-byte on JCS canonical form; canonical-form drift (e.g. a `serde_jcs` bump) would silently corrupt every link. The pin is `serde_jcs = "0.2"` plus canonical-vector regression tests.
- The verifier never re-canonicalizes off the wire archive; it reads the persisted JCS NDJSON bytes and recomputes BLAKE3 directly, so the persisted bytes are load-bearing.
- This crate ships the pure-logic skeleton (in-memory fakes); the real CF R2 PutObject with Object Lock, the scheduled DO verifier cron (UTC 02:00), and SIEM fan-out are deferred to the WI-S09-007 ship gate (not yet in the deployed build) — do not assume the live R2/Object-Lock binding exists just because the chain logic does.
- BLAKE3 here is the per-tenant chain digest; the Rekor `hashedrekord` separately uses SHA-256 over the canonical payload (that is Rekor's required scheme), so two different hash families coexist by design — don't "unify" them.
- The SEV-1 auth events fan out directly to SIEM (not just the 60-s outbox drain); the chain/transparency layer is downstream of that fan-out and is not the incident-response fast path.

# Citations

- `docs/internal/auth-event-taxonomy.md:9-13` — CloudEvents envelope emitted atomically into the outbox; the S-09 chain processor seals events into the chain.
- `docs/internal/auth-event-taxonomy.md:115-128` — the exactly-6 SEV-1 direct-to-SIEM fan-out set.
- `crates/corelink-audit-chain/src/chain.rs:97-99` — `compute_canonical_bytes` (RFC 8785 JCS).
- `crates/corelink-audit-chain/src/chain.rs:152-162` — `link_chain_hash_streaming` = `BLAKE3(prev || canonical_bytes)`.
- `crates/corelink-audit-chain/src/chain.rs:285-303` — `HashChainBuilder::append` sequence + `prev_hash` enforcement and head advance.
- `crates/corelink-audit-chain/src/chain.rs:299-302` — monotonic `saturating_add(1)` advance.
- `crates/corelink-audit-chain/src/event.rs:79-82` — genesis zero-hash / zero-sequence constants.
- `crates/corelink-audit-chain/src/event.rs:204-229` — `ChainHash` newtype, lowercase-hex wire form.
- `crates/corelink-audit-chain/src/event.rs:308-369` — the `AuditEvent` CloudEvents 1.0 struct + chain-link fields.
- `crates/corelink-audit-chain/src/event.rs:439-445` — `is_genesis` predicate.
- `crates/corelink-audit-chain/src/event.rs:46-57` — already-redacted payload requirement (no re-redaction at chain emit).
- `crates/corelink-audit-chain/src/verifier.rs:140-156` — tenant-isolation + sequence-monotonicity guards.
- `crates/corelink-audit-chain/src/verifier.rs:167-185` — constant-time `prev_hash` compare + fail-closed break emit.
- `crates/corelink-audit-chain/src/verifier.rs:181-184` — `ChainBreak` returned after meta-audit (fail-closed).
- `crates/corelink-audit-chain/src/verifier.rs:186-191` — link recompute + running-head advance.
- `crates/corelink-audit-chain/src/verifier.rs:53-63` — `VerifyOutcome` shape.
- `crates/corelink-audit-chain/src/verifier.rs:167-191` — INV-OBS-AUDIT-CHAIN-INTEGRITY enforced by the verify walk (fail-closed on first divergence).
- `crates/corelink-audit-chain/src/lib.rs:89-92` — INV-AUDIT-APPEND-ONLY (R2 Object Lock 7y — infra, wired WI-S09-007).
- `crates/corelink-transparency-log/src/entry.rs:28-41` — `SignedEntry` (payload + signature + PEM key).
- `crates/corelink-transparency-log/src/entry.rs:63-66` — SHA-256 content digest.
- `crates/corelink-transparency-log/src/entry.rs:71-91` — canonical Rekor `hashedrekord` v0.0.1 builder.
- `crates/corelink-transparency-log/src/submit.rs:24-39` — `RekorSubmitter` async seam.
- `crates/corelink-transparency-log/src/submit.rs:101-107` — fail-OPEN transient degrade.
- `crates/corelink-transparency-log/src/submit.rs:108-118` — deterministic rejection also degrades (protects write path).
- `crates/corelink-transparency-log/src/lib.rs:26-29` — submitter, not log operator.
- `crates/corelink-transparency-log/src/lib.rs:31-38` — fail-OPEN witness, entry already durable (ADR-0066).
