---
type: "Plane"
title: "Durable Object lifecycle (CoreLinkServer)"
description: "The per-tenant Durable Object that manages the Rust container lifecycle (cold start, health, idle stop) and proxies HTTP to it."
source_files:
  - "worker/src/durable_object.ts"
checkpoint_sha: "2d82ec319d0d20a3689bc444d2875b6b38338031"
provenance: "AUTHORED"
tags: ["planes", "durable-object", "container-lifecycle", "cold-start"]
timestamp: "2026-06-26T00:00:00Z"
---

# Durable Object lifecycle (CoreLinkServer)

`CoreLinkServer` is the Durable Object that sits between the Worker edge and the Rust container. There
is exactly one DO instance per tenant (the Worker keys it by `idFromName(tenant_id)`), and each instance
owns the lifecycle of that tenant's container: it cold-starts the container on first request, polls its
`/_health` until ready, multiplexes every HTTP request onto the container's port 50051 via a TCP-port
`Fetcher`, and destroys the container after an idle timeout. It is also where the container's
environment contract is materialized — the DO is the layer that forwards R2/D1 credentials and every
feature secret into `container.start({ env })`. Its hardest correctness problems are concurrency
(single-threaded but async-concurrent cold-start races) and self-healing stale persisted state.

# Role
- The container lifecycle manager + gRPC/HTTP proxy, one instance per tenant
  (`worker/src/durable_object.ts:1-26`).
- The materializer of the container env contract: every secret/credential the container reads is
  forwarded here through `container.start({ env })` — including the erasure-attestation operator flags
  (`ERASURE_ATTESTATION_REGION`/`_SINGLE_REGION`), guarded by `check-env-contract.py`. The forward-list
  now also carries the container's tuning knobs — `PAT_MINT_MAX_INFLIGHT`, `PAT_MINT_MAX_PER_MINUTE`,
  `QUOTA_COST_PER_OP_MICROS`, `EXPORT_ROW_BUFFER_BYTES` — which the container reads via a const-aliased
  `std::env::var(CONST)` that the old string-literal contract scanner could not see; before they were
  forwarded an operator `wrangler secret put` was SILENTLY ignored (the `ERASURE_SALT_KEY` class of bug),
  each var falling back to a built-in default (empty ⇒ default, unchanged)
  (`worker/src/durable_object.ts:523-677`).

# How it works
1. The DO restores its persisted `LifecycleState` under `blockConcurrencyWhile` on every wakeup so a
   re-hydrated isolate sees the last known container status (`worker/src/durable_object.ts:284-297`).
2. `fetch` binds the Worker-injected `x-corelink-tenant-id` into lifecycle state, ensures the container
   is running, resets the idle timer, then proxies (`worker/src/durable_object.ts:303-370`).
3. `proxyToContainer` rewrites the request to `http://localhost:50051` and forwards it through the
   `getTcpPort(CONTAINER_PORT)` fetcher, never reading the body (`worker/src/durable_object.ts:251-264`).
4. `ensureContainerRunning` is the state machine: running→proceed, stale-running/stopped/degraded→
   restart, starting→wait (`worker/src/durable_object.ts:398-456`).
5. `startContainer` closes the concurrent-start race with a SYNCHRONOUS in-memory flip to `"starting"`
   before the first `await` (`worker/src/durable_object.ts:474-500`).
6. Cold start emits the `corelink.do.cold_start.v1` audit event BEFORE calling `container.start`, per
   the charter's audit-before-mutation rule (`worker/src/durable_object.ts:505-519`).
7. `waitForContainerHealth` polls `GET /_health` on the container port until a 200 or the 90s startup
   timeout (`worker/src/durable_object.ts:778-808`). The wait loop FAST-EXITS the moment the container
   status flips to the terminal `"stopped"` state (a bad deploy / OOM / panic) — it no longer spins the
   full ~90s `STARTUP_TIMEOUT_MS` on a container that is already dead, returning a prompt `503` so the
   next request triggers a restart (`worker/src/durable_object.ts:756-767`).
8. An idle timer destroys the container after `IDLE_TIMEOUT_MS` (5 min), emitting the death event first
   (`worker/src/durable_object.ts:80-83`, `worker/src/durable_object.ts:878-909`).
9. A periodic `alarm` re-probes health and marks the container `degraded` after `MAX_HEALTH_FAILURES`
   (`worker/src/durable_object.ts:931-985`).

# Invariants
- One DO instance per tenant — the DO ID is tenant-derived, never cross-tenant
  (`worker/src/durable_object.ts:1-26`).
- Audit events are emitted BEFORE the state mutation they describe (charter rule)
  (`worker/src/durable_object.ts:505-519`).
- A concurrent burst cannot double-start the container: the in-memory `"starting"` flip is the
  load-bearing guard, set synchronously before any await (`worker/src/durable_object.ts:474-500`).
- A stale persisted `"starting"` older than `STALE_STARTING_MS` self-heals to a restart, never wedging
  every request forever (the F-020 `_system` wedge fix) (`worker/src/durable_object.ts:96-104`).
- The proxy never reads or logs the request body (INV-NO-BODY-IN-LOGS)
  (`worker/src/durable_object.ts:251-264`).

# Gotchas
- DOs are single-threaded but ASYNC-concurrent: every `await` is a yield point where another queued
  `fetch` can run — which is exactly why the concurrent-start guard flips the status synchronously.
- The container env forward is a silent-failure trap: a secret the container reads via `env::var` but
  the DO never forwards will appear "set" to the operator yet never reach the container.

# Citations
1. `worker/src/durable_object.ts:1-26` — the DO's responsibilities + per-tenant pinning doc.
2. `worker/src/durable_object.ts:80-83` — `CONTAINER_PORT` 50051 and the 5-min `IDLE_TIMEOUT_MS`.
3. `worker/src/durable_object.ts:96-104` — `STALE_STARTING_MS` self-heal (the F-020 wedge fix).
4. `worker/src/durable_object.ts:251-264` — `proxyToContainer` via the `getTcpPort` fetcher, no body read.
5. `worker/src/durable_object.ts:284-297` — constructor restoring `LifecycleState` under `blockConcurrencyWhile`.
6. `worker/src/durable_object.ts:303-370` — the DO `fetch` entry: tenant bind, ensure-running, proxy.
7. `worker/src/durable_object.ts:398-456` — `ensureContainerRunning` lifecycle state machine.
8. `worker/src/durable_object.ts:474-500` — the synchronous in-memory `"starting"` concurrent-start guard.
9. `worker/src/durable_object.ts:505-519` — audit-before-mutation cold-start event + `container.start`.
10. `worker/src/durable_object.ts:523-677` — the `container.start({ env })` env-contract forward.
11. `worker/src/durable_object.ts:778-808` — `waitForContainerHealth` polling `/_health`; `worker/src/durable_object.ts:756-767` — the M1 fast-exit on a terminal `"stopped"` container (no full ~90s spin on a dead container).
12. `worker/src/durable_object.ts:878-909` — the idle-timeout destroy path.
13. `worker/src/durable_object.ts:931-985` — the periodic `alarm` health re-probe + degrade.
