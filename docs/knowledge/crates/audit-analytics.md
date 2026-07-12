---
type: "CrateCluster"
title: "Audit/analytics crate cluster"
description: "The tamper-EVIDENCE (detect-at-verify) and observability layer — the CloudEvents audit taxonomy with PII-as-hash typing, the LIVE per-tenant BLAKE3 hash chain (sealed by the hourly audit-drain) + daily verifier, and the public Rekor transparency-log submission seam (post-hoc witness deferred). NOTE: corelink-audit's SHA-256 link primitives are dead/legacy — the live chain-hash is corelink-audit-chain's BLAKE3."
source_files:
  - "crates/corelink-audit/src/lib.rs"
  - "crates/corelink-audit/src/link_hash.rs"
  - "crates/corelink-audit/src/redact.rs"
  - "crates/corelink-audit-chain/src/lib.rs"
  - "crates/corelink-audit-chain/src/chain.rs"
  - "crates/corelink-audit-chain/src/verifier.rs"
  - "crates/corelink-container/src/routes/audit_drain.rs"
  - "crates/corelink-transparency-log/src/lib.rs"
  - "crates/corelink-transparency-log/src/entry.rs"
  - "crates/corelink-transparency-log/src/submit.rs"
checkpoint_sha: "d2a1f643464c2bd4636cd7fb62f17d3843c621ee"
provenance: "AUTHORED"
tags: ["crates", "audit", "analytics", "transparency", "cloudevents", "observability"]
timestamp: "2026-06-26T00:00:00Z"
---

# Audit/analytics crate cluster

Every security-relevant action in CoreLink must leave a tamper-EVIDENT (detect-at-verify) trace, and this cluster is the machinery that makes a deletion or edit of a SEALED row detectable by a verifier holding an independent prior head. It is grouped around one design spine: events are serialized via RFC 8785 JCS before hashing so the digest is deterministic, then linked without ever re-canonicalizing. **The LIVE chain-hash is `corelink-audit-chain`'s BLAKE3, NOT `corelink-audit`'s SHA-256.** `corelink-audit-chain` owns the executed link primitive — `link_chain_hash_from_canonical` = un-keyed `BLAKE3(prev || JCS(payload))` (`crates/corelink-audit-chain/src/chain.rs:183`) — and the wired producer is the hourly audit-drain (`crates/corelink-container/src/routes/audit_drain.rs:426-453`, mounted in `main.rs`), which seals pending `audit_outbox` rows into the per-tenant chain in D1; the daily `ChainVerifier` (`crates/corelink-audit-chain/src/verifier.rs:73`) walks it. `corelink-audit` owns the event taxonomy + the PII-as-hash type system, but its SHA-256 `compute_content_hash` / `link_chain_hash` (`crates/corelink-audit/src/link_hash.rs:156`, `crates/corelink-audit/src/link_hash.rs:171`) are DEAD/LEGACY — they have ZERO non-test callers (only tests/examples/doc-comments reference them) and are NOT the live chain-hash; do not present them as such. `corelink-transparency-log` is the seam that witnesses a signed entry to the public sigstore/Rekor log.

Because the live per-link BLAKE3 link is UN-KEYED, the chain BODY is tamper-EVIDENCE, not tamper-PROOF against an insider: anyone holding the rows can recompute a self-consistent chain body — but the chain HEAD checkpoint is now Ed25519-SIGNED (CF-6), so a forged head fails verification on the next drain resume (fail-closed SEV-1) and an insider lacks the signing seed. The pre-seal `audit_outbox` window + the still-UNWIRED R2 Object-Lock + the un-keyed per-link body still leave gaps; the strongest cross-check is still an INDEPENDENT pinned head (or the Rekor witness when wired). See [audit-chain](/compliance/audit-chain.md) for the full honest-scope note.

# Role

The cluster underpins the [RFC-6962 audit / transparency chain](/compliance/audit-chain.md) and the [audit/analytics export plane](/ops/audit-analytics-plane.md). It is the producer side of every `auth.*`, `cas:*`, `gc:*`, and billing audit event, the integrity verifier that proves the chain is unbroken, and the optional public-witness submitter that lets a relying party verify *against* CoreLink rather than trusting it.

# How it works

- `corelink-audit` ships the CloudEvents 1.0 `AuthEvent` envelope, the 33-variant `AuthEventType`, and SHA-256 chain-hash primitives `compute_content_hash` (JCS → SHA-256, `crates/corelink-audit/src/link_hash.rs:156`) + `link_chain_hash` (`sha256(prev || content_hash)`, never re-canonicalize, `crates/corelink-audit/src/link_hash.rs:171`) — but these are DEAD/LEGACY (zero non-test callers; a `grep` for `compute_content_hash`/`link_chain_hash` outside `link_hash.rs` resolves only to tests, examples, doc-comments, and the unrelated `corelink-billing-aggregator` BLAKE3 chain). The live audit chain-hash is BLAKE3 in `corelink-audit-chain` (next bullets), NOT this SHA-256.
- **The live chain producer is the hourly audit-drain** `crates/corelink-container/src/routes/audit_drain.rs:426-453` (mounted in `main.rs`): it seals pending `audit_outbox` rows (written PLAIN/UNCHAINED first, `emitted_at IS NULL`) into the per-tenant BLAKE3 chain in D1 via `link_chain_hash_from_canonical` `crates/corelink-audit-chain/src/chain.rs:183`, advancing `audit_chain_head` under a single-writer compare-and-set `crates/corelink-container/src/routes/audit_drain.rs:790-900`. The ≤1h pre-seal window is mutable with no chain evidence, and R2 Object-Lock is unwired — so this is tamper-EVIDENCE, not proof-against-insider. **CF-6:** each checkpoint advance now Ed25519-SIGNS the canonical head tuple `(tenant_id, region, head_hash, next_sequence)` and the drain re-verifies it on resume — a non-verifying / unseeded signed head is TAMPER → fail-CLOSED SEV-1 `crates/corelink-container/src/routes/audit_drain.rs:502-560` (migration 0080 adds `head_signature` / `head_signed_at_ms` / `signing_key_id`), so the chain HEAD is tamper-evident against a D1-writer even though the per-link BLAKE3 body stays un-keyed.
- PII is unrepresentable as raw text: every PII-bearing field is a `*Hash` newtype whose only constructor is a one-way SHA-256-prefix-16-hex `derive` (no `From<String>`), so a refactor adding a raw field is a compile error (`crates/corelink-audit/src/redact.rs:54-61`, `crates/corelink-audit/src/redact.rs:81-90`, `crates/corelink-audit/src/redact.rs:117`).
- `corelink-audit-chain` builds a per-tenant chain (`HashChainBuilder`: head + next_sequence, links over JCS bytes, `crates/corelink-audit-chain/src/chain.rs:230-235`; the link check `verify_chain_link`, `crates/corelink-audit-chain/src/chain.rs:206`) and a daily `ChainVerifier` that walks a slice and fails closed on the first mismatch with a SEV-0 `chain_break_detected` emit (`crates/corelink-audit-chain/src/verifier.rs:73`).
- `corelink-transparency-log` takes an already-signed CoreLink entry, builds the canonical Rekor `hashedrekord` (`SignedEntry::to_rekor_hashedrekord`, `crates/corelink-transparency-log/src/entry.rs:71`), submits it post-hoc off the write path (`crates/corelink-transparency-log/src/submit.rs:35-50`), and folds transport faults into a `Degraded` outcome (`crates/corelink-transparency-log/src/submit.rs:87`). NOTE: this crate is the SUBMISSION SEAM — the actual Rekor network witness is deferred (in-memory fake) (see Gotchas). (The per-tenant CHAIN producer is separately LIVE via the audit-drain, above; what is deferred here is the public Rekor witness + the corelink-audit `OutboxEmitter`/SIEM path.)

# Invariants

- `INV-AUDIT-NO-RAW-PII`: no public type carries raw-`String` PII — only the hash newtypes, whose only constructor is the one-way `derive` (`crates/corelink-audit/src/redact.rs:54-61`); there is no `From<String>`.
- `INV-AUDIT-CHAIN-HASH-DETERMINISTIC`: events are JCS-canonicalized before hashing, so the digest is independent of map iteration order / locale / float formatting. The LIVE form is `corelink-audit-chain`'s BLAKE3: the drain JCS-canonicalizes the payload, then `link_chain_hash_from_canonical` hashes those exact bytes (`crates/corelink-audit-chain/src/chain.rs:183-193`). (`corelink-audit`'s SHA-256 `compute_content_hash` `crates/corelink-audit/src/link_hash.rs:156-160` has the same determinism property but is DEAD — not the live chain-hash.)
- The chain processor never re-canonicalizes at link time — the live drain/verifier read the persisted JCS bytes and re-hash directly via `link_chain_hash_from_canonical`, avoiding double-canonicalization drift (`crates/corelink-audit-chain/src/chain.rs:183-193`). (`corelink-audit`'s SHA-256 `link_chain_hash` `crates/corelink-audit/src/link_hash.rs:171-180` is the dead/legacy analogue.)
- The live per-link BLAKE3 link is UN-KEYED (`Hasher::new()`, no `keyed_hash`/HMAC — `crates/corelink-audit-chain/src/chain.rs:156`), so the chain BODY is tamper-EVIDENCE (detect-at-verify vs an independent head), NOT tamper-proof against an insider who can recompute the whole chain body — though the chain HEAD is now Ed25519-SIGNED (CF-6, `crates/corelink-container/src/routes/audit_drain.rs:213-271`), so a forged head fails verification on resume (fail-closed SEV-1); R2 Object-Lock + the pre-seal `audit_outbox` window still leave storage mutable.
- The chain verifier is fail-CLOSED and per-tenant partitioned — a cross-tenant slice is rejected at the verifier boundary and the first break aborts (`crates/corelink-audit-chain/src/verifier.rs:73`, `crates/corelink-audit-chain/src/chain.rs:206`).
- The Rekor witness is fail-OPEN: a transport failure yields `WitnessOutcome::Degraded` (queued retry), never an `Err` onto a caller's hot path, because the entry is already durably logged (`crates/corelink-transparency-log/src/submit.rs:48-53`, `crates/corelink-transparency-log/src/submit.rs:106-117`).

# Gotchas

- The asymmetry is deliberate: the *private* audit chain fails CLOSED (integrity is a gate), but the *public* Rekor witness fails OPEN (it is best-effort enrichment, never on the write path).
- CoreLink is a Rekor *submitter*, never a log operator — it does not run an append-only log or vouch for Rekor consistency; that is the public good ADR-0066 declines to rebuild. The transparency-log crate is the submission SEAM + in-memory fake (`InMemoryRekor`); the live network Rekor submission is deferred.
- `corelink-audit` ships the trait + in-memory sink; the production `OutboxEmitter` (D1 batch INSERT alongside `corelink-meta::commit_*`) and SIEM fan-out land in the wiring layer (DEFERRED per the crate's `# Architectural split` doc-comment), which is where `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` is finally enforced.
- **`corelink-audit`'s SHA-256 link primitives are DEAD/LEGACY.** `compute_content_hash` / `link_chain_hash` (`crates/corelink-audit/src/link_hash.rs:156`, `:171`) have ZERO non-test callers — the live chain producer (the audit-drain) calls `corelink-audit-chain`'s un-keyed BLAKE3 `link_chain_hash_from_canonical`, not these. Two different hash families exist by historical accident, not by design here; do NOT cite the SHA-256 primitives as the live chain-hash, and do not "unify" them.

# Citations

1. `crates/corelink-audit/src/link_hash.rs:156` — `compute_content_hash` (JCS → SHA-256). DEAD/LEGACY: zero non-test callers; NOT the live chain-hash. The `crates/corelink-audit/src/lib.rs:13-20` mention is the module doc-comment summary.
2. `crates/corelink-audit/src/link_hash.rs:171` — `link_chain_hash` (`sha256(prev || content)`). DEAD/LEGACY: zero non-test callers; NOT the live chain-hash.
2b. `crates/corelink-audit-chain/src/chain.rs:183` — `link_chain_hash_from_canonical` = un-keyed BLAKE3, the LIVE link primitive the drain + verifier call.
2c. `crates/corelink-audit-chain/src/chain.rs:156` — plain `Hasher::new()` (un-keyed; basis of evidence-not-proof).
2d. `crates/corelink-container/src/routes/audit_drain.rs:426-453` — `seal_rows`, the LIVE BLAKE3 chain producer (`BLAKE3(prev || JCS(payload))`).
2e. `crates/corelink-container/src/routes/audit_drain.rs:790-900` — `drain_partition`: seal to D1 + single-writer CAS advance of `audit_chain_head` (no R2; ≤1h pre-seal window mutable) + CF-6 head sign-on-advance / verify-on-resume.
2f. `crates/corelink-container/src/routes/audit_drain.rs:213-271` — CF-6 keyed head: `canonical_head_bytes` / `sign_head` / `verify_head` (Ed25519 over the JCS head tuple).
2g. `crates/corelink-container/src/routes/audit_drain.rs:502-560` — `check_head_on_resume`: re-verify the resumed head signature; a non-verifying / unseeded signed head = TAMPER → fail-CLOSED (SEV-1).
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
