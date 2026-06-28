---
type: "CrateCluster"
title: "Audit/analytics crate cluster"
description: "The tamper-evidence and observability layer — the CloudEvents audit taxonomy with PII-as-hash typing, the per-tenant hash chain + daily verifier, and the public Rekor transparency-log submission seam (post-hoc witness deferred)."
source_files:
  - "crates/corelink-audit/src/lib.rs"
  - "crates/corelink-audit/src/link_hash.rs"
  - "crates/corelink-audit/src/redact.rs"
  - "crates/corelink-audit-chain/src/lib.rs"
  - "crates/corelink-audit-chain/src/chain.rs"
  - "crates/corelink-audit-chain/src/verifier.rs"
  - "crates/corelink-transparency-log/src/lib.rs"
  - "crates/corelink-transparency-log/src/entry.rs"
  - "crates/corelink-transparency-log/src/submit.rs"
checkpoint_sha: "30ec21dc78d79c85f4d7e1e19e13118c66025c9e"
provenance: "AUTHORED"
tags: ["crates", "audit", "analytics", "transparency", "cloudevents", "observability"]
timestamp: "2026-06-26T00:00:00Z"
---

# Audit/analytics crate cluster

Every security-relevant action in CoreLink must leave a tamper-evident trace, and this cluster is the machinery that guarantees the trace is canonical, PII-free, and chained so a deletion or edit is detectable. It is grouped around one design spine: events are serialized via RFC 8785 JCS before hashing so the digest is deterministic, then linked without ever re-canonicalizing. `corelink-audit` owns the event taxonomy, the PII-as-hash type system, and the SHA-256 chain-hash primitives (`compute_content_hash` / `link_chain_hash` in `crates/corelink-audit/src/link_hash.rs:156`, `crates/corelink-audit/src/link_hash.rs:171`); `corelink-audit-chain` builds and daily-verifies the per-tenant hash chain (`HashChainBuilder` / `ChainVerifier`, `crates/corelink-audit-chain/src/chain.rs:230`, `crates/corelink-audit-chain/src/verifier.rs:73`); `corelink-transparency-log` is the seam that witnesses a signed entry to the public sigstore/Rekor log.

# Role

The cluster underpins the [RFC-6962 audit / transparency chain](/compliance/audit-chain.md) and the [audit/analytics export plane](/ops/audit-analytics-plane.md). It is the producer side of every `auth.*`, `cas:*`, `gc:*`, and billing audit event, the integrity verifier that proves the chain is unbroken, and the optional public-witness submitter that lets a relying party verify *against* CoreLink rather than trusting it.

# How it works

- `corelink-audit` ships the CloudEvents 1.0 `AuthEvent` envelope, the 33-variant `AuthEventType`, and the executed chain-hash primitives `compute_content_hash` (JCS → SHA-256, `crates/corelink-audit/src/link_hash.rs:156`) + `link_chain_hash` (`sha256(prev || content_hash)`, never re-canonicalize, `crates/corelink-audit/src/link_hash.rs:171`).
- PII is unrepresentable as raw text: every PII-bearing field is a `*Hash` newtype whose only constructor is a one-way SHA-256-prefix-16-hex `derive` (no `From<String>`), so a refactor adding a raw field is a compile error (`crates/corelink-audit/src/redact.rs:54-61`, `crates/corelink-audit/src/redact.rs:81-90`, `crates/corelink-audit/src/redact.rs:117`).
- `corelink-audit-chain` builds a per-tenant chain (`HashChainBuilder`: head + next_sequence, links over JCS bytes, `crates/corelink-audit-chain/src/chain.rs:230-235`; the link check `verify_chain_link`, `crates/corelink-audit-chain/src/chain.rs:206`) and a daily `ChainVerifier` that walks a slice and fails closed on the first mismatch with a SEV-0 `chain_break_detected` emit (`crates/corelink-audit-chain/src/verifier.rs:73`).
- `corelink-transparency-log` takes an already-signed CoreLink entry, builds the canonical Rekor `hashedrekord` (`SignedEntry::to_rekor_hashedrekord`, `crates/corelink-transparency-log/src/entry.rs:71`), submits it post-hoc off the write path (`crates/corelink-transparency-log/src/submit.rs:35-50`), and folds transport faults into a `Degraded` outcome (`crates/corelink-transparency-log/src/submit.rs:87`). NOTE: this crate is the SUBMISSION SEAM — the live OutboxEmitter producer wiring + the actual Rekor network witness are deferred (see Gotchas).

# Invariants

- `INV-AUDIT-NO-RAW-PII`: no public type carries raw-`String` PII — only the hash newtypes, whose only constructor is the one-way `derive` (`crates/corelink-audit/src/redact.rs:54-61`); there is no `From<String>`.
- `INV-AUDIT-CHAIN-HASH-DETERMINISTIC`: events are JCS-canonicalized before hashing, so the digest is independent of map iteration order / locale / float formatting — `compute_content_hash` runs `serde_jcs::to_vec` then SHA-256 (`crates/corelink-audit/src/link_hash.rs:156-160`).
- The chain processor never re-canonicalizes at link time — `link_chain_hash` reads the persisted hex `content_hash` / `prev_chain_hash` directly and byte-concats, avoiding double-canonicalization drift (`crates/corelink-audit/src/link_hash.rs:171-180`).
- The chain verifier is fail-CLOSED and per-tenant partitioned — a cross-tenant slice is rejected at the verifier boundary and the first break aborts (`crates/corelink-audit-chain/src/verifier.rs:73`, `crates/corelink-audit-chain/src/chain.rs:206`).
- The Rekor witness is fail-OPEN: a transport failure yields `WitnessOutcome::Degraded` (queued retry), never an `Err` onto a caller's hot path, because the entry is already durably logged (`crates/corelink-transparency-log/src/submit.rs:48-53`, `crates/corelink-transparency-log/src/submit.rs:106-117`).

# Gotchas

- The asymmetry is deliberate: the *private* audit chain fails CLOSED (integrity is a gate), but the *public* Rekor witness fails OPEN (it is best-effort enrichment, never on the write path).
- CoreLink is a Rekor *submitter*, never a log operator — it does not run an append-only log or vouch for Rekor consistency; that is the public good ADR-0066 declines to rebuild. The transparency-log crate is the submission SEAM + in-memory fake (`InMemoryRekor`); the live network Rekor submission is deferred.
- `corelink-audit` ships the trait + in-memory sink; the production `OutboxEmitter` (D1 batch INSERT alongside `corelink-meta::commit_*`) and SIEM fan-out land in the wiring layer (DEFERRED per the crate's `# Architectural split` doc-comment), which is where `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` is finally enforced.

# Citations

1. `crates/corelink-audit/src/link_hash.rs:156` — the executed `compute_content_hash` (JCS → SHA-256); the `crates/corelink-audit/src/lib.rs:13-20` mention is the module doc-comment summary.
2. `crates/corelink-audit/src/link_hash.rs:171` — the executed `link_chain_hash` (`sha256(prev || content)`, no re-canonicalize).
3. `crates/corelink-audit/src/redact.rs:54-61` — `INV-AUDIT-NO-RAW-PII`: `PrincipalIdHash::derive`, the one-way constructor; no `From<String>`.
4. `crates/corelink-audit/src/redact.rs:81-90` — `PatIdHash::derive`; `crates/corelink-audit/src/redact.rs:117` — `EmailHash`.
5. `crates/corelink-audit-chain/src/chain.rs:230` — `HashChainBuilder` (head + next_sequence, per-tenant link builder).
6. `crates/corelink-audit-chain/src/chain.rs:206` — `verify_chain_link`, the per-link integrity check.
7. `crates/corelink-audit-chain/src/verifier.rs:73` — the fail-closed daily `ChainVerifier` (per-tenant partitioned, SEV-0 `chain_break_detected`).
8. `crates/corelink-transparency-log/src/entry.rs:71` — `SignedEntry::to_rekor_hashedrekord`, the canonical Rekor entry builder.
9. `crates/corelink-transparency-log/src/submit.rs:87` — `witness_or_degrade`; `crates/corelink-transparency-log/src/submit.rs:48` — the `WitnessOutcome::Degraded` fail-OPEN arm (never an `Err` on the hot path).
10. `crates/corelink-audit/src/lib.rs:13-30` — the crate's `# Architectural split` doc-comment (taxonomy summary + the DEFERRED `OutboxEmitter` / SIEM producer wiring); the executed primitives it names live in `link_hash.rs` (cites 1-2).
11. `crates/corelink-audit-chain/src/lib.rs:1-20` — the audit-chain crate doc-comment (per-tenant chain rationale); the executed builder/verifier live in `chain.rs` / `verifier.rs` (cites 5-7).
12. `crates/corelink-transparency-log/src/lib.rs:91` — the crate's public re-export surface (`RekorSubmitter`, `WitnessOutcome`, `witness_or_degrade`, `InMemoryRekor`); the executed submission path lives in `entry.rs` / `submit.rs` (cites 8-9).
