---
type: "Surface"
title: "DevEnv edge surface"
description: "How a /v1/customer/devenv request is authenticated (Clerk OR PAT), quota-gated against the tenant's entitlement, stripped of client-supplied trust headers, and forwarded to a Durable Object owned by another Worker."
source_files:
  - "worker/src/index.ts"
  - "worker/src/lib/devenv_guard.ts"
  - "worker/src/lib/openapi_devenv.ts"
  - "wrangler.toml"
checkpoint_sha: "07bb61ee80779947e6bff8cdc01f895414088fc9"
provenance: "AUTHORED"
tags: ["surfaces", "devenv", "worker-edge", "auth", "quota", "durable-object"]
timestamp: "2026-08-30T00:00:00Z"
---

# DevEnv edge surface

`/v1/customer/devenv*` is the customer-facing entry to persistent cloud development environments. It is the only CoreLink surface whose Durable Object is **owned by a different Worker**, and the only one that accepts a browser session (Clerk) and a machine token (PAT) on the same path. Both properties change what the edge has to do before anything reaches the DO.

# Context

`matchRoute` classifies the path as the `devenv_v1` route kind, resolving the tenant later than other surfaces because the tenant is not in the path — it comes from whichever credential authenticated (`worker/src/index.ts:981-984`). The OpenAPI 3.1 document for the surface is served separately at `GET /openapi.json` from a static module, so publishing the contract costs no D1 or DO hop (`worker/src/index.ts:2059-2060`, document at `worker/src/lib/openapi_devenv.ts:4`).

The DO binding is **not declared today.** It was a cross-Worker reference — an explicit `script_name` pointing at `corelink-spawn-worker`, which owns the class — and #1447 removed it because the spawn-worker does not export `RunnerDevEnvDO` and could not itself deploy, which blocked EVERY prod deploy behind it. The comment explaining the binding survives where the block was (`wrangler.toml:250-251`). The surface is built for the binding to be absent: the `Env` type marks it optional (`worker/src/index.ts:164`) and the handler returns 503 rather than throwing when it is missing (`worker/src/index.ts:2995-2997`).

# Decision

**Dual credential, single path, decided by shape.** The handler tries `parsePat` first and branches on the RESULT, not on a header or a flag: a value that does not parse as a PAT is treated as a Clerk session and verified as one, and only a parseable PAT takes the PAT path (`worker/src/index.ts:2954-2984`). A viewer role is downgraded to `read-only` scope at the edge, so the DO never has to know Clerk's role vocabulary.

**Quota is checked at the edge, before the DO is touched.** `checkDevenvQuota` reads the tenant's `runners_entitlement` row and refuses an absent or `_anonymous` tenant outright (`worker/src/lib/devenv_guard.ts:16-30`). Doing it here rather than inside the DO keeps a quota-exceeded request from spinning up per-tenant DO state at all.

**Client-supplied trust headers are stripped before forwarding, and the authorization header is dropped entirely.** The edge rebuilds the header set, calls `stripClientTrustHeaders`, deletes `authorization`, and then sets the trust headers itself — tenant id, scope, role, token prefix, request id (`worker/src/index.ts:3005-3018`). The DO therefore cannot be told who the caller is by the caller; the credential does not travel past the boundary that verified it.

**When the binding returns, it must be mirrored into `env.prod`.** `durable_objects` is non-inheritable in wrangler: an env block replaces the top-level list rather than extending it, so a binding declared only at top level is absent in prod. The comment recording that rule outlived the binding it governed (`wrangler.toml:566-568`) — worth keeping, because shipping only the top-level binding makes DevEnv green in dev and a permanent 503 in prod.

# Consequences

The cross-Worker binding means DevEnv availability depends on `corelink-spawn-worker` being deployed with the class. This concept previously recorded that a `script_name` pointing at a Worker that does not export `RunnerDevEnvDO` "fails at request time, not at deploy time" — that prediction was WRONG, and the correction is worth more than the original claim. It failed at DEPLOY time, for the whole Worker, and took 43 unrelated commits down with it (see B-110). A cross-Worker binding is not a runtime dependency that degrades; it is a deploy-time dependency that blocks. The mirrored prod binding is what keeps that failure from being prod-only — the omission that would otherwise ship green in dev and 503 in prod while the customer nav links straight at it.

Quota state lives in `runners_entitlement`, shared with the runner fabric, so a DevEnv launch and a runner spawn draw on the same tenant ceiling. The monthly vCPU aggregate is `devenv_monthly_vcpu` (migration 0106), registered in the DSR erase set as ERASE per ADR-S11-013 — a pre-invoice usage aggregate, not the fiscal record.

# Citations

1. `worker/src/index.ts:981-984` — `matchRoute` classifies `/v1/customer/devenv*` and `/v1/devenv*` as `devenv_v1`, deferring tenant resolution to the credential.
2. `worker/src/index.ts:2954-2984` — dual Clerk/PAT auth decided by whether `parsePat` returns null; viewer role downgraded to `read-only`.
3. `worker/src/lib/devenv_guard.ts:16-30` — `checkDevenvQuota` refuses an absent/`_anonymous` tenant and reads `runners_entitlement` for the ceiling.
4. `worker/src/index.ts:3005-3018` — `stripClientTrustHeaders`, `authorization` deleted, and the trust headers set by the edge rather than accepted from the client.
5. `worker/src/index.ts:2059-2060` — the OpenAPI 3.1 document served from a static module at `GET /openapi.json`.
6. `worker/src/lib/openapi_devenv.ts:4` — `devenvOpenApiSpec`, the published contract for the surface.
7. `wrangler.toml:250-251` — the surviving comment where the `RUNNER_DEVENV_DO` cross-Worker binding stood before #1447 removed it.
8. `wrangler.toml:566-568` — the mirroring rule the binding must obey when it returns: top-level-only ships green in dev and a permanent 503 in prod.
8a. `worker/src/index.ts:164` — `RUNNER_DEVENV_DO?` is OPTIONAL in the `Env` type, which is why the absent binding is a 503 and not a boot failure.
8b. `worker/src/index.ts:2995-2997` — the handler's explicit absent-binding arm: `SERVICE_UNAVAILABLE`, not a throw.
