---
id: "ADR-0070"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-07-19"
updated: "2026-07-19"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "auth", "tenant-suspend", "offboarding", "kv", "cache", "performance", "cas-latency", "sam", "perf-99"]
---

# ADR-0070 — Tenant-Suspend Gate: L2 KV Cache with a Bounded Enforcement Window

## Status

ACTIVE (decision ratified + implemented). Tech-lead decision, South-America
auth-latency wave (finding #99), 2026-07-19. Sibling to the `pat`-read L2
(PR #858/#859, same wave) and ADR-0030 (`SLO-FRESH-PAT-REVOKE ≤ 60 s`). See
`worker/src/lib/tenant_suspend_gate.ts` and the memory note
`worker-pat-verify-cache-and-d1-locality`.

## Context

The tenant fast-suspend gate (`isTenantSuspended`, go-live GAP G4) runs inside
`extractAuth` on **every** authenticated customer CAS/AC request. It reads
`tenant_offboarding_state` (migration 0046) and denies **403 fail-closed** when
`state ∈ {suspended, erased}`.

Finding #99 isolated the South-America (São-Paulo) authenticated hot path as
**D1-over-HTTP locality**, not CPU: Cloudflare D1's primary is single-region
(`running_in_region: ENAM`) and **has no South-America region**, so each read from
the SAM edge is ~120 ms even via a read replica. `extractAuth` did **two** such
reads per request — the `pat` row AND this suspend gate. PR #858/#859 moved the
`pat` read to a globally-replicated **Workers KV** L2 (`patrow:<token_id>`, 60 s
TTL) — KV is edge-local (~3 ms) and, crucially, **per-colo cached so it survives
Cloudflare's isolate fan-out** (a per-isolate in-memory cache does not: a single
client's requests spread across many isolates, measured 0/30 hits). After #859
the suspend gate was the **remaining ~120 ms floor**: its in-memory cache is
per-isolate and therefore fan-out-defeated, so it hit D1 on essentially every SAM
request.

The suspend gate is unlike the `pat` read in one decisive way. The `pat` L2
caches **positive rows only** (a cache miss ⇒ D1, so a freshly-minted token
authenticates immediately). But for the suspend gate the **hot-path common case
is the _negative_ verdict** ("this tenant is NOT suspended"). To make the gate
edge-local we must cache that negative — and caching "not suspended" means a
tenant suspended in D1 can retain hot-path access until the cache entry expires.
That is a security-relevant trade-off and is the subject of this ADR.

Note this is not a _new_ posture: the gate's pre-existing in-memory cache already
accepted a `≤ TTL` staleness on a fresh suspend ("a newly-suspended tenant loses
access within one TTL; individual PAT revoke remains the immediate lever"). This
ADR extends that same bounded-staleness contract to the KV tier and pins the
number.

## Decision

1. **Add a Workers-KV L2 to the suspend gate, mirroring the `pat` L2.** Read
   order becomes **L1 in-memory (per-isolate) → L2 KV (`tsusp:<tenant_id>`,
   `METADATA_KV`) → L3 D1 (the `first-unconstrained` replica session, still the
   source-of-truth on every miss)**. The write-behind is handed to
   `ctx.waitUntil` (a `void kv.put(...)` is cancelled when the response returns —
   the #859 bug — so it must extend the request lifetime; a no-`waitUntil` caller
   falls back to `await`).

2. **Cache BOTH verdicts in KV** (the negative included). This is the explicit
   departure from the `pat` L2's positive-only rule and is what makes the gate
   edge-local for the not-suspended common case.

3. **The suspend-enforcement window is bounded to the KV TTL = 60 s** (Cloudflare
   KV's floor for `expirationTtl`), plus a small `≤ 5 s` per-isolate slack (see
   #4). A tenant suspended in D1 keeps customer CAS/AC access for at most this
   window before the KV entry expires and the next read re-hydrates from D1. This
   is the ratified trade-off. It is acceptable because:
   - The `suspended` offboarding arm is a **day-scale** state (T+45..T+90);
     enforcing it ~60 s later is immaterial to the offboarding SLA.
   - The **immediate hard-stop levers are unchanged**: revoking the tenant's PATs
     denies access at the `pat` layer (worker-initiated revokes also KV-delete the
     `patrow:` entry for immediacy), and the container `NativePatGate` is a second
     independent check. For an abuse response requiring a `≤ 0`-window stop, PAT
     revoke — not the offboarding state — is the operator lever.
   - The window matches the `pat` L2's own `≤ 60 s` bound (ADR-0030), so the auth
     plane has **one uniform, documented freshness window**, not two.

4. **Tighten the L1 in-memory TTL from 30 s → 5 s.** With KV now carrying the herd
   + far-D1 latency load, L1 reverts to pure micro-burst dedup within one isolate,
   matching the `pat` L1 and the container `NativePatGate` verify cache. This
   keeps the **chained** staleness bound at `≤ 60 s (KV) + ≤ 5 s (isolate) = 65 s`
   rather than compounding the old 30 s L1 onto the 60 s KV (which would be 90 s).

5. **Failure posture is preserved.** A KV fault is swallowed as a miss (falls
   through to D1 — KV never breaks auth). A D1 read error is never cached; it
   fails **OPEN for availability** (a transient D1 fault must not break active
   tenants) **EXCEPT** that a **KNOWN-suspended** cached value still denies — a
   tenant we already know is suspended is never let through on a fault.

## Consequences

- **Positive:** removes the last ~120 ms far-D1 read from the SAM authenticated
  hot path (the `pat` read already served from KV after #859); the gate now serves
  edge-local (~3 ms) for the not-suspended common case. One uniform 60 s auth
  freshness window across the `pat` and suspend layers.
- **Negative / risk:** a tenant suspended in D1 retains customer CAS/AC access for
  `≤ 65 s`. Mitigated by the unchanged immediate levers (PAT revoke; container
  gate) and by the coarse day-scale nature of the `suspended` arm. Documented and
  bounded, not silent.
- **Reversal:** removing the `kv` option from the `extractAuth` call site reverts
  to L1 + D1 with zero data migration; the KV entries self-expire in ≤ 60 s.

## Alternatives considered

- **KV deny-set (write `tsusp:<tenant>` only at suspend time; a miss ⇒ allowed
  without ever reading D1).** Rejected: a KV miss would be **unbounded** fail-open
  for any suspension applied by a path that did not write the key (a migration, an
  admin script, a direct D1 mutation). The TTL-cached-read design bounds staleness
  to 60 s **regardless of how** the D1 state was set — the only SOTA-safe choice on
  a credential-adjacent gate. (A proactive KV-delete at suspend-write time is a
  sound _future enhancement_ to make suspension immediate, layered ON TOP of the
  TTL backstop — tracked, not required for this change.)
- **Only cache the positive (suspended=true) in KV, never the negative.** Rejected:
  the hot-path common case is the negative, so this yields **no latency win** — it
  would still hit D1 on every not-suspended request (the exact floor we are
  removing).
- **Keep the 30 s L1, add KV on top.** Rejected: the chained window becomes 90 s,
  wider than the `pat` L2's 60 s and dishonest against the "one uniform window"
  goal. Tightening L1 to 5 s costs only slightly more (cheap ~3 ms) KV reads.
- **Do nothing (leave the ~120 ms floor).** Rejected: it is the last removable
  component of the #99 SAM latency and the fix mirrors an already-shipped,
  already-proven pattern.
