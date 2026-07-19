---
type: "ADR"
title: "ADR-0070 — Tenant-suspend gate: L2 KV cache with a bounded enforcement window"
description: "Why the tenant fast-suspend gate gained a Workers-KV L2 that caches even the NEGATIVE verdict, trading a bounded ≤60s+≤5s suspend-enforcement window for an edge-local read on the South-America hot path."
source_files:
  - "specs/03_architecture/adrs/ADR-0070-tenant-suspend-gate-kv-l2-cache.md"
checkpoint_sha: "d10fe2653e889406ec3ae528f130233e05843eea"
provenance: "AUTHORED"
tags: ["adr", "auth", "tenant-suspend", "offboarding", "kv", "cache", "performance", "sam"]
timestamp: "2026-07-19T00:00:00Z"
---

# ADR-0070 — Tenant-suspend gate: L2 KV cache with a bounded enforcement window

The tenant fast-suspend gate (`isTenantSuspended`, go-live GAP G4) runs inside `extractAuth` on **every**
authenticated customer CAS/AC request, reading `tenant_offboarding_state` (migration 0046) and denying
**403 fail-closed** when `state ∈ {suspended, erased}`. This ADR records the decision to front that read
with a globally-replicated **Workers-KV L2** that — unlike the sibling `pat` L2 — caches even the
**negative** ("not suspended") verdict, accepting a bounded, documented suspend-enforcement window in
exchange for removing the last far-D1 read from the South-America authenticated hot path. It is the
sibling of the `pat`-read L2 (PR #858/#859, same #99 latency wave) and reuses ADR-0030's 60 s freshness
bound.

# Context

Finding #99 isolated the São-Paulo authenticated hot path as **D1-over-HTTP locality**, not CPU:
Cloudflare D1's primary is single-region (`running_in_region: ENAM`) with **no South-America region**, so
each read from the SAM edge is ~120 ms even via a read replica — and `extractAuth` did **two** such reads
per request, the `pat` row AND this suspend gate. After #859 moved the `pat` read to KV, the suspend gate
was the remaining ~120 ms floor: its in-memory cache is per-isolate and therefore defeated by Cloudflare's
isolate fan-out (a single client's requests spread across many isolates), so it hit D1 on essentially
every SAM request. The gate differs from the `pat` read in one decisive way: the `pat` L2 caches positive
rows only (a miss ⇒ D1, so a fresh mint authenticates immediately), but the suspend gate's hot-path common
case is the **negative** verdict — to make the gate edge-local that negative must be cached, and caching
"not suspended" means a tenant suspended in D1 can retain access until the entry expires. That is a
security-relevant trade-off, and the pre-existing in-memory cache already accepted a `≤ TTL` staleness on
a fresh suspend; this ADR extends that same bounded-staleness contract to the KV tier and pins the number.

# Decision

1. **Add a Workers-KV L2 mirroring the `pat` L2.** Read order becomes **L1 in-memory (per-isolate) → L2
   KV (`tsusp:<tenant_id>` on `METADATA_KV`) → L3 D1** (the `first-unconstrained` replica session, still
   the source-of-truth on every miss); the write-behind is handed to `ctx.waitUntil` (a bare
   `void kv.put(...)` is cancelled when the response returns — the #859 bug — and a no-`waitUntil` caller
   falls back to `await`).
2. **Cache BOTH verdicts in KV**, the negative included — the explicit departure from the `pat` L2's
   positive-only rule and what makes the gate edge-local for the not-suspended common case.
3. **Bound the suspend-enforcement window to the KV TTL = 60 s** (KV's `expirationTtl` floor) plus a
   `≤ 5 s` per-isolate slack. Acceptable because the `suspended` offboarding arm is day-scale (T+45..T+90);
   the immediate hard-stop levers are unchanged — PAT revoke (worker-initiated revokes also KV-delete the
   `patrow:` entry) and the container `NativePatGate` — and the window matches the `pat` L2's own ≤ 60 s
   ADR-0030 bound, giving the auth plane one uniform freshness window.
4. **Tighten the L1 in-memory TTL from 30 s → 5 s** so L1 reverts to pure micro-burst dedup, matching the
   `pat` L1; the chained bound is then ≤ 65 s rather than a compounded 90 s.
5. **Preserve the failure posture:** a KV fault is swallowed as a miss (falls through to D1 — KV never
   breaks auth); a D1 read error is never cached and fails **OPEN for availability** EXCEPT that a
   KNOWN-suspended cached value still denies.

# Consequences

- Positive: removes the last ~120 ms far-D1 read from the SAM authenticated hot path; the gate now serves
  edge-local (~3 ms) for the not-suspended common case, under one uniform 60 s auth freshness window.
- Negative/risk: a tenant suspended in D1 retains customer CAS/AC access for ≤ 65 s — mitigated by the
  unchanged immediate levers (PAT revoke; container gate) and the coarse day-scale `suspended` arm;
  documented and bounded, not silent.
- Reversal: removing the `kv` option from the `extractAuth` call site reverts to L1 + D1 with no data
  migration; the KV entries self-expire in ≤ 60 s. A rejected alternative — a KV *deny-set* (a miss ⇒
  allowed without reading D1) — was unbounded fail-open for any suspension applied by a path that did not
  write the key; the TTL-cached-read design bounds staleness regardless of how the D1 state was set.

# Citations

1. `specs/03_architecture/adrs/ADR-0070-tenant-suspend-gate-kv-l2-cache.md:29-59` — the Context: the #99
   SAM D1-over-HTTP locality floor, isolate fan-out defeating the per-isolate cache, and why the gate must
   cache the NEGATIVE verdict (unlike the positive-only `pat` L2).
2. `specs/03_architecture/adrs/ADR-0070-tenant-suspend-gate-kv-l2-cache.md:63-100` — the Decision: the KV
   L2 (`tsusp:<tenant_id>`, `waitUntil` write-behind), caching both verdicts, the ≤ 60 s + ≤ 5 s bounded
   enforcement window, the L1 30 s → 5 s tightening, and the preserved fail-OPEN-except-known-suspend posture.
3. `specs/03_architecture/adrs/ADR-0070-tenant-suspend-gate-kv-l2-cache.md:104-113` — the Consequences: the
   removed ~120 ms SAM floor, the bounded ≤ 65 s suspend window (with unchanged immediate levers), and the
   zero-migration reversal.
4. `specs/03_architecture/adrs/ADR-0070-tenant-suspend-gate-kv-l2-cache.md:117-134` — the
   Alternatives-considered: the rejected KV deny-set (unbounded fail-open), positive-only caching (no
   latency win), keeping the 30 s L1 (90 s chained window), and do-nothing.
</content>
</invoke>
