---
type: "ADR"
title: "ADR-0033 — Audit events: CloudEvents 1.0 + JCS hash chain + PII newtypes"
description: "Why auth audit events use a CloudEvents 1.0 envelope, an atomic outbox, type-system hash newtypes for PII, an RFC-8785 JCS content hash chain, SEV-1 dual fan-out, and per-tenant retention hints."
source_files:
  - "specs/03_architecture/adrs/ADR-0033-audit-events-cloudevents.md"
  - "crates/corelink-audit-chain/src/chain.rs"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "audit", "cloudevents", "jcs", "hash-chain", "pii", "s03", "s09"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0033 — Audit events: CloudEvents 1.0 + JCS hash chain + PII newtypes

Every authentication operation must emit an audit event that the S-09 chain processor seals into a tamper-evident hash chain — with zero raw PII, byte-deterministic hashing, and no possibility of a handler succeeding without an audit row. This ADR records the six decisions (envelope, ordering, PII surface, hash compute, SEV-1 fan-out, retention) that make those guarantees structural rather than best-effort.

# Context

Five load-bearing invariants constrain the surface: no raw PII in the chain, audit-emit atomic with the handler mutation, a byte-deterministic content hash across producers/platforms/serde versions, exhaustive event-type→payload mapping, and an accurate per-tenant retention hint (ADR-0033:31-48).

# Decision

Events use a CloudEvents 1.0 envelope (SIEM-native, JCS-canonicalizable) written through an outbox in the *same* D1 batch as the handler mutation so a 200 without an audit row is structurally impossible; PII rides as one-way hash newtypes (`PrincipalIdHash`, `PatIdHash`, …) with no `From<String>` constructor so raw PII in canonical bytes is a compile error rather than a runtime-filter gap (ADR-0033:64-148). The content hash is `SHA-256(JCS-canonicalize(event))` per RFC 8785, and the chain processor reads the *persisted* JCS bytes off the row rather than re-canonicalizing (avoiding `serde_jcs` version drift corrupting every link); six SEV-1 event types additionally fan out to a direct SIEM webhook for sub-second visibility while the outbox stays canonical; and every event carries a tier-derived retention hint so the retention worker honors per-tenant promises in one SQL pass (ADR-0033:149-230).

# Consequences

The design yields forensic completeness, compile-time PII safety, a drift-proof deterministic chain, SIEM-native parsing, and tenant-differentiated retention, traded against a 33-event-type maintenance surface, a load-bearing pinned `serde_jcs` dependency, SIEM-webhook coupling on SEV-1 (non-blocking), a 64-bit hash-prefix collision bound, and a deferred production emitter shim (ADR-0033:246-285).

# Status vs shipped code

One clarification on the hash primitive: the ADR describes the content hash as `SHA-256(JCS-canonicalize(event))`,
but the shipped per-tenant **chain-link** hash is **BLAKE3-256** over the JCS-canonical bytes
(`crates/corelink-audit-chain/src/chain.rs:156-160`, streaming variant `:188-191`). SHA-256 survives only
in the Rekor/transparency `hashedrekord` path, not in the per-tenant audit chain. The decision (atomic
outbox, hash-newtype PII redaction, read-persisted-JCS-not-recanonicalize, deterministic chain) is
unchanged — only the named chain digest reflects the shipped BLAKE3.

# Citations

1. `specs/03_architecture/adrs/ADR-0033-audit-events-cloudevents.md:31-48` — the five audit load-bearing invariants (Context).
2. `specs/03_architecture/adrs/ADR-0033-audit-events-cloudevents.md:64-148` — CloudEvents envelope, atomic outbox, hash-newtype PII redaction (Decision).
3. `specs/03_architecture/adrs/ADR-0033-audit-events-cloudevents.md:149-230` — RFC-8785 JCS content/chain hash, SEV-1 dual fan-out, per-tenant retention hint (Decision).
4. `specs/03_architecture/adrs/ADR-0033-audit-events-cloudevents.md:246-285` — positive guarantees vs maintenance/dependency/coupling trade-offs (Consequences).
