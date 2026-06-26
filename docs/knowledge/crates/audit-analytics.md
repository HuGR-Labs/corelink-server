---
type: "CrateCluster"
title: "Audit/analytics crate cluster"
description: "The tamper-evidence and observability layer — the CloudEvents audit taxonomy with PII-as-hash typing, the per-tenant hash chain + daily verifier, and the public Rekor transparency-log submission seam."
source_files:
  - "crates/corelink-audit/src/lib.rs"
  - "crates/corelink-audit-chain/src/lib.rs"
  - "crates/corelink-transparency-log/src/lib.rs"
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
- PII is unrepresentable as raw text: every PII-bearing field is a `*Hash` newtype whose only constructor is a one-way SHA-256-prefix-16-hex derivation, so a refactor adding a raw field is a compile error (`crates/corelink-audit/src/lib.rs:27-32`, `crates/corelink-audit/src/lib.rs:57-65`).
- `corelink-audit-chain` builds a per-tenant chain (`HashChainBuilder`: head + next_sequence, BLAKE3-256 links over JCS bytes) and a daily `ChainVerifier` that walks a slice and fails closed on the first mismatch with a SEV-0 `chain_break_detected` emit (`crates/corelink-audit-chain/src/lib.rs:33-52`).
- `corelink-transparency-log` takes an already-signed CoreLink entry, builds the canonical Rekor `hashedrekord` v0.0.1, submits it post-hoc off the write path, and records the returned inclusion proof (`crates/corelink-transparency-log/src/lib.rs:11-30`).

# Invariants

- `INV-AUDIT-NO-RAW-PII`: no public type carries raw-`String` PII — only the hash newtypes, which have no `From<String>` (`crates/corelink-audit/src/lib.rs:27-32`).
- `INV-AUDIT-CHAIN-HASH-DETERMINISTIC`: events are JCS-canonicalized before hashing, so the digest is independent of map iteration order / locale / float formatting (`crates/corelink-audit/src/lib.rs:33-38`).
- The chain processor never re-canonicalizes at link time — it reads the persisted JCS bytes / `content_hash` directly to avoid double-canonicalization drift (`crates/corelink-audit/src/lib.rs:66-70`).
- The chain verifier is fail-CLOSED and per-tenant partitioned — a cross-tenant slice is rejected at the verifier boundary and the first break aborts (`crates/corelink-audit-chain/src/lib.rs:13-20`, `crates/corelink-audit-chain/src/lib.rs:47-52`).
- The Rekor witness is fail-OPEN: a transport failure yields `WitnessOutcome::Degraded` (queued retry), never an `Err` onto a caller's hot path, because the entry is already durably logged (`crates/corelink-transparency-log/src/lib.rs:31-38`).

# Gotchas

- The asymmetry is deliberate: the *private* audit chain fails CLOSED (integrity is a gate), but the *public* Rekor witness fails OPEN (it is best-effort enrichment, never on the write path).
- CoreLink is a Rekor *submitter*, never a log operator — it does not run an append-only log or vouch for Rekor consistency; that is the public good ADR-0066 declines to rebuild.
- `corelink-audit` ships the trait + in-memory sink; the production `OutboxEmitter` (D1 batch INSERT alongside `corelink-meta::commit_*`) and SIEM fan-out land in the wiring layer, which is where `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` is finally enforced.

# Citations

1. `crates/corelink-audit/src/lib.rs:13-20` — the CloudEvents envelope, event taxonomy, and chain-hash primitives.
2. `crates/corelink-audit/src/lib.rs:27-32` — `INV-AUDIT-NO-RAW-PII`: PII typed as one-way hash newtypes.
3. `crates/corelink-audit/src/lib.rs:33-38` — `INV-AUDIT-CHAIN-HASH-DETERMINISTIC` via JCS canonicalization.
4. `crates/corelink-audit/src/lib.rs:57-65` — forbidden surface: no raw-PII fields, no `From<String>`.
5. `crates/corelink-audit/src/lib.rs:66-70` — no re-canonicalization at chain link time.
6. `crates/corelink-audit-chain/src/lib.rs:13-20` — per-tenant chain integrity + tenant-isolation at the verifier.
7. `crates/corelink-audit-chain/src/lib.rs:33-52` — `HashChainBuilder` + the fail-closed daily `ChainVerifier`.
8. `crates/corelink-transparency-log/src/lib.rs:11-30` — the Rekor submission seam (sign → hashedrekord → submit → witness).
9. `crates/corelink-transparency-log/src/lib.rs:31-38` — fail-OPEN witness: `Degraded`, never an `Err` on the hot path.
