---
id: "ADR-0069"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-19"
updated: "2026-06-19"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "auth", "pat", "argon2id", "hash", "performance", "cas-latency", "wp4", "deferred"]
---

# ADR-0069 — PAT Verification: Fast Keyed Hash, not Argon2id (high-entropy tokens)

## Status

ACTIVE (decision ratified) — implementation **DEFERRED post-WP-2** with rationale below.
Tech-lead decision, CAS hot-path latency wave (WP-4), 2026-06-19. See
`docs/perf/2026-06-19-cas-hot-path-latency.md` and ADR-0068 (the $-ceiling, a sibling
hot-path gate).

## Context

CoreLink stores each Personal Access Token's random secret as an **Argon2id** hash
(`adapter_pat.rs`, `native_pat_gate.rs`) and re-verifies it with Argon2id on the data
plane. Argon2id is a **memory-hard password-hashing** function: its cost (m=64 MiB, t=3,
p=4) exists to make brute-force of **low-entropy human passwords** expensive.

A CoreLink PAT is **NOT a password** — it is a server-minted, high-entropy random token
(≥256 bits). Brute-forcing a 256-bit random secret is infeasible regardless of the hash
speed, so the memory-hardness buys **no security** for this input class — it only buys
**latency** (tens of ms native; pathological in CPU-constrained contexts). This is a
**primitive/threat-model mismatch**: memory-hard hashing is the right tool for passwords,
the wrong tool for high-entropy machine credentials. Industry practice (GitHub et al.)
stores high-entropy PATs under a fast hash (SHA-256/HMAC) for exactly this reason.

The CAS-latency investigation (WP-2) showed Argon2id is **not** the dominant CAS cost (the
two D1-over-HTTP hops were — fixed in WP-2a/WP-2b); on the warm path the PAT-gate
verify-cache skips Argon2id entirely. Argon2id only bites on a **verify-cache miss**
(cold token / TTL expiry / new isolate).

## Decision

1. **Verify high-entropy PATs with a fast keyed hash** (HMAC-SHA256 / SHA-256 of the stored
   secret), not Argon2id. The existing HMAC-possession gate + the stored-secret second
   factor (defense-in-depth, red-team finding #4) are preserved — only the *primitive* for
   the stored-secret check changes from memory-hard to fast-keyed, which is sound because
   the input is high-entropy.
2. **Migrate lazily / dual-read — no big-bang.** Verification accepts BOTH the existing
   Argon2id `pat_hash` AND the new fast-hash format (format-tagged). On a successful
   Argon2id verify, opportunistically re-write the row with the fast hash; new mints write
   the fast hash from the start. No destructive migration, no flag-day, additive only
   (INV-AUTH-MIGRATION-ADDITIVE).
3. **DEFER implementation to post-WP-2.** Rationale: (a) after WP-2a/WP-2b remove the D1
   hops, Argon2id only affects pat-gate cache misses — the urgency is gone; (b) a
   stored-hash-format change on the launch-critical auth plane is the wrong risk to take
   pre-launch under time pressure; (c) it should be measured first (residual cache-miss
   Argon2id cost post-WP-2) to confirm it is worth the migration at all. This is a
   **documented, decided deferral, not debt** — the position is ratified here; the
   implementation is scheduled, with the lazy/dual-read design fixed above so it is a
   transcription when it is built.

## Consequences

- **Positive:** removes seconds-class (CPU-constrained) / tens-of-ms (native) verify cost
  on the cache-miss path; right primitive for the input class; no security loss.
- **Negative / risk:** an auth-plane stored-hash migration must be done carefully (dual-read
  window, format tag, additive migration + tests). Until implemented, the cache-miss
  Argon2id cost remains (bounded, secondary post-WP-2).
- **Reversal:** the dual-read accepts Argon2id indefinitely, so the change is reversible by
  reverting the mint/re-write side; no data loss.

## Alternatives considered

- **Keep Argon2id, just raise the verify-cache TTL** — cheaper interim, but only masks the
  cost (still pays on cold/new tokens) and does not fix the primitive mismatch. Acceptable
  as a stop-gap; not the end state.
- **Big-bang re-hash migration** — rejected: needs every PAT re-minted or a bulk re-hash;
  flag-day risk on the auth plane; the lazy dual-read achieves the same with no flag-day.
