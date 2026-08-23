---
id: "AUDIT-2026-08-23-CACHE-INTEGRITY-COVERAGE"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-23"
updated: "2026-08-23"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "cache", "integrity", "poisoning", "runner-job", "turborepo", "sccache", "action-cache"]
---

# Cache-integrity coverage across the five cache surfaces

> **Date:** 2026-08-23 · **Trigger:** a market sweep found that the loudest unmet
> need in the remote-build-cache market is TRUST, not price (Bazel closed its
> cache-poisoning issue as *not planned*; AndroidX disables remote caching on
> release CI because "the time saved is not worth the cost of potential
> failure"). That raised an internal question worth answering with evidence
> rather than assumption: how well does CoreLink actually verify what it serves?
> **Method:** read the write and read path of every surface down to the durable
> store, then check the claim against the code rather than against the shape of
> the code.

## Verdict

**Content integrity is in better shape than the internal note that prompted this
audit claimed, and the real asymmetry is somewhere else entirely.**

The prompting claim was that verification coverage is "irregular — `cas` 7,
`bazel_v2` 3, `turbo_v8` 3, `cargo` 2, `ac` 0", counted by grepping each route
file for hash-verification symbols. **That metric was measuring the wrong
thing.** Verification is not implemented per route; it is centralised one layer
below them, in `R2CasHandler::verify_content_hash`
(`crates/corelink-container/src/storage/r2_s3.rs:998`), which the native CAS
route, the Bazel REAPI bridge and sccache all funnel through. A route file with
zero mentions of BLAKE3 can be fully covered — and three of them are. Counting
symbols per file measures how the code is *organised*, not what it *enforces*.

What the read of the actual paths did surface is a different and narrower gap:
**the runner-job containment invariant is enforced on the native plane and
absent on the adapter plane**, and **Turborepo artifacts are simultaneously
unverifiable, mutable, and unpinned**. Neither is cross-tenant. Both are worth
closing.

## Coverage, as measured from the durable store upward

| Surface | Content verified on write | Re-verified on read | Key→content binding | Runner-job containment |
|---|---|---|---|---|
| Native CAS (`routes/cas.rs`) | ✅ BLAKE3 | ✅ BLAKE3 | key IS the content hash | ✅ deny-DELETE |
| Bazel REAPI v2 (`routes/bazel_v2.rs`) | ✅ SHA-256 | ✅ SHA-256 | key IS the content hash | ✅ deny-DELETE + AC key pin |
| sccache / cargo (`routes/cargo.rs`) | ✅ BLAKE3 via `MoatCache` | ✅ re-hash-on-read | client-chosen, **mutable** | ❌ **absent** |
| Action Cache (`routes/ac.rs`) | n/a by protocol | n/a by protocol | client-chosen, **immutable** | ✅ deny-DELETE + exact-key pin |
| Turborepo v8 (`routes/turbo_v8.rs`) | ❌ **impossible by protocol** | ❌ | client-chosen, **mutable** | ❌ **absent** |

Two cells marked "n/a"/"impossible" are **not defects**. The AC maps an *action
digest* to the *result* of running that action; the result's bytes do not hash to
the key, so there is nothing to re-verify — which is precisely why the AC defends
itself a different way, with create-only semantics. Turborepo's hash is opaque to
us (xxhash, sha512-prefix, or custom, chosen by the client), stated explicitly at
`crates/corelink-turbo-bridge/src/adapter.rs:5-14`: passing it as `claimed_hash`
would fail verification against bytes it was never a hash of. Calling either one
"missing verification" would be a category error.

## Findings

### F-1 — the runner-job marker never reaches the adapter plane (MEDIUM)

The `pat` table carries a runner-job marker (migration
`0086_pat_runner_job_ac_key.sql`) whose stated purpose is that "a stolen per-job
credential must not be able to EVICT the tenant's cache". The native plane
honours it: CAS DELETE is refused for a runner-job credential even when the scope
header grants write (`crates/corelink-container/src/routes/cas.rs:1507`), and AC
writes are pinned to the job's exact key (`routes/ac.rs`, via `ac_key_allowed`).

The adapter plane never sees it. `PAT_LOOKUP_SQL`
(`crates/corelink-container/src/adapter_pat.rs:174`) selects
`tenant_id, pat_hash, scope, find_only` — the 0086 columns are not read, so no
adapter surface can enforce the narrowing. Meanwhile sccache exposes a live
WebDAV DELETE, gated on cache-write scope alone
(`crates/corelink-container/src/routes/cargo.rs:407-419`), which removes the
tenant's key→content map row.

Net effect: a credential deliberately narrowed to deny-DELETE on the native plane
can delete that tenant's sccache cache entries. Not cross-tenant, and the CAS
blob survives for GC — the damage is eviction (rebuild cost, lost warm cache),
not data loss.

This is the same structural blindness ADR-0071 already identified and closed for
`find_only`: "any plane that authorizes from the D1 `scope` DIRECTLY — not the
Worker's `x-corelink-scope` header — would see `read-only` and grant read." The
same reasoning applies to the runner-job marker and was not applied to it.

### F-2 — Turborepo artifacts are unverifiable, mutable, and unpinned at once (MEDIUM)

Each property is individually defensible; together they leave the surface with no
integrity story at all.

- **Unverifiable** — by protocol, as above. Accepted.
- **Mutable** — a PUT to an existing key overwrites it; the byte-accounting code
  handles the overwrite case explicitly (`routes/turbo_v8.rs:1314-1330`), so this
  is deliberate, not accidental. Compare the AC, which is create-only precisely
  as an anti-squat defence.
- **Unpinned** — `routes/turbo_v8.rs` contains no reference to the runner-job
  marker at all (zero occurrences, against 12 in `ac.rs` and 6 in `cas.rs`).

So any credential with cache-write scope for a tenant can replace the content
behind any Turborepo cache key of that tenant, and the platform has no way to
detect that the bytes changed. A poisoned entry is served as a hit.

### F-3 — the metric that produced the wrong conclusion (process)

The "coverage is irregular, `ac` has zero" claim came from counting
hash-verification symbols per route file. Three of the five surfaces are covered
by a shared gate one layer down, and two of the five cannot be covered at all for
protocol reasons. The metric therefore mismeasured every single surface: it
under-reported three and mislabelled two correct designs as gaps. Recorded here
because the claim had already reached a PR body and a memory file before it was
checked.

## What is NOT wrong

- The `_public` cross-tenant path is **not** client-writable. `_public` rows are
  written only by the read-through mirror routes (npm/oci/brew/pip) from bytes
  fetched upstream, and `adapter_cache.rs` re-hashes on read and treats a
  mismatch as a miss so the entry self-heals. Cross-tenant poisoning would
  require an upstream compromise, which is a different threat model.
- CAS overwrite is harmless by construction: the key is the content hash and the
  write gate verifies it, so an "overwrite" can only rewrite identical bytes.
- The AC's create-only rule already blocks the obvious AC-poisoning move.

## Recommendations, in cost order

1. **Add the 0086 columns to `PAT_LOOKUP_SQL`, then narrow DELETE on the sccache
   surface for a runner-job credential** (F-1) — but NOT as a blanket refusal.
   ⚠️ **A blanket deny would break the live runner fleet.** sccache's own
   write-check does `PUT .sccache_check` → read back → `DELETE`
   (`crates/corelink-container/src/routes/cargo.rs:399-406`), and the runner
   dogfood path is exactly sccache-over-CoreLink with a runner-minted PAT. Deny
   that DELETE unconditionally and every runner box fails its startup write-check
   against a cache we control. The containment must therefore permit the
   write-check sentinel and refuse everything else, and the behaviour of a real
   sccache client against a 403 on that DELETE must be observed before the change
   ships — not assumed. Needs a regression test on the adapter plane, like
   `find_only_pat_is_rejected_on_the_adapter_plane`, plus one asserting the
   sentinel still round-trips.
2. **Give Turborepo an integrity envelope** (F-2): record CoreLink's own
   BLAKE3 of the stored bytes at write time and verify it on read. This cannot
   detect a client that poisons its own cache — nothing can, the key is opaque —
   but it does detect at-rest corruption and tampering below our API, which is
   currently undetectable on this surface alone.
3. **Decide whether Turborepo PUT should be create-only**, matching the AC. This
   is a product decision, not a bug fix: Turborepo clients may legitimately
   re-PUT, and refusing an overwrite could break them. It should not be changed
   without checking real client behaviour.

## Residual risk if nothing is done

Confined to a single tenant, and to credentials that tenant issued. The realistic
scenario is a leaked or misused per-job runner credential evicting or poisoning
that tenant's own build cache: slower builds and, in the Turborepo case,
potentially wrong build outputs served as cache hits. No cross-tenant exposure
was found on any of the five surfaces.
