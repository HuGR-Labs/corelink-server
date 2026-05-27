# `cas-worker` middleware shim

Phase 0.G seeded `src/middleware/analytics.ts` here so that the CAS data plane
has a single emit point for the three TTFV-anchoring events from PLG §7.1:

- `first_cli_authed` — first authenticated `/v1/ping` per tenant
- `first_cas_write` — first cache PUT per tenant
- `first_cache_hit` — first cache HIT per tenant (the §3.2 canonical event)

These three are **deduplicated per tenant** (only the very first ever fires).
Engagement events (`cache_hits_10` / `100` / `1k`) are a nightly batch over
the cas_events log and live in `apps/analytics-worker` not here.

## Coordination with Wave-33 Stream B

Per Phase 0 execution plan §10, this app overlaps with Wave-33 Stream B's
`crates/cas-worker/` refactor. The middleware contract is intentionally
narrow so Stream B can move the file location or rewrite the surrounding
worker without touching the contract:

```ts
emitFirstHit({ env, tenantId, patId, contentHash, latencyMs, cfColo })
```

If Stream B has already moved the file, the import path adapts but the
function signature does not.

## Integration

Wire from whichever handler serves `GET /v1/cache/{key}`:

```ts
import { emitCacheHit } from "./middleware/analytics";

const start = Date.now();
const obj = await env.CAS_BUCKET.get(key);
if (obj) {
  ctx.waitUntil(emitCacheHit({
    env,
    tenantId,
    patId,
    contentHash: key,
    latencyMs: Date.now() - start,
    cfColo: request.cf?.colo ?? "unknown",
  }));
  return new Response(obj.body, { headers: { "Content-Type": "application/octet-stream" } });
}
```
