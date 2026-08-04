---
type: "Plane"
title: "Durable Object lifecycle (CoreLinkServer)"
description: "The per-tenant Durable Object that manages the Rust container lifecycle (cold start, health, idle stop) and proxies HTTP to it."
source_files:
  - "worker/src/durable_object.ts"
  - "worker/src/index.ts"
  - "worker/src/event_log_do.ts"
  - "worker/src/rollout_controller.ts"
  - "worker/src/replication_coordinator_do.ts"
source_blobs:
  - "worker/src/durable_object.ts@9b407f2b402cc395576be41779afe9387d5a6bc9"
  - "worker/src/index.ts@dc5e60d15637947b7e2b5d7737ab8f293cadf8e8"
  - "worker/src/event_log_do.ts@2dac20f20396ef0e0f5f35a0b3b640c096aabe09"
  - "worker/src/rollout_controller.ts@cb91436477c54a2d6085f5a52a52a6e130ab79a0"
  - "worker/src/replication_coordinator_do.ts@9123a2c4cd02c0f71e363a6447f3e15edf12ed1e"
checkpoint_sha: "e34c59ea521e8d4b86bcd211bc0795b4f855259b"
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
  each var falling back to a built-in default (empty ⇒ default, unchanged). The same forward now also
  carries the OPTIONAL CF-6 audit-chain head-signing vars
  `AUDIT_CHAIN_SIGNING_SEED_HEX`/`AUDIT_CHAIN_SIGNING_KEY_ID`, which default to reusing the
  erasure-attestation seed/key-id when unset, and the CTRL-PRIV-001 `EMAIL_HASH_SALT`
  (the server-held salt for the email-hash pseudonym; unset ⇒ legacy unsalted SHA-256, zero
  regression) — which the container's `email_hash::hash_email` reads, so it MUST be forwarded
  here or a `wrangler secret put EMAIL_HASH_SALT` would never reach the container
  (`worker/src/durable_object.ts:564-761`).

# How it works
1. The DO restores its persisted `LifecycleState` under `blockConcurrencyWhile` on every wakeup so a
   re-hydrated isolate sees the last known container status (`worker/src/durable_object.ts:314-327`).
2. `fetch` binds the Worker-injected `x-corelink-tenant-id` into lifecycle state, ensures the container
   is running, touches the durable idle clock (`lastActivityMs`, in-memory — the alarm persists it),
   then proxies (`worker/src/durable_object.ts:333-432`).
3. `proxyToContainer` rewrites the request to `http://localhost:50051` and forwards it through the
   `getTcpPort(CONTAINER_PORT)` fetcher, never reading the body (`worker/src/durable_object.ts:282-295`).
4. `ensureContainerRunning` is the state machine: running→proceed, stale-running/stopped/degraded→
   restart, starting→wait (`worker/src/durable_object.ts:439-497`).
5. `startContainer` closes the concurrent-start race with a SYNCHRONOUS in-memory flip to `"starting"`
   before the first `await` (`worker/src/durable_object.ts:507-542`).
6. Cold start emits the `corelink.do.cold_start.v1` audit event BEFORE calling `container.start`, per
   the charter's audit-before-mutation rule (`worker/src/durable_object.ts:547-564`).
7. `waitForContainerHealth` polls `GET /_health` on the container port until a 200 or the 90s startup
   timeout (`worker/src/durable_object.ts:872-902`). The `waitForContainerReady` queue FAST-EXITS the
   moment the container status flips to the terminal `"stopped"` state (a bad deploy / OOM / panic) — it
   no longer spins the full ~90s `STARTUP_TIMEOUT_MS` on a container that is already dead, returning a
   prompt `503` so the next request triggers a restart (`worker/src/durable_object.ts:837-866`).
8. The idle reaper lives INSIDE the alarm tick (durable), not in a `setTimeout`: it compares the
   persisted `lastActivityMs` against `IDLE_TIMEOUT_MS` (30 min) and, on expiry, emits the death event,
   re-checks the clock (the emit is an outbound POST — a yield point a live request can slip through),
   destroys the container, and ENDS the alarm chain so the DO hibernates too
   (`worker/src/durable_object.ts:96-113`, `worker/src/durable_object.ts:1043-1072`). An in-memory
   timer could not do this job: it evaporates on isolate eviction while the storage-backed alarm chain
   survives, which is exactly how a once-started container became immortal and billed 24/7.
9. `alarm()` is a `try/finally` shell that ALWAYS re-arms the chain unless the tick deliberately ended
   it (dead container, or a just-reaped one) — a throwing tick can never strand the reaper, and the DO
   `fetch` path re-arms a chain that was already lost (`getAlarm() === null`). `alarmTick()` re-probes
   health and marks the container `degraded` after `MAX_HEALTH_FAILURES`; a `degraded`-but-running
   container stays in the chain (probe skipped) so it remains subject to the reaper
   (`worker/src/durable_object.ts:991-1007`, `worker/src/durable_object.ts:1015-1128`).
10. The Worker exports three SIBLING DO classes alongside `CoreLinkServer`
    (`worker/src/index.ts:3347`). `EventLogDO` is the ADR-0065 per-tenant append-only event-log
    primitive — it adopts the first `x-corelink-tenant-id` it sees, persists that pin, and refuses any
    other tenant's request with a `403 TENANT_MISMATCH` (`worker/src/event_log_do.ts:207-218`). Its
    `append` monotonically assigns `seq` under `blockConcurrencyWhile` (persist the entry, THEN advance
    the head, so a crash orphans rather than gaps), dispatched from `/_eventlog/append`
    (`worker/src/event_log_do.ts:220-225`, `worker/src/event_log_do.ts:266-277`). It is bound in
    `wrangler.toml` and exported, but NO edge route dispatches to it — the `EVENT_LOG_DO` binding is
    referenced only as an OPTIONAL field of `Env` (`worker/src/index.ts:65-70`); it is a ready primitive
    awaiting its hugit-P2 seam-D consumer.
11. `RolloutController` is an UNWIRED stub: bound in `wrangler.toml` and exported, it answers
    `/_do/health` with 200 but returns `501 NOT_IMPLEMENTED` ("RolloutController WASM bridge not yet
    wired (Phase C)") for every other request — the real rollout logic lives in Rust/WASM and is not yet
    bridged (`worker/src/rollout_controller.ts:34-56`).
12. `ReplicationCoordinatorDO` (WI-MULTI-REGION-V1) is the SINGLE global replication-coordinator DO —
    addressed by a FIXED name (`idFromName(REPLICATION_COORDINATOR_SINGLETON)`), so its single-instance /
    single-writer guarantee IS the split-brain-safe promotion lock. It persists the region role-map +
    heartbeats under `blockConcurrencyWhile` and self-arms a periodic `alarm()` that runs the
    evaluate→promote tick and always re-arms itself (`worker/src/replication_coordinator_do.ts:419-476`).
    The Worker intercepts `/_internal/replication/*` (after the shared internal-auth gate, before the
    generic container forward) and maps it to the DO's `/_repl/<op>` (`arm`/`status`/`tick`)
    (`worker/src/replication_coordinator_do.ts:525-547`).

# Invariants
- One DO instance per tenant — the DO ID is tenant-derived, never cross-tenant
  (`worker/src/durable_object.ts:1-26`).
- Audit events are emitted BEFORE the state mutation they describe (charter rule)
  (`worker/src/durable_object.ts:547-564`).
- A concurrent burst cannot double-start the container: the in-memory `"starting"` flip is the
  load-bearing guard, set synchronously before any await (`worker/src/durable_object.ts:507-542`).
- A stale persisted `"starting"` older than `STALE_STARTING_MS` self-heals to a restart, never wedging
  every request forever (the F-020 `_system` wedge fix) (`worker/src/durable_object.ts:127-135`).
- The proxy never reads or logs the request body (INV-NO-BODY-IN-LOGS)
  (`worker/src/durable_object.ts:282-295`).
- `EventLogDO` is per-tenant and append-only: a DO pinned to one tenant rejects a request carrying a
  different `x-corelink-tenant-id` with `403 TENANT_MISMATCH` (`worker/src/event_log_do.ts:207-218`),
  and `seq` is strictly monotonic + gap-free under `blockConcurrencyWhile` (persist-entry-then-advance-
  head) (`worker/src/event_log_do.ts:266-277`).
- Honest wiring: of the four exported DO classes, `CoreLinkServer` serves every tenant's data plane and
  `ReplicationCoordinatorDO` serves the single-instance `/_internal/replication/*` control plane
  (`worker/src/replication_coordinator_do.ts:419-476`). `EventLogDO` is implemented + bound + exported but
  has NO live edge caller yet (`EVENT_LOG_DO` is an OPTIONAL `Env` field, `worker/src/index.ts:65-70`), and
  `RolloutController` is a bound+exported STUB returning `501 NOT_IMPLEMENTED` for everything but health
  (`worker/src/rollout_controller.ts:34-56`).
- The replication coordinator is a TRUE singleton: because the Worker always addresses it by the fixed
  `REPLICATION_COORDINATOR_SINGLETON` name, at most one instance can promote a replica at a time — that
  name-addressed single-writer property IS the split-brain-safe lock (never auto-promotes into a
  partition) (`worker/src/replication_coordinator_do.ts:525-547`).

# Gotchas
- DOs are single-threaded but ASYNC-concurrent: every `await` is a yield point where another queued
  `fetch` can run — which is exactly why the concurrent-start guard flips the status synchronously.
- The container env forward is a silent-failure trap: a secret the container reads via `env::var` but
  the DO never forwards will appear "set" to the operator yet never reach the container.
- Two of the Worker's four exported DO classes are NOT live request paths: `EventLogDO` is a wired-but-
  unconsumed primitive (no edge dispatcher reads the optional `EVENT_LOG_DO` binding) and
  `RolloutController` is a Phase-C stub (501) — don't cite either as an active serving plane.
  (`CoreLinkServer` and `ReplicationCoordinatorDO` ARE live.)

# Citations
1. `worker/src/durable_object.ts:1-26` — the DO's responsibilities + per-tenant pinning doc.
2. `worker/src/durable_object.ts:91-113` — `CONTAINER_PORT` 50051 and the 30-min `IDLE_TIMEOUT_MS`, whose header documents why the reaper MUST be alarm-driven (the in-memory timer died on isolate eviction ⇒ immortal, 24/7-billed containers).
3. `worker/src/durable_object.ts:127-135` — `STALE_STARTING_MS` self-heal (the F-020 wedge fix).
4. `worker/src/durable_object.ts:282-295` — `proxyToContainer` via the `getTcpPort` fetcher, no body read.
5. `worker/src/durable_object.ts:314-327` — constructor restoring `LifecycleState` under `blockConcurrencyWhile`.
6. `worker/src/durable_object.ts:333-432` — the DO `fetch` entry: tenant bind, ensure-running, idle-clock touch, proxy.
7. `worker/src/durable_object.ts:439-497` — `ensureContainerRunning` lifecycle state machine.
8. `worker/src/durable_object.ts:507-542` — the synchronous in-memory `"starting"` concurrent-start guard.
9. `worker/src/durable_object.ts:547-564` — audit-before-mutation cold-start event + `container.start`.
10. `worker/src/durable_object.ts:564-761` — the `container.start({ env })` env-contract forward.
11. `worker/src/durable_object.ts:872-902` — `waitForContainerHealth` polling `/_health`; `worker/src/durable_object.ts:837-866` — the M1 fast-exit on a terminal `"stopped"` container (no full ~90s spin on a dead container).
12. `worker/src/durable_object.ts:1043-1072` — the DURABLE idle reaper inside `alarmTick()`: an absent `lastActivityMs` backfills, an expired one emits the death event (audit-before-mutation), RE-CHECKS the clock (every `await` is a yield point where a queued request may have arrived), then destroys the container and signals chain-end so the DO hibernates instead of heartbeating a dead container forever.
13. `worker/src/durable_object.ts:991-1007` — `alarm()`: a thin `try/finally` that ALWAYS re-arms the chain unless the tick reported a deliberate end, so a throwing tick can never strand the reaper (the posture `ReplicationCoordinatorDO.alarm()` already used); `worker/src/durable_object.ts:1015-1128` — `alarmTick()`: chain guard (dead container ⇒ end the chain), the idle reaper, the `degraded`-but-running arm (probe skipped, chain kept so the reaper still applies), the dedup arm, then the health re-probe + degrade.
14. `worker/src/index.ts:3347` — the Worker's named export of `CoreLinkServer`, `RolloutController`, `EventLogDO`, `ReplicationCoordinatorDO` (the DO-class exports at the module tail, immediately after the `export default handler` Sentry-wrapped fetch handler).
15. `worker/src/event_log_do.ts:207-218` — `EventLogDO` cross-tenant guard: a tenant-pinned DO rejects a different `x-corelink-tenant-id` with `403 TENANT_MISMATCH` (ADR-0065).
16. `worker/src/event_log_do.ts:220-225` — the `/_eventlog/append` + `/_eventlog/read` route dispatch.
17. `worker/src/event_log_do.ts:266-277` — `handleAppend`: monotonic gap-free `seq` under `blockConcurrencyWhile` (persist-entry-then-advance-head).
18. `worker/src/index.ts:65-70` — the `EVENT_LOG_DO` binding declared OPTIONAL on `Env` (the only `worker/src` reference; no edge route dispatches to it yet).
19. `worker/src/rollout_controller.ts:34-56` — `RolloutController` UNWIRED stub: `/_do/health` 200 but `501 NOT_IMPLEMENTED` "WASM bridge not yet wired (Phase C)" for all other requests.
20. `worker/src/replication_coordinator_do.ts:419-476` — `ReplicationCoordinatorDO`: the single global coordinator DO — persists role-map + heartbeats under `blockConcurrencyWhile` and self-arms the periodic `alarm()` evaluate→promote driver (always re-arms, even on a throwing tick).
21. `worker/src/replication_coordinator_do.ts:525-547` — the DO `fetch` router for the `/_repl/<op>` control plane (`arm`/`status`/`tick`), which the Worker reaches by mapping `/_internal/replication/*` onto it; the fixed `REPLICATION_COORDINATOR_SINGLETON` name is the split-brain-safe single-writer promotion lock.
