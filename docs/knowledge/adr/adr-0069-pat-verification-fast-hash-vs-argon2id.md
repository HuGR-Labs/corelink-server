---
type: "ADR"
title: "ADR-0069 — PAT verification: fast keyed hash, not Argon2id (high-entropy tokens)"
description: "Why high-entropy PATs should be verified with a fast keyed hash rather than memory-hard Argon2id, and why the migration is a documented deferral with a lazy dual-read design."
source_files:
  - "specs/03_architecture/adrs/ADR-0069-pat-verification-fast-hash-vs-argon2id.md"
  - "crates/corelink-container/src/native_pat_gate.rs"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "auth", "pat", "argon2id", "hash", "performance", "deferred"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0069 — PAT verification: fast keyed hash, not Argon2id (high-entropy tokens)

CoreLink stored each PAT's random secret as an Argon2id hash and re-verified it with Argon2id on the
data plane — but a CoreLink PAT is not a password. It is a server-minted, high-entropy (≥256-bit)
random token, so Argon2id's memory-hardness (the cost that makes brute-forcing low-entropy human
passwords expensive) buys *no security* for this input class and only buys *latency*. This ADR records
the decision to verify high-entropy PATs with a fast keyed hash, and — equally important — to **defer**
the implementation as a documented, ratified position rather than debt, with the lazy dual-read
migration fixed in advance. It sits in the CAS hot-path latency wave (WP-4) alongside ADR-0068.

# Context

Argon2id is the right tool for low-entropy passwords and the wrong tool for high-entropy machine
credentials — a primitive/threat-model mismatch; industry practice (GitHub et al.) stores high-entropy
PATs under a fast hash for exactly this reason. The CAS-latency investigation (WP-2) further showed
Argon2id is not the dominant CAS cost (two D1-over-HTTP hops were, fixed in WP-2a/WP-2b), and the warm
PAT-gate verify-cache skips Argon2id entirely — it only bites on a verify-cache miss (cold token, TTL
expiry, or a new isolate).

# Decision

1. **Verify high-entropy PATs with a fast keyed hash** (HMAC-SHA256 / SHA-256 of the stored secret),
   not Argon2id; the existing HMAC-possession gate and the stored-secret second factor are preserved —
   only the stored-secret check's *primitive* changes from memory-hard to fast-keyed, sound because the
   input is high-entropy.
2. **Migrate lazily / dual-read — no big-bang.** Verification accepts BOTH the existing Argon2id hash
   and the new format-tagged fast hash; a successful Argon2id verify opportunistically re-writes the
   row, new mints write the fast hash, and the migration is additive (no flag-day, no destructive
   migration).
3. **Defer implementation to post-WP-2** because after the D1 hops were removed the urgency is gone, an
   auth-plane stored-hash-format change is the wrong risk pre-launch under time pressure, and it should
   be measured first. This is a decided deferral, not debt — the position is ratified and the design is
   fixed so the build is a transcription.

# Consequences

- Positive: removes the cache-miss verify cost (seconds-class in CPU-constrained contexts, tens-of-ms
  native) and uses the right primitive for the input class with no security loss.
- Negative/risk: an auth-plane stored-hash migration must be careful (dual-read window, format tag,
  additive migration + tests); until implemented the bounded cache-miss Argon2id cost remains.
- Reversal: the dual-read accepts Argon2id indefinitely, so the change is reversible by reverting the
  mint/re-write side with no data loss. A rejected stop-gap was simply raising the verify-cache TTL,
  which only masks the cost and does not fix the primitive mismatch.

# Status vs shipped code

To avoid reading this ADR as "Argon2id was removed": the fast keyed hash is the **fast-reject** layer, and
Argon2id remains the **possession backstop**. The shipped native PAT gate verifies the HMAC fast path
first, then — on a verify-cache miss, at the top of each billable handler, after scope+tenant and before
storage — runs the Argon2id stored-secret check
(`crates/corelink-container/src/native_pat_gate.rs:194`, `:140-194`; warm cache hits skip it). They are
DIFFERENT layers, not a swap: this ADR changes only which primitive the cold stored-secret check uses for
high-entropy tokens. The decision and the lazy dual-read migration stand unchanged.

# Citations

1. `specs/03_architecture/adrs/ADR-0069-pat-verification-fast-hash-vs-argon2id.md:26-44` — the Context:
   PATs are high-entropy not passwords, the primitive mismatch, and Argon2id only biting on cache miss.
2. `specs/03_architecture/adrs/ADR-0069-pat-verification-fast-hash-vs-argon2id.md:46-65` — the
   Decision: fast keyed hash + lazy dual-read additive migration + the documented post-WP-2 deferral.
3. `specs/03_architecture/adrs/ADR-0069-pat-verification-fast-hash-vs-argon2id.md:67-75` — the
   Consequences: the latency win, the migration risk, and the indefinite-dual-read reversibility.
4. `specs/03_architecture/adrs/ADR-0069-pat-verification-fast-hash-vs-argon2id.md:79-81` — the
   Alternatives-considered: a rejected stop-gap was simply raising the verify-cache TTL (masks cost, not the mismatch).
