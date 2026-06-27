---
type: "ComplianceControl"
title: "DSR edge crons + erasure-queue consumer"
description: "The signup-worker scheduled() handler: the 24h DSR verify sweep, the dsr.queued.v1 erasure-queue consumer that drives the container erase endpoint, and the PAT-plaintext scrub cron — the edge-plane half of CoreLink's GDPR erasure obligation."
source_files:
  - apps/signup-worker/src/webhooks/dsr_verify_cron.ts
  - apps/signup-worker/src/webhooks/dsr_consumer.ts
  - apps/signup-worker/src/webhooks/pat_scrub_cron.ts
  - apps/signup-worker/src/index.ts
checkpoint_sha: "7f62573f2be4f07352de830fe98f400bb1345adb"
provenance: "AUTHORED"
tags: ["dsr", "gdpr", "erasure", "cron", "queue", "scheduled", "pat", "compliance", "worker-edge"]
timestamp: "2026-06-27T00:00:00Z"
---

CoreLink's bytes-actually-deleted erasure engine lives in the container (`routes/dsr.rs`, see the sibling [DSR / right-to-erasure pipeline](/compliance/dsr-erasure.md)), but the **edge plane** owns three time-driven obligations that the synchronous request path cannot: (1) re-confirming, after the 24h SLA window, that every queued erasure actually completed across all backends; (2) carrying a `user.deleted` erasure obligation from the Cloudflare Queue to the container's internal erase endpoint with at-least-once reliability; and (3) guaranteeing the freshly-minted PAT plaintext never stays resident in Clerk `private_metadata` if the user never visits `/welcome`. All three are wired into the signup-worker's single `scheduled()` / `queue()` runtime and are LIVE — the DSR verify sweep + PAT scrub fire on the hourly Cron Trigger, the consumer on every `dsr.queued.v1` delivery. The DSR sweep's load-bearing source, the `dsr_requested` anchor table, is present in prod D1. Every arm is fail-CLOSED: an unbound auth key or a transport fault makes a sweep inert or retries the message rather than silently dropping a regulatory obligation.

# Role

This control is the **edge-plane scheduler for GDPR Art.17 erasure + credential hygiene**. It is not the eraser (the container is); it is the durable clock and the reliable courier around it. It closes three gaps the in-request container path leaves open: an SLA-breach that never produced a tombstone would be invisible without a sweep; an async `user.deleted` erasure needs queue-grade redelivery, not a best-effort fetch; and a one-time-reveal secret needs a backstop scrub for the user who never reveals it. The Cloudflare `scheduled()` and `queue()` handlers in the signup-worker are the runtime that drives all three.

# How it works

- **Hourly Cron Trigger drives two independent sweeps.** The `scheduled()` handler computes `nowMs` once and fires both the DSR verify sweep and the PAT scrub under separate `ctx.waitUntil(...)`, each `.catch()`-isolated so one failing sweep never aborts the other (`apps/signup-worker/src/index.ts:89-122`).
- **DSR verify sweep is gated on the D1 binding.** `runDsrVerifySweep` runs only when `env.CONFIG_DB` is bound; otherwise the sweep is skipped entirely at the `scheduled()` seam (`apps/signup-worker/src/index.ts:92-107`).
- **24h SLA enumeration from the load-bearing `dsr_requested` anchor.** The sweep's primary source is `SELECT dsr_id, tenant_id, requested_at FROM dsr_requested WHERE status = 'requested' AND requested_at <= ?1`, where `?1 = nowMs - DEADLINE_MS` (24h), catching DSRs that failed before any tombstone and so have no `dsr_erasure_log` row (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts:165-171`, `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:146-149`, `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:66`).
- **Legacy second source + dedupe.** A second query over `dsr_erasure_log` (bounded by a 7-day look-back window) catches DSRs that produced ≥1 tombstone, and the two sets are merged by `dsr_id` with the `dsr_requested` anchor winning as the true SLA clock (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts:182-189`, `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:199-221`).
- **Each candidate POSTs the container verify endpoint.** `postVerify` sends `POST /_internal/dsr/verify` with the `x-corelink-internal-auth` header, preferring the dedicated erase key over the shared key, optionally via the `CORELINK_API_SVC` service binding (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts:95-110`).
- **Sweep flips the anchor only on `verified_complete`.** A `VerifiedComplete` (ok) response with `decision === "verified_complete"` flips the anchor to `status = 'verified'` so it drops out of future sweeps; a `verified_partial` / `sla_breached` still returns HTTP 200 but leaves the row enumerable for the next tick (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts:237-249`).
- **Erasure-queue consumer carries `dsr.queued.v1` to the container.** The `queue()` handler delegates each batch to `handleErasureQueueBatch`, which calls `processErasureMessage` per message and acks on a 2xx, retries otherwise — per-message isolation, no head-of-line block (`apps/signup-worker/src/index.ts:69-79`, `apps/signup-worker/src/webhooks/dsr_consumer.ts:94-106`).
- **Consumer forwards to the internal erase endpoint.** `processErasureMessage` POSTs the message body to `POST /_internal/dsr/erase` with the `x-corelink-internal-auth` header, via the service binding when present (`apps/signup-worker/src/webhooks/dsr_consumer.ts:68-80`).
- **PAT scrub pages all Clerk users and clears stale reveals.** `runPatScrubSweep` pages the Clerk Backend API (≤`MAX_PAGES`), and for each user past the reveal TTL it `scrubUser` PATCHes `private_metadata.pat_plaintext` (+ its clock) to `null`, which Clerk's merge semantics treat as key-removal (`apps/signup-worker/src/webhooks/pat_scrub_cron.ts:149-172`, `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:102-128`).
- **Scrub decision is TTL-bounded and fail-CLOSED.** `shouldScrub` keeps a reveal within `PAT_REVEAL_TTL_MS` (1h) so `/welcome` can still reveal it once, but treats a present-plaintext-with-missing-clock as STALE and scrubs it (`apps/signup-worker/src/webhooks/pat_scrub_cron.ts:81-94`, `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:38`).

# Invariants

- **An SLA breach without a tombstone is still detected.** The `dsr_requested` anchor query has no lower window bound, so a DSR permanently stuck at `status = 'requested'` is exactly the row the sweep keeps surfacing until it completes or self-expires after a week (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts:163-171`).
- **The sweep is inert, never permissive, without an auth key.** When neither the dedicated erase key nor the shared internal key is bound, `runDsrVerifySweep` returns `skipped: true` with zero counts and makes no verify call — it never proceeds unauthenticated (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts:137-145`).
- **A verify transport error never aborts the sweep.** `postVerify` catches transport throws and reports `ok: false` rather than propagating, so one unreachable verify never blocks the rest of the batch (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts:120-127`).
- **A missing `dsr_requested` table degrades, it does not fail.** If the anchor query throws (env without migration 0069), the sweep logs and falls back to the `dsr_erasure_log` source rather than failing the whole tick (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts:173-179`).
- **An un-authenticatable erasure is retried, never dropped.** When no erase/internal key is bound, `processErasureMessage` returns `ok: false` so the batch handler `message.retry()`s — a GDPR erasure obligation self-heals once the secret is provisioned, it is never acked away (`apps/signup-worker/src/webhooks/dsr_consumer.ts:59-66`, `apps/signup-worker/src/webhooks/dsr_consumer.ts:100-104`).
- **Redelivery is safe because the container erase is idempotent.** A non-2xx or transport error retries the message; the container orchestrator dedups per `(dsr_id, backend)`, so a redelivered erasure never double-erases (`apps/signup-worker/src/webhooks/dsr_consumer.ts:76-86`).
- **The PAT scrub never proceeds without the Clerk key.** `runPatScrubSweep` returns `skipped: true` when `CLERK_SECRET_KEY` is unbound, and `scrubUser` never throws on a single bad user so the sweep completes (`apps/signup-worker/src/webhooks/pat_scrub_cron.ts:140-143`, `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:122-127`).
- **A secret with no usable reveal clock is scrubbed, not kept.** `shouldScrub` returns `true` for a present `pat_plaintext` whose `pat_revealed_at` is missing/NaN/≤0, so a legacy or mis-written secret is never left resident forever (`apps/signup-worker/src/webhooks/pat_scrub_cron.ts:88-92`).

# Gotchas

- The cited cron files live under `apps/signup-worker/src/webhooks/`, NOT `src/lib/` or a `src/crons/` dir (the authoring brief flagged the path as unconfirmed) — `lib/` holds only the shared helpers (`erase-auth-key.ts`, `d1.ts`) these crons import.
- The DSR verify sweep does NOT delete bytes — it only re-fingerprints and flips the anchor; the actual erasure is the container's `POST /_internal/dsr/erase` engine. The sweep is a verification + SLA-breach detector, not an eraser (`apps/signup-worker/src/webhooks/dsr_verify_cron.ts:130-136`).
- Both edge crons are best-effort by design: the hourly tick re-runs and every operation (verify, erase, scrub) is idempotent, so a dropped status-flip / a stopped page-loop self-corrects on the next tick — do not add a hard-fail retry layer on top.
- Inert-until-bound is not a bug: the DSR sweep stays no-op until an erase/internal key is bound (task #46) and the PAT scrub until `CLERK_SECRET_KEY` is bound; both report `skipped: true` so an unprovisioned env reads as inert, not broken.

# Citations

1. `scheduled()` handler drives both hourly sweeps, `.catch()`-isolated under `waitUntil`: `apps/signup-worker/src/index.ts:89-122`; DSR sweep gated on `CONFIG_DB`: `apps/signup-worker/src/index.ts:92-107`.
2. `queue()` handler → `handleErasureQueueBatch`: `apps/signup-worker/src/index.ts:69-79`.
3. 24h SLA enumeration from `dsr_requested` (load-bearing anchor): `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:165-171`; deadline `nowMs - DEADLINE_MS` + the 24h constant: `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:146-149`, `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:66`.
4. No-lower-bound on the anchor query (stuck-DSR breach detection): `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:163-171`.
5. Legacy `dsr_erasure_log` second source + dedupe by `dsr_id`: `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:182-189`, `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:199-221`.
6. `POST /_internal/dsr/verify` with `x-corelink-internal-auth`: `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:95-110`.
7. Anchor flips to `verified` only on `verified_complete`: `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:237-249`.
8. Sweep inert without an auth key (`skipped: true`): `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:137-145`; verify transport error → `ok:false`, never aborts: `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:120-127`; missing `dsr_requested` degrades to the log source: `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:173-179`; sweep is verify-only not eraser: `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:130-136`.
9. Consumer ack-on-2xx / retry-otherwise, per-message isolation: `apps/signup-worker/src/webhooks/dsr_consumer.ts:94-106`, `apps/signup-worker/src/webhooks/dsr_consumer.ts:100-104`.
10. Consumer POSTs `POST /_internal/dsr/erase` with the internal-auth header (svc-binding aware): `apps/signup-worker/src/webhooks/dsr_consumer.ts:68-80`.
11. No erase key → `ok:false` → retry (never drops the obligation): `apps/signup-worker/src/webhooks/dsr_consumer.ts:59-66`; idempotent redelivery: `apps/signup-worker/src/webhooks/dsr_consumer.ts:76-86`.
12. PAT scrub pages Clerk users + per-user scrub: `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:149-172`; PATCH `private_metadata.pat_plaintext = null`: `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:102-128`.
13. `shouldScrub` TTL + fail-closed stale decision: `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:81-94`, `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:88-92`; 1h reveal TTL: `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:38`.
14. Scrub inert without `CLERK_SECRET_KEY`; `scrubUser` never throws on one bad user: `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:140-143`, `apps/signup-worker/src/webhooks/pat_scrub_cron.ts:122-127`.
15. Sibling container-side erasure engine: [DSR / right-to-erasure pipeline](/compliance/dsr-erasure.md).
</content>
</invoke>
