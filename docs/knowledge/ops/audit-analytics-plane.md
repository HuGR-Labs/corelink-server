---
type: "Runbook"
title: "The audit export + analytics plane and customer re-verification"
description: "How a tenant exports its tamper-evident audit chain, re-verifies it with the CLI, and queries analytics — all per-tenant isolated and fail-closed."
source_files:
  - "crates/corelink-container/src/routes/audit_drain.rs"
  - "crates/corelink-container/src/routes/audit_analytics.rs"
  - "crates/corelink-container/src/routes/audit_analytics/state.rs"
  - "crates/corelink-container/src/routes/audit_analytics/rate_limit.rs"
  - "crates/corelink-container/src/routes/audit_analytics/audit_sink.rs"
  - "crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs"
  - "crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs"
  - "crates/corelink-container/src/routes/audit_analytics/shadow_factory.rs"
  - "crates/corelink-container/src/routes/audit_export.rs"
  - "crates/corelink-container/src/routes/audit_export/audit_sink.rs"
  - "crates/corelink-container/src/routes/audit_export/handler.rs"
  - "crates/corelink-container/src/routes/audit_export/parse.rs"
  - "crates/corelink-container/src/routes/audit_export/stream.rs"
  - "crates/corelink-container/src/routes/audit_export/state.rs"
  - "crates/corelink-container/src/routes/audit_export/types.rs"
  - "apps/analytics-worker/src/ingest.ts"
  - "apps/signup-worker/src/webhooks/audit_drain_cron.ts"
  - "docs/cli/audit-export.md"
checkpoint_sha: "202d597d16133c49d04501553d6d5d263a28d3fd"
provenance: "AUTHORED"
tags: ["ops", "audit", "export", "analytics", "compliance", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# The audit export + analytics plane and customer re-verification

The audit plane lets a tenant pull its own append-only, BLAKE3-chained audit log as a streaming NDJSON
envelope, re-verify the whole chain offline or off-the-wire with the `corelink` CLI, and run bounded
analytics queries over it. The trust property is end-to-end: the server emits a chain-head anchor and,
if it detects a break mid-stream, an abort trailer that the CLI surfaces as a distinct exit code so
SIEM/Drata wrappers can tell a data-integrity event from a generic failure. Every route is per-tenant
isolated and the audit emit is fail-closed. Related: [the audit-analytics crate cluster](/crates/audit-analytics.md).

The tamper-evidence the export plane proves is produced UPSTREAM by the **S-09 audit-chain drain**
(`POST /_internal/audit/drain`), which as of current main is **LIVE** — no longer a deferred skeleton.
The `corelink_audit_chain` BLAKE3 [`HashChainBuilder`] was always real + property-tested but had no live
producer; the drain is that producer. It seals the pending `audit_outbox` rows (the plain, unchained
CloudEvents envelopes written by the DSR/analytics/export sinks, where `emitted_at IS NULL`) into the
hash chain — computing `chain_hash = BLAKE3(prev_hash || RFC-8785-JCS(payload))` per row, persisting the
exact JCS bytes the verifier re-hashes, and advancing the per-`(tenant, region)` `audit_chain_head`
checkpoint. Until a drain runs, a given partition's `audit_outbox` rows are NOT yet tamper-evident (a D1
writer could alter or delete a row undetected); after it runs they are linked into the verifiable chain.
An hourly Cloudflare Cron Trigger in the signup-worker (`apps/signup-worker/src/webhooks/audit_drain_cron.ts`)
POSTs the drain endpoint so the seal converges (idempotent → a missed hour just seals a bigger batch
next run; INERT no-op until the erase/internal-auth key is bound). Note the EXPORT route's durable backing is still partly
deferred — `build_state` wires an IN-MEMORY exporter + sink; the durable R2-backed exporter + CloudEvents
sink remain DEFERRED behind the stable `Arc<dyn …>` surface (see below) — but the chain-sealing producer
itself is now wired.

# Role

It is the compliance read-and-prove surface: customers (and their evidence-collection automation) get
a cryptographically re-verifiable export plus rate-limited analytics, without ever being able to read
another tenant's rows.

# How it works

- The S-09 drain endpoint is mounted (env-gated, internal-auth) as `POST /_internal/audit/drain` and is the LIVE chain-seal producer: `handle_drain` scans the `(tenant_id, region)` partitions with pending rows then seals each `crates/corelink-container/src/routes/audit_drain.rs:514-564`.
- The per-partition seal is crash-safe and fork-free: `drain_partition` resolves the resume head (the durable sealed-rows tail is authoritative over the checkpoint when ahead), seals every pending row IN ORDER guarded by `emitted_at IS NULL` (idempotent), then advances the `audit_chain_head` checkpoint with a compare-and-set (single-writer anti-fork → aborts the partition on drift rather than forking) `crates/corelink-container/src/routes/audit_drain.rs:457-512`.
- The link itself is `chain_hash = BLAKE3(prev_hash || RFC-8785-JCS(payload))` over the EXACT bytes persisted as `canonical_jcs` (what the verifier re-hashes); `seal_rows` is fully deterministic, which is the basis of concurrent-drain fork-freedom `crates/corelink-container/src/routes/audit_drain.rs:208-235`.
- The drain auth gate is the constant-time internal-auth check (≥32-char key floor, fail-CLOSED: unmounted without the erase/internal-auth key + D1) `crates/corelink-container/src/routes/audit_drain.rs:95-137`.
- The drain is invoked by an hourly Cloudflare Cron Trigger in the signup-worker — `runAuditDrainSweep` resolves the erase-first/shared-fallback key and POSTs `/_internal/audit/drain`, INERT (no-op skip, never a 401 storm) until that key is bound `apps/signup-worker/src/webhooks/audit_drain_cron.ts:49-100`.
- The export route's public surface (router + state) lives in the `state` submodule (`crates/corelink-container/src/routes/audit_export/state.rs:88`, `crates/corelink-container/src/routes/audit_export/state.rs:157`), re-exported verbatim through the barrel `crates/corelink-container/src/routes/audit_export.rs:144-148`.
- `build_state` wires the LIVE native exporter + export-audit sink as IN-MEMORY implementations (`InMemoryAuditExporter` + `InMemoryExportAuditSink`) — non-durable, lost on process restart and not shared across containers; the durable R2-backed exporter + `RateLimiter` DO singleton + CloudEvents audit sink are DEFERRED behind the stable `Arc<dyn ...>` trait-object surface (`crates/corelink-container/src/routes/audit_export/state.rs:88`).
- The export audit emit is routed through `emit_or_503`, aborting with 503 on sink error `crates/corelink-container/src/routes/audit_export/audit_sink.rs:100-105`.
- Mid-stream chain-break detection emits the abort trailer via `mid_stream_abort_trailer_value` `crates/corelink-container/src/routes/audit_export/stream.rs:412-417`.
- The trailer + chain-head anchor header names are canonical constants `crates/corelink-container/src/routes/audit_export/types.rs:42-57`.
- The CLI offline mode re-verifies a downloaded NDJSON file against a chain-head anchor `docs/cli/audit-export.md:10-31`.
- The CLI HTTP-aware mode streams directly off the wire and watches for the abort trailer `docs/cli/audit-export.md:33-55`.
- Analytics routes (event-count, timeline) ship their router (`crates/corelink-container/src/routes/audit_analytics/state.rs:80`) + per-tenant rate-limit config (`crates/corelink-container/src/routes/audit_analytics/state.rs:63`) enforced by `rate_limit_check` (`crates/corelink-container/src/routes/audit_analytics/rate_limit.rs:99`).
- Each analytics handler runs the native PAT possession gate before any data access `crates/corelink-container/src/routes/audit_analytics.rs:118`.
- The analytics audit emit is itself a fail-CLOSED helper: `emit_or_503` pushes the row through the sink and, on `Err`, drops the prepared success response and returns `503 "audit pipeline closed"` instead `crates/corelink-container/src/routes/audit_analytics/audit_sink.rs:76-85`.
- The `/event-count` handler binds the tenant SOLELY to the `AuthTenant`-extracted `x-corelink-tenant-id` header (400 on a non-UUID), runs the PAT gate, rejects inverted windows, rate-limits, then aggregates per-tenant bucket counts emitting an `analytics_query` audit row on every arm `crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs:42-61`.
- The `/timeline` handler additionally caps query cost with a bucket-cardinality guard — `(to - from) / granularity` must not exceed `MAX_TIMELINE_BUCKETS`, with `granularity` itself bounded to `(0, MAX_GRANULARITY_MS]` — before any aggregate runs `crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs:94-124`.
- `resolve_shadow_via_prelude` prefers the wave-26 `RequestPrelude` region (hot path skips the per-request region round-trip) and, when the prelude is absent or bound to a different tenant, emits a `request_prelude_missing` marker row + WARN and falls back to `shadow_factory.for_tenant` rather than failing the route `crates/corelink-container/src/routes/audit_analytics/shadow_factory.rs:121-156`.
- The export handler treats the client-controllable `:tenant` path segment as an untrusted echo: it is parsed and constant-time-compared against the `AuthTenant` header tenant, and a mismatch emits a SEV-1 cross-tenant audit row and returns 403 (fail-CLOSED: 503 on sink failure) BEFORE any data access `crates/corelink-container/src/routes/audit_export/handler.rs:83-99`.
- The export window timestamps are parsed by `parse_timestamp`, which accepts a raw Unix-epoch-ms integer or a deliberately minimal RFC 3339 `YYYY-MM-DDTHH:MM:SSZ` subset (UTC-only, no fractional seconds, no offsets) to keep the security-sensitive window grammar small `crates/corelink-container/src/routes/audit_export/parse.rs:25-32`.
- The product-analytics EDGE collector (`POST /v1/event` on the standalone analytics-worker) is the ingest tap that feeds the analytics D1 store; it authenticates each request two ways — a CORS `Origin` allow-list for browsers, or a constant-time-compared `X-Corelink-Ingest-Key` for trusted servers — and rejects anything matching neither with 403 before any write (`apps/analytics-worker/src/ingest.ts:114-136`, `apps/analytics-worker/src/ingest.ts:127-136`), the compare being length-checked + XOR-folded so a wrong key cannot be timing-probed (`apps/analytics-worker/src/ingest.ts:71-78`).
- The ingest validator enforces a HARD privacy gate at the edge so no PII reaches the analytics store: a forbidden `email`/`ip`/`ip_address`/`remote_addr` key anywhere in an event's `properties` is rejected and the `event_name` must be in a closed allow-list (`apps/analytics-worker/src/ingest.ts:85-112`, `apps/analytics-worker/src/ingest.ts:104-107`).

# Invariants

- The chain seal is idempotent + fork-free: each row UPDATE is guarded by `emitted_at IS NULL` and the head advance is a compare-and-set on the resumed value, so a re-run or a concurrent drain never double-seals and never forks the chain `crates/corelink-container/src/routes/audit_drain.rs:457-512`.
- The persisted `canonical_jcs` is byte-for-byte what was hashed (`chain_hash = BLAKE3(prev || canonical_jcs)`), so the verifier path re-hashes the stored bytes rather than re-canonicalizing `crates/corelink-container/src/routes/audit_drain.rs:208-235`.
- The export audit row is emitted fail-CLOSED — a sink `Err` aborts with 503 before streaming `crates/corelink-container/src/routes/audit_export/audit_sink.rs:100-105`.
- Analytics data access is gated per-tenant: the PAT gate rejects forged/wrong-tenant (401) or verifier fault (503) before reads `crates/corelink-container/src/routes/audit_analytics.rs:118`.
- The edge ingest tap is authenticated, never anonymous: an event with neither an allow-listed `Origin` nor a correct constant-time-matched `X-Corelink-Ingest-Key` is rejected 403 before any D1 write `apps/analytics-worker/src/ingest.ts:127-136`.
- PII can never land in the analytics store: the edge validator hard-rejects any event whose `properties` carries an `email`/`ip`/`ip_address`/`remote_addr` field — the privacy rule is enforced at the gate, not left to each caller `apps/analytics-worker/src/ingest.ts:104-107`.
- A mid-stream abort surfaces as sysexits DATAERR (65) on the CLI, distinct from generic exit 1 `docs/cli/audit-export.md:79-89`.
- The bearer token is never logged, printed, or surfaced in error messages (CTRL-CRED-001) `docs/cli/audit-export.md:91-99`.

# Gotchas

- The HTTP-fetch path caps the response body at 64 MiB so a malicious server cannot drain CLI memory `docs/cli/audit-export.md:100-102`.
- If both the `--chain-head-anchor` flag and the response header are present they MUST match constant-time — a mismatch is an error, not a warning `docs/cli/audit-export.md:51-55`.

# Citations

1. `crates/corelink-container/src/routes/audit_export/audit_sink.rs:100-105` — `emit_or_503` fail-closed export audit (the enforcing impl).
2. `crates/corelink-container/src/routes/audit_export/state.rs:88`, `crates/corelink-container/src/routes/audit_export/state.rs:157` — export `build_state` (in-memory exporter/sink; durable R2/CloudEvents sink deferred) + `router` impl; `crates/corelink-container/src/routes/audit_export.rs:144-148` — barrel re-export of that public surface.
3. `crates/corelink-container/src/routes/audit_export/stream.rs:412-417` — `mid_stream_abort_trailer_value` builder.
4. `crates/corelink-container/src/routes/audit_export/types.rs:42-57` — chain-head-anchor + abort header constants.
5. `crates/corelink-container/src/routes/audit_analytics/state.rs:80` — analytics `router`; `crates/corelink-container/src/routes/audit_analytics/state.rs:63` + `crates/corelink-container/src/routes/audit_analytics/rate_limit.rs:99` — per-tenant rate-limit config + `rate_limit_check`.
6. `crates/corelink-container/src/routes/audit_analytics.rs:118` — per-handler native PAT possession gate (in-barrel impl).
7. `docs/cli/audit-export.md:10-31` — CLI offline (file) re-verify mode.
8. `docs/cli/audit-export.md:33-55` — CLI HTTP-aware re-verify + anchor cross-check.
9. `docs/cli/audit-export.md:79-89` — exit-65 sysexits DATAERR on mid-stream abort.
10. `docs/cli/audit-export.md:91-99` — token never logged (CTRL-CRED-001) + constant-time anchor compare.
11. `docs/cli/audit-export.md:100-102` — 64 MiB response-body cap.
12. `apps/analytics-worker/src/ingest.ts:114-136` — the edge `POST /v1/event` collector entry (CORS-origin OR ingest-key auth, 403 otherwise); the dual-auth 403 gate at `apps/analytics-worker/src/ingest.ts:127-136`; the constant-time key compare `constantTimeEqual` at `apps/analytics-worker/src/ingest.ts:71-78`.
13. `apps/analytics-worker/src/ingest.ts:85-112` — the `validate()` event gate (closed `event_name` allow-list + size bounds); the hard PII privacy gate forbidding `email`/`ip`/`ip_address`/`remote_addr` in `properties` at `apps/analytics-worker/src/ingest.ts:104-107`.
14. `crates/corelink-container/src/routes/audit_analytics/audit_sink.rs:76-85` — analytics `emit_or_503` fail-CLOSED helper (drops success response, returns 503 on sink `Err`).
15. `crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs:42-61` — `/event-count` handler: `AuthTenant`-bound tenant (400 on non-UUID) + native PAT gate before data access.
16. `crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs:94-124` — `/timeline` bucket-cardinality + granularity cost guards (`MAX_TIMELINE_BUCKETS` / `MAX_GRANULARITY_MS`).
17. `crates/corelink-container/src/routes/audit_analytics/shadow_factory.rs:121-156` — `resolve_shadow_via_prelude`: prelude-preferred region with `request_prelude_missing` marker + factory fallback.
18. `crates/corelink-container/src/routes/audit_export/handler.rs:83-99` — export `:tenant` path-segment constant-time cross-tenant check → SEV-1 row + 403 (fail-CLOSED).
19. `crates/corelink-container/src/routes/audit_export/parse.rs:25-32` — `parse_timestamp`: epoch-ms-or-minimal-RFC3339 window parser.
20. `crates/corelink-container/src/routes/audit_drain.rs:514-564` — `handle_drain`: LIVE S-09 `POST /_internal/audit/drain` entry (partition scan → seal → summary).
21. `crates/corelink-container/src/routes/audit_drain.rs:457-512` — `drain_partition`: crash-safe resume (sealed-tail authoritative) + idempotent `emitted_at IS NULL`-guarded seal + compare-and-set head advance (anti-fork).
22. `crates/corelink-container/src/routes/audit_drain.rs:208-235` — `seal_rows`: deterministic `chain_hash = BLAKE3(prev || RFC-8785-JCS(payload))` over the exact persisted `canonical_jcs` bytes.
23. `crates/corelink-container/src/routes/audit_drain.rs:95-137` — constant-time internal-auth gate + fail-CLOSED env mount (≥32-char key + D1 required).
24. `apps/signup-worker/src/webhooks/audit_drain_cron.ts:49-100` — `runAuditDrainSweep`: hourly cron that POSTs the drain (erase-first/shared-fallback key; inert until bound).
