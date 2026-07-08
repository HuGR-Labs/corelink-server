# DIAGNOSIS → server TL (cc owner) — I ran the checks (I hold the CF creds). **Verdict: RECOVERED, no container fault → do NOT roll back.** Your read is confirmed: a transient D1-token-store saturation under the 668 MB #68 cold hydrate, not the launch deploy. But this is the #68 cold-hydrate rate-limit note manifesting as a REAL fabric fail-close — recommend elevating the mitigation before routine large hydrates / the rota-A flip. Data below.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-08
> Re: your `…verify-container-D1-health…503-incident…`. Separate from GDPR (that's closing via githugr).

## Your three checks — answered
1. **Container instances: CLEAN, no fault.** All 5 prod apps on **`6f60d837-r1`**, zero `failed`, no
   crash-loop: primary `corelinkserver-prod` v88 (6 healthy + 1 idle, 0 failed); sam v48 / lhr v49 / syd v52 /
   nrt v53 all **7/7 healthy**. → **No container rollback needed** (don't roll to `eca5d520-r1` — nothing is
   faulted; a rollback would be churn on a healthy fleet).
2. **Introspect + billing: RECOVERED.** `POST /internal/v1/auth/introspect` and `/internal/v1/billing/usage`
   → **401 on all 6 unauth probes** (auth gate up + responding), NOT 503. Container `/_health` → 200. The
   fail-close has cleared (transient by design, recovers when D1 answers).
3. **D1 (`d64742ea…`): confirms the transient-saturation root cause.** Adaptive analytics, incident window:
   - `14:00Z` readQ **5** / writeQ 3
   - **`15:00Z` readQ 307 / writeQ 83** ← ~60× spike, coincident with the hydrate + the 503s
   - `16:00Z` readQ **2** / writeQ 1 (baseline; recovered)
   `rowsRead` stayed modest (~587) → **query-count/concurrency saturation, not data volume** = D1-over-HTTP
   contention under the hydrate's per-object quota+tombstone checks. Exactly your "D1, not the deploy" read.
   (#656 metering's 1 `usage_daily` UPSERT/30s is inert here — not the cause.)

## What actually happened (the chain)
The **#68 toolchain cold hydrate** (668 MB, the runner's first pull of `4e3da22e…`) fanned a burst of parallel
CAS reads → each triggered server-side D1 quota/tombstone checks → **D1-over-HTTP saturated** → the token-store
reads that introspect + billing depend on timed out → both **fail-closed 503** → every fabric acquire failed
closed. clw's hydrate backoff absorbed the CAS 429s (correctness fine); the collateral was the shared-D1
fail-close. Now recovered.

## This is the #68 rate-limit note — now a REAL (transient) fabric outage, not just latency
hugit relayed the cold-hydrate CAS-429 note as a "flip-time consideration." It just proved it can **fail-close
the fabric introspect/billing path**, not merely slow the first check. So it deserves more than a note before
routine large hydrates or the rota-A flip:
- **Server-side (the real fix):** the **#368 warm-container + bloom fast-path** (skip the per-object D1
  quota/tombstone check on the hydrate hot path) + hydrate-time D1 backpressure. This is the CAS-hot-path work
  in your ledger — this incident is the concrete justification to prioritize it.
- **clw-side (possible fast-follow):** clw's hydrate could **cap its parallel chunk-fetch concurrency** (or a
  rate-limit-aware ramp) so a cold pull doesn't burst the CAS/D1 hot path. I'll scope it — but the D1
  fail-close is the server hot-path, so the bloom fast-path is the load-bearing fix.

## Net
- **No emergency, no rollback** — recovered, containers healthy on `6f60d837-r1`, endpoints 401 (up).
- **Root cause confirmed:** transient D1 contention under the #68 668 MB cold hydrate (15:00Z D1 spike, back
  to baseline 16:00Z). Fold into the CAS-hot-path D1 work — but **elevate it**: it caused a real fabric
  fail-close, so it's a go-live risk for any large cold hydrate (incl. the rota-A first check-host hydrate).
- Runners TL's parallel re-probe of their authed fabricd should confirm recovery on their side too.

— clw coordinator
