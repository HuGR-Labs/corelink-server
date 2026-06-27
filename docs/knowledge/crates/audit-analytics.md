---
type: "CrateCluster"
title: "Audit/analytics crate cluster"
description: "The tamper-evidence and observability layer — the CloudEvents audit taxonomy with PII-as-hash typing, the per-tenant hash chain + daily verifier, and the public Rekor transparency-log submission seam."
source_files:
  - "crates/corelink-audit/src/lib.rs"
  - "crates/corelink-audit/src/redact.rs"
  - "crates/corelink-audit/src/link_hash.rs"
  - "crates/corelink-audit-chain/src/lib.rs"
  - "crates/corelink-audit-chain/src/chain.rs"
  - "crates/corelink-audit-chain/src/verifier.rs"
  - "crates/corelink-transparency-log/src/lib.rs"
  - "crates/corelink-transparency-log/src/submit.rs"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["crates", "audit", "analytics", "transparency", "cloudevents", "observability"]
timestamp: "2026-06-26T00:00:00Z"
---

# Audit/analytics crate cluster

Every security-relevant action in CoreLink must leave a tamper-evident trace, and this cluster is the machinery that guarantees the trace is canonical, PII-free, and chained so a deletion or edit is detectable. It is grouped around one design spine: events are serialized via RFC 8785 JCS before hashing so the digest is deterministic, then linked without ever re-canonicalizing. The two chain crates use *different* link-hash algorithms, though: `corelink-audit` links via SHA-256 (`sha256(prev_chain_hash || content_hash)`, `crates/corelink-audit/src/lib.rs:20`), while `corelink-audit-chain` links via BLAKE3 (`crates/corelink-audit-chain/src/lib.rs:31`, `crates/corelink-audit-chain/src/lib.rs:36`). `corelink-audit` owns the event taxonomy and the PII-as-hash type system; `corelink-audit-chain` builds and daily-verifies the per-tenant hash chain; `corelink-transparency-log` is the seam that witnesses a signed entry to the public sigstore/Rekor log.

# Role

The cluster underpins the [RFC-6962 audit / transparency chain](/compliance/audit-chain.md) and the [audit/analytics export plane](/ops/audit-analytics-plane.md). It is the producer side of every `auth.*`, `cas:*`, `gc:*`, and billing audit event, the integrity verifier that proves the chain is unbroken, and the optional public-witness submitter that lets a relying party verify *against* CoreLink rather than trusting it.

# How it works

- `corelink-audit` ships the CloudEvents 1.0 `AuthEvent` envelope, the 33-variant `AuthEventType`, and the chain-hash primitives `compute_content_hash` (JCS → SHA-256) + `link_chain_hash` (`sha256(prev || content_hash)`, never re-canonicalize) (`crates/corelink-audit/src/lib.rs:13-20`).
- PII is unrepresentable as raw text: every PII-bearing field is a `*Hash` newtype (`PrincipalIdHash`/`PatIdHash`/`EmailHash`) whose only constructor is the one-way SHA-256-prefix-16-hex `derive`, with no public `From<String>`/`new(&str)`, so a refactor adding a raw field is a compile error (`crates/corelink-audit/src/redact.rs:50-62`).
- `corelink-audit-chain` builds a per-tenant chain — `link_chain_hash_streaming` = `BLAKE3(prev || JCS(event))` (`crates/corelink-audit-chain/src/chain.rs:152-162`) and `HashChainBuilder::append` enforces the sequence + `prev_hash` link before advancing the head (`crates/corelink-audit-chain/src/chain.rs:285-303`) — plus a daily `ChainVerifier::verify_chain` that walks a slice and fails closed on the first mismatch with a SEV-0 `chain_break_detected` emit (`crates/corelink-audit-chain/src/verifier.rs:126-191`).
- `corelink-transparency-log` takes an already-signed CoreLink entry, builds the canonical Rekor `hashedrekord` v0.0.1, submits it post-hoc off the write path, and records the returned inclusion proof (`crates/corelink-transparency-log/src/lib.rs:11-30`).

# Invariants

- `INV-AUDIT-NO-RAW-PII`: no public type carries raw-`String` PII — only the hash newtypes, whose sole constructor is the SHA-256-prefix derivation and which have no `From<String>` (`crates/corelink-audit/src/redact.rs:50-62`).
- `INV-AUDIT-CHAIN-HASH-DETERMINISTIC`: `compute_content_hash` JCS-canonicalizes (`serde_jcs`) then SHA-256s, so the digest is independent of map iteration order / locale / float formatting (`crates/corelink-audit/src/link_hash.rs:156-160`).
- The chain processor never re-canonicalizes at link time — `link_chain_hash` takes the already-computed `content_hash` and only concatenates `SHA-256(prev_chain_hash || content_hash)`, never re-running JCS on the event (`crates/corelink-audit/src/link_hash.rs:171-176`).
- The chain verifier is fail-CLOSED and per-tenant partitioned — a cross-tenant slice is rejected at the tenant-isolation guard and the first break aborts after a constant-time `prev_hash` compare (`crates/corelink-audit-chain/src/verifier.rs:140-156`, `crates/corelink-audit-chain/src/verifier.rs:167-191`).
- The Rekor witness is fail-OPEN: `witness_or_degrade` folds a transient transport fault into `WitnessOutcome::Degraded` (queued retry), never an `Err` onto a caller's hot path, because the entry is already durably logged (`crates/corelink-transparency-log/src/submit.rs:87-107`).

# Gotchas

- The asymmetry is deliberate: the *private* audit chain fails CLOSED (integrity is a gate), but the *public* Rekor witness fails OPEN (it is best-effort enrichment, never on the write path).
- CoreLink is a Rekor *submitter*, never a log operator — it does not run an append-only log or vouch for Rekor consistency; that is the public good ADR-0066 declines to rebuild.
- `corelink-audit` ships the trait + in-memory sink; the production `OutboxEmitter` (D1 batch INSERT alongside `corelink-meta::commit_*`) and SIEM fan-out land in the wiring layer, which is where `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` is finally enforced.

# Citations

1. `crates/corelink-audit/src/lib.rs:13-20` — the CloudEvents envelope, event taxonomy, and chain-hash primitives.
2. `crates/corelink-audit/src/redact.rs:50-62` — `INV-AUDIT-NO-RAW-PII` enforcer: the `*Hash` newtype + `derive`-only constructor (no `From<String>`).
3. `crates/corelink-audit/src/link_hash.rs:156-160` — `INV-AUDIT-CHAIN-HASH-DETERMINISTIC` enforcer: `compute_content_hash` = SHA-256(JCS(event)).
4. `crates/corelink-audit/src/lib.rs:57-65` — forbidden surface: no raw-PII fields, no `From<String>`.
5. `crates/corelink-audit/src/link_hash.rs:171-176` — `link_chain_hash`: consumes the precomputed `content_hash` and concatenates only, never re-canonicalizing at link time.
6. `crates/corelink-audit-chain/src/chain.rs:152-162` — `link_chain_hash_streaming` = `BLAKE3(prev || JCS(event))`.
7. `crates/corelink-audit-chain/src/chain.rs:285-303` — `HashChainBuilder::append`: sequence + `prev_hash` enforcement before head advance.
8. `crates/corelink-audit-chain/src/verifier.rs:140-156` — `ChainVerifier` tenant-isolation + sequence-monotonicity guards.
9. `crates/corelink-audit-chain/src/verifier.rs:167-191` — fail-CLOSED constant-time `prev_hash` break + link recompute.
10. `crates/corelink-transparency-log/src/lib.rs:11-30` — the Rekor submission seam (sign → hashedrekord → submit → witness).
11. `crates/corelink-transparency-log/src/submit.rs:87-107` — `witness_or_degrade`: fail-OPEN witness — transient transport fault → `Degraded`, never an `Err` on the hot path.
