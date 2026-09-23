---
type: "Runbook"
title: "The audit export + analytics plane and customer re-verification"
description: "How a tenant exports its tamper-evident audit chain, re-verifies it with the CLI, and queries analytics — all per-tenant isolated and fail-closed."
source_files:
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01_part2.rs"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part3.rs"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_03.rs"
  - "crates/corelink-container/src/routes/audit_analytics.rs"
  - "crates/corelink-container/src/routes/audit_analytics/state.rs"
  - "crates/corelink-container/src/routes/audit_analytics/rate_limit.rs"
  - "crates/corelink-container/src/routes/audit_analytics/audit_sink.rs"
  - "crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs"
  - "crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs"
  - "crates/corelink-container/src/routes/audit_analytics/shadow_factory.rs"
  - "crates/corelink-container/src/routes/audit_analytics/d1_sink.rs"
  - "crates/corelink-container/src/routes/audit_export.rs"
  - "crates/corelink-container/src/routes/audit_export/audit_sink.rs"
  - "crates/corelink-container/src/routes/audit_export/handler.rs"
  - "crates/corelink-container/src/routes/audit_export/parse.rs"
  - "crates/corelink-container/src/routes/audit_export/stream.rs"
  - "crates/corelink-container/src/routes/audit_export/state.rs"
  - "crates/corelink-container/src/routes/audit_export/types.rs"
  - "apps/analytics-worker/src/ingest.ts"
  - "apps/signup-worker/src/webhooks/audit_drain_cron.ts"
  - "scripts/verify-signup-worker-secrets.sh"
  - ".github/workflows/signup-worker-deploy.yml"
  - "docs/cli/audit-export.md"
source_blobs:
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs@da516361a5e83dbc75ac42df2612664067719c28"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01_part2.rs@30736d73ba18cdedea2741455f8364963267f5a6"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs@41ca30d8bfca9a0d5794a5c5f075f54ffc923460"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs@49a10a1b0a1529dc035dcb64bfd506a5e37f2314"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part3.rs@e203c91bad78d1eda38bbbfc34ccf1e4d16335ab"
  - "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_03.rs@afa44c26c394cdc1fb35e16317337527ff6b5037"
  - "crates/corelink-container/src/routes/audit_analytics.rs@9dc5a2d75801c56e85ae91207c22e06e773f0b46"
  - "crates/corelink-container/src/routes/audit_analytics/state.rs@8ab24aae4b9c2e7f8e9a695d90160657720510d0"
  - "crates/corelink-container/src/routes/audit_analytics/rate_limit.rs@0d4c178138803676e1362c9c5791d135023bd948"
  - "crates/corelink-container/src/routes/audit_analytics/audit_sink.rs@ea4bc3e9e6fa5deef38504c5c9e58c48e90e365f"
  - "crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs@af98f0eec3d342cd6f183949b54535d3c591366d"
  - "crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs@8c18de6cd43038e42b2eadbe91194ed282c702bf"
  - "crates/corelink-container/src/routes/audit_analytics/shadow_factory.rs@d5dad6a73495c50bffb63e7d525d748ffc1d56cd"
  - "crates/corelink-container/src/routes/audit_analytics/d1_sink.rs@fc3d825e829e0a3173d2d820af22f504208ac7b4"
  - "crates/corelink-container/src/routes/audit_export.rs@d8d21c6266f5e918b20d55fe3eb9e2ab826ce457"
  - "crates/corelink-container/src/routes/audit_export/audit_sink.rs@fce98bed160671fad821f0ff2bf1e3fd0368d785"
  - "crates/corelink-container/src/routes/audit_export/handler.rs@004f3b3108cef24b0a39c38d3848aac3ff3ae326"
  - "crates/corelink-container/src/routes/audit_export/parse.rs@6578d6a6da7dcdab2030b76744f621a4eb7a254d"
  - "crates/corelink-container/src/routes/audit_export/stream.rs@d3686a858ff91594fa3cf4a53aaf9022d3ddbeda"
  - "crates/corelink-container/src/routes/audit_export/state.rs@39c9d125936f2b764e08265ac24bf9571373bc5b"
  - "crates/corelink-container/src/routes/audit_export/types.rs@28cb841350019701917580b514078bfb972727bb"
  - "apps/analytics-worker/src/ingest.ts@a2a6b9fdbeda8e0bfa6cecee7f1b72974aad30f9"
  - "apps/signup-worker/src/webhooks/audit_drain_cron.ts@b280343eeedaffb7308f1a7b35a20bf989ff693e"
  - "scripts/verify-signup-worker-secrets.sh@f6bc52815c9bddf3d2b9cd11658226f880df08eb"
  - ".github/workflows/signup-worker-deploy.yml@c23cc5068234015d0301663a942a072033c1b28e"
  - "docs/cli/audit-export.md@813c70f0a1807411138d865e73c44bb16143102e"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
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

As of current main the drain is also **keyed** (CF-6): the per-`(tenant, region)` `audit_chain_head` is
Ed25519-SIGNED, not just BLAKE3-chained. Unkeyed BLAKE3 alone is forgeable by an insider with D1 write —
they can rewrite the sealed rows, recompute a self-consistent chain + head, overwrite the checkpoint to
match, and the unkeyed verifier still passes. So on every checkpoint ADVANCE the drain signs the canonical
head tuple — RFC-8785 JCS of `{head_hash, next_sequence, region, tenant_id}` — and persists the
`head_signature` (base64) + `head_signed_at_ms` + `signing_key_id` columns (migration 0080). On RESUME the
stored signature is re-verified against that canonical tuple + the seed-derived public key; a head that
does NOT verify (or a signed head with no seed configured to verify it) is treated as TAMPER and the drain
refuses to extend the chain from it — fail-CLOSED, logged SEV-1 `tamper_detected`. A NULL signature
(pre-0080 legacy head) or a head signed under a DIFFERENT `signing_key_id` (seed/key rotation) is tolerated
and re-signed on the next advance. The signing key REUSES the per-region erasure-attestation Ed25519 seed
(no new secret; an optional dedicated `AUDIT_CHAIN_SIGNING_SEED_HEX` takes precedence), and with NO seed
configured the head is advanced UNSIGNED (legacy/tolerated) to preserve drain liveness in dev/CI. Net: the
live audit chain head is now tamper-EVIDENT against a malicious D1 writer, not merely append-linked.

# Role

It is the compliance read-and-prove surface: customers (and their evidence-collection automation) get
a cryptographically re-verifiable export plus rate-limited analytics, without ever being able to read
another tenant's rows.

# How it works

- The S-09 drain endpoint is mounted (env-gated, internal-auth) as `POST /_internal/audit/drain` and is the LIVE chain-seal producer: `handle_drain` scans the `(tenant_id, region)` partitions with pending rows then seals each under a GLOBAL per-call row budget (`AUDIT_DRAIN_BATCH_LIMIT`, default 200, clamped ≥1; all five production environment blocks explicitly set it to 512 in `wrangler.toml`) so a cold backlog can never make one call exceed the edge subrequest timeout; a budget-bounded sweep returns `incomplete: true` so a caller (the hourly cron or a manual loop) re-drains until it is false `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_03.rs:1-126`.
- Bounding is per-PREFIX, not per-transaction: `read_pending_rows` seals only the ordered `ORDER BY enqueued_at, id LIMIT ?N` prefix of a partition's unsealed tail, and correctness is preserved because `resolve_resume` already prefers the durable sealed-tail over the checkpoint when ahead — a prefix-then-resume seal is byte-identical to a one-shot seal (`seal_rows` is a pure function of `(start_head, start_seq, rows)`) `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs:257-266`.
- The per-partition seal is crash-safe and fork-free: `drain_partition_inner` resolves the resume head (the durable sealed-rows tail is authoritative over the checkpoint when ahead), seals every pending row IN ORDER in bounded JSON1 write chunks (32 rows max, each guarded by `emitted_at IS NULL`), then advances the `audit_chain_head` checkpoint with a compare-and-set (single-writer anti-fork → aborts the partition on drift rather than forking) `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs:165-337`. B-038 wraps it in `drain_partition`, which serialises the partition with a per-partition lease + a seal-loop fence when `AUDIT_DRAIN_LEASE_ENABLED` is ON (default OFF/inert) `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs:72-156`.
- The link itself is `chain_hash = BLAKE3(prev_hash || RFC-8785-JCS(payload))` over the EXACT bytes persisted as `canonical_jcs` (what the verifier re-hashes); `seal_rows` is fully deterministic, which is the basis of concurrent-drain fork-freedom `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01_part2.rs:244-288`.
- The drain auth gate is the constant-time internal-auth check (`internal_auth_ok` `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs:511-518`) plus the ≥32-char key floor + fail-CLOSED env mount (unmounted without the DEDICATED `CORELINK_ERASE_AUTH_KEY` — NO shared-key fallback, finding H4 — + D1) in `build_state_from_env` `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs:522-610`.
- CF-6 keyed head: every advance Ed25519-SIGNS the canonical head tuple (JCS of `{head_hash, next_sequence, region, tenant_id}` via `canonical_head_bytes` `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs:317-344`, signed by `sign_head` `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs:366-382`) and persists it alongside the checkpoint `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part3.rs:55-160`; on resume `check_head_on_resume` re-verifies the stored signature and returns `FailClosed` on tamper / signed-but-no-seed `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs:1-67`, which the partition drain converts into a SEV-1 `tamper_detected` abort `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part3.rs:55-160`.
- The drain is invoked by an hourly Cloudflare Cron Trigger in the signup-worker. `runAuditDrainSweep` accepts ONLY the dedicated ≥32-char `CORELINK_ERASE_AUTH_KEY` — shared-only/blank/short configuration skips fail-CLOSED before a request — then POSTs `/_internal/audit/drain` `apps/signup-worker/src/webhooks/audit_drain_cron.ts:52-59`, `:188-203`; the deploy lane verifies that the secret NAME is bound before it deploys `scripts/verify-signup-worker-secrets.sh:51-59`, `.github/workflows/signup-worker-deploy.yml:126-130`. A 2xx is not automatically success: the caller decodes the complete typed handler contract (`ok`, six safe non-negative counters, `incomplete`), stops visibly on malformed data, `ok:false`, failed partitions, or incomplete-with-no-progress, and never blind-retries those states `apps/signup-worker/src/webhooks/audit_drain_cron.ts:192-204`, `:252-259`. Only a valid incomplete response with durable progress (`rows_sealed` or `heads_resigned`) re-calls, bounded by `MAX_DRAIN_CALLS` and `DRAIN_WALL_BUDGET_MS`; lease skips and head drift are treated as backpressure so they do not create a D1 retry storm, and a bounded early stop remains visibly incomplete `apps/signup-worker/src/webhooks/audit_drain_cron.ts:55-68`, `:305-307`.
- The export route's public surface (router + state) lives in the `state` submodule (`crates/corelink-container/src/routes/audit_export/state.rs:88`, `crates/corelink-container/src/routes/audit_export/state.rs:157`), re-exported verbatim through the barrel `crates/corelink-container/src/routes/audit_export.rs:144-148`.
- `build_state` wires the LIVE native exporter + export-audit sink as IN-MEMORY implementations (`InMemoryAuditExporter` + `InMemoryExportAuditSink`) — non-durable, lost on process restart and not shared across containers; the durable R2-backed exporter + `RateLimiter` DO singleton + CloudEvents audit sink are DEFERRED behind the stable `Arc<dyn ...>` trait-object surface (`crates/corelink-container/src/routes/audit_export/state.rs:88`).
- The export audit emit is routed through `emit_or_503`, aborting with 503 on sink error `crates/corelink-container/src/routes/audit_export/audit_sink.rs:100-105`.
- Mid-stream chain-break detection emits the abort trailer via `mid_stream_abort_trailer_value` `crates/corelink-container/src/routes/audit_export/stream.rs:412-417`.
- The trailer + chain-head anchor header names are canonical constants `crates/corelink-container/src/routes/audit_export/types.rs:42-57`.
- The CLI offline mode re-verifies a downloaded NDJSON file against a chain-head anchor `docs/cli/audit-export.md:10-31`.
- The CLI HTTP-aware mode streams directly off the wire and watches for the abort trailer `docs/cli/audit-export.md:33-55`.
- Analytics routes (event-count, timeline) ship their router (`crates/corelink-container/src/routes/audit_analytics/state.rs:80`) + per-tenant rate-limit config (`crates/corelink-container/src/routes/audit_analytics/state.rs:63`) enforced by `rate_limit_check` (`crates/corelink-container/src/routes/audit_analytics/rate_limit.rs:99`).
- Every audit surface runs a fail-CLOSED **audit-read scope gate** (WP-B) BEFORE any data access and BEFORE the PAT-possession gate: the Worker-trusted `x-corelink-scope` must carry a READ-capable token (`read-only`/`read-write`/`cas:*`/`admin`/`owner`) or the request is rejected `403 "insufficient scope"`; a credential with NO read capability (empty/missing scope, or a `find-missing`-only / `billing`-only token) cannot read audit / security-PII data. Gated on READ (not admin) so the self-serve customer CLI export keeps working — self-serve PATs can never be `admin` — while cross-tenant isolation stays the `x-corelink-tenant-id` binding's job `crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs:62`, `crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs:60`, `crates/corelink-container/src/routes/audit_export/handler.rs:111`.
- Each analytics handler runs the native PAT possession gate before any data access `crates/corelink-container/src/routes/audit_analytics.rs:124-129`.
- The analytics audit emit is itself a fail-CLOSED helper: `emit_or_503` pushes the row through the sink and, on `Err`, drops the prepared success response and returns `503 "audit pipeline closed"` instead `crates/corelink-container/src/routes/audit_analytics/audit_sink.rs:76-85`.
- The `/event-count` handler binds the tenant SOLELY to the `AuthTenant`-extracted `x-corelink-tenant-id` header (400 on a non-UUID), runs the audit-read scope gate then the PAT gate, rejects inverted windows, rate-limits, then aggregates per-tenant bucket counts emitting an `analytics_query` audit row on every arm `crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs:42-86`.
- The `/timeline` handler additionally caps query cost with a bucket-cardinality guard — `(to - from) / granularity` must not exceed `MAX_TIMELINE_BUCKETS`, with `granularity` itself bounded to `(0, MAX_GRANULARITY_MS]` — before any aggregate runs `crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs:82-128`.
- `resolve_shadow_via_prelude` prefers the wave-26 `RequestPrelude` region (hot path skips the per-request region round-trip) and, when the prelude is absent or bound to a different tenant, emits a `request_prelude_missing` marker row + WARN and falls back to `shadow_factory.for_tenant` rather than failing the route `crates/corelink-container/src/routes/audit_analytics/shadow_factory.rs:121-156`.
- **The analytics data source is the LIVE D1 `customer_audit_events` table (`#71`), NOT the Neon shadow.** The former per-region Neon Postgres "analytics shadow" was designed but NEVER wired in prod (the DSN env was never set and the `neon-real` driver was never compiled into the shipped container, so every prod query fell back to the in-memory factory and returned an EMPTY aggregate); it is retired and the aggregates now read the SAME durable D1 table `/v1/customer/audit` reads (migration 0077). `D1ShadowSinkFactory::from_env` builds the factory over a shared `D1HttpCustomerDb` row source `crates/corelink-container/src/routes/audit_analytics/d1_sink.rs:100-103` and `for_tenant` binds a read-only `D1AuditAnalyticsSink` `crates/corelink-container/src/routes/audit_analytics/d1_sink.rs:114-119` whose `aggregate_event_count` runs `SELECT event_type, COUNT(*) … WHERE tenant_id = ?1 AND ts_ms in [from,to)` with the optional `event_type` filter BOUND (`?4`, never interpolated) `crates/corelink-container/src/routes/audit_analytics/d1_sink.rs:206-231`, and `aggregate_timeline` buckets by `((ts_ms - CAST(?2 AS INTEGER)) / CAST(?4 AS INTEGER))` (the CAST forces integer floor-division — D1's REST API binds JSON numbers as SQLite REAL) under the same tenant scope `crates/corelink-container/src/routes/audit_analytics/d1_sink.rs:243-284`. The read-only sink rejects `NeonShadowSink::sync_chunk` — the archive WRITE half is the D1 `audit_outbox` sink (`#74`) `crates/corelink-container/src/routes/audit_analytics/d1_sink.rs:190-201`. Because the swap is behind the existing `ShadowSinkFactory` seam, the route wiring, rate-limit, native-PAT gate and audit-emit ordering above are UNCHANGED — only the data source moved.
- The export handler treats the client-controllable `:tenant` path segment as an untrusted echo: it is parsed and constant-time-compared against the `AuthTenant` header tenant, and a mismatch emits a SEV-1 cross-tenant audit row and returns 403 (fail-CLOSED: 503 on sink failure) BEFORE any data access `crates/corelink-container/src/routes/audit_export/handler.rs:83-99`.
- The export window timestamps are parsed by `parse_timestamp`, which accepts a raw Unix-epoch-ms integer or a deliberately minimal RFC 3339 `YYYY-MM-DDTHH:MM:SSZ` subset (UTC-only, no fractional seconds, no offsets) to keep the security-sensitive window grammar small `crates/corelink-container/src/routes/audit_export/parse.rs:25-32`.
- The product-analytics EDGE collector (`POST /v1/event` on the standalone analytics-worker) is the ingest tap that feeds the analytics D1 store; it authenticates each request two ways — a CORS `Origin` allow-list for browsers, or a constant-time-compared `X-Corelink-Ingest-Key` for trusted servers — and rejects anything matching neither with 403 before any write (`apps/analytics-worker/src/ingest.ts:143-168`, `apps/analytics-worker/src/ingest.ts:159-167`), the compare being length-checked + XOR-folded so a wrong key cannot be timing-probed (`apps/analytics-worker/src/ingest.ts:95-102`).
- Because the browser `Origin` header is attacker-controllable outside a real browser, only the keyed path is TRUSTED: revenue/provisioning-truth events (`SERVER_ONLY_EVENT_NAMES` — `paid_subscription_started`, `plan_downgraded`, `tenant_created`, …) are accepted ONLY with the ingest key and rejected as `server_only_event` on the anonymous Origin path (`apps/analytics-worker/src/ingest.ts:47-61`, `apps/analytics-worker/src/ingest.ts:118-122`), and that anonymous path is capped at 1 event/request (batching stays a keyed-server affordance) so a spoofed `Origin` cannot amplify D1 writes (`apps/analytics-worker/src/ingest.ts:188-193`).
- The ingest validator enforces a HARD privacy gate at the edge so no PII reaches the analytics store: a forbidden `email`/`ip`/`ip_address`/`remote_addr` key anywhere in an event's `properties` is rejected and the `event_name` must be in a closed allow-list (`apps/analytics-worker/src/ingest.ts:109-141`, `apps/analytics-worker/src/ingest.ts:132-136`).

# Invariants

- The chain seal is idempotent + fork-free: each bounded JSON1 UPDATE is guarded by `emitted_at IS NULL` (`write_seals_batch` `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs:72-156`) and the head advance is a compare-and-set on the resumed value (`advance_head_cas` `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs:165-337`), so a re-run or a concurrent drain never double-seals and never forks the chain.
- The persisted `canonical_jcs` is byte-for-byte what was hashed (`chain_hash = BLAKE3(prev || canonical_jcs)`), so the verifier path re-hashes the stored bytes rather than re-canonicalizing `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01_part2.rs:244-288`.
- The chain HEAD is tamper-evident, not just append-linked (CF-6): a resumed checkpoint whose Ed25519 signature does not verify under the current key id — or a signed head with no seed to verify it — is rejected as TAMPER and the drain refuses to extend it (fail-CLOSED, SEV-1), so a D1 writer cannot silently rewrite the head `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs:1-67`.
- The export audit row is emitted fail-CLOSED — a sink `Err` aborts with 503 before streaming `crates/corelink-container/src/routes/audit_export/audit_sink.rs:100-105`.
- Analytics data access is gated per-tenant: the PAT gate rejects forged/wrong-tenant (401) or verifier fault (503) before reads `crates/corelink-container/src/routes/audit_analytics.rs:124-129`.
- The edge ingest tap is authenticated, never anonymous: an event with neither an allow-listed `Origin` nor a correct constant-time-matched `X-Corelink-Ingest-Key` is rejected 403 before any D1 write `apps/analytics-worker/src/ingest.ts:127-136`.
- PII can never land in the analytics store: the edge validator hard-rejects any event whose `properties` carries an `email`/`ip`/`ip_address`/`remote_addr` field — the privacy rule is enforced at the gate, not left to each caller `apps/analytics-worker/src/ingest.ts:104-107`.
- A mid-stream abort surfaces as sysexits DATAERR (65) on the CLI, distinct from generic exit 1 `docs/cli/audit-export.md:84-90`.
- The bearer token is never logged, printed, or surfaced in error messages (CTRL-CRED-001) `docs/cli/audit-export.md:98-101`.

# Gotchas

- The HTTP-fetch path caps the response body at 64 MiB so a malicious server cannot drain CLI memory `docs/cli/audit-export.md:105-107`.
- If both the `--chain-head-anchor` flag and the response header are present they MUST match constant-time — a mismatch is an error, not a warning `docs/cli/audit-export.md:60`, `docs/cli/audit-export.md:102-104`.

# Citations

1. `crates/corelink-container/src/routes/audit_export/audit_sink.rs:100-105` — `emit_or_503` fail-closed export audit (the enforcing impl).
2. `crates/corelink-container/src/routes/audit_export/state.rs:88`, `crates/corelink-container/src/routes/audit_export/state.rs:157` — export `build_state` (in-memory exporter/sink; durable R2/CloudEvents sink deferred) + `router` impl; `crates/corelink-container/src/routes/audit_export.rs:144-148` — barrel re-export of that public surface.
3. `crates/corelink-container/src/routes/audit_export/stream.rs:412-417` — `mid_stream_abort_trailer_value` builder.
4. `crates/corelink-container/src/routes/audit_export/types.rs:42-57` — chain-head-anchor + abort header constants.
5. `crates/corelink-container/src/routes/audit_analytics/state.rs:80` — analytics `router`; `crates/corelink-container/src/routes/audit_analytics/state.rs:63` + `crates/corelink-container/src/routes/audit_analytics/rate_limit.rs:99` — per-tenant rate-limit config + `rate_limit_check`.
6. `crates/corelink-container/src/routes/audit_analytics.rs:124-129` — per-handler native PAT possession gate (in-barrel impl).
7. `docs/cli/audit-export.md:10-31` — CLI offline (file) re-verify mode.
8. `docs/cli/audit-export.md:33-55` — CLI HTTP-aware re-verify + anchor cross-check.
9. `docs/cli/audit-export.md:84-90` — exit-65 sysexits DATAERR on mid-stream abort.
10. `docs/cli/audit-export.md:98-104` — token never logged (CTRL-CRED-001) + constant-time anchor compare.
11. `docs/cli/audit-export.md:105-107` — 64 MiB response-body cap.
12. `apps/analytics-worker/src/ingest.ts:114-136` — the edge `POST /v1/event` collector entry (CORS-origin OR ingest-key auth, 403 otherwise); the dual-auth 403 gate at `apps/analytics-worker/src/ingest.ts:127-136`; the constant-time key compare `constantTimeEqual` at `apps/analytics-worker/src/ingest.ts:71-78`.
13. `apps/analytics-worker/src/ingest.ts:85-112` — the `validate()` event gate (closed `event_name` allow-list + size bounds); the hard PII privacy gate forbidding `email`/`ip`/`ip_address`/`remote_addr` in `properties` at `apps/analytics-worker/src/ingest.ts:104-107`.
14. `crates/corelink-container/src/routes/audit_analytics/audit_sink.rs:76-85` — analytics `emit_or_503` fail-CLOSED helper (drops success response, returns 503 on sink `Err`).
15. `crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs:42-86` — `/event-count` handler: `AuthTenant`-bound tenant (400 on non-UUID) + audit-read scope gate + native PAT gate before data access.
15b. `crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs:62`, `crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs:60`, `crates/corelink-container/src/routes/audit_export/handler.rs:111` — the WP-B fail-CLOSED audit-read scope gate (`403 "insufficient scope"` unless the trusted `x-corelink-scope` carries a read-capable token).
16. `crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs:82-128` — `/timeline` bucket-cardinality + granularity cost guards (`MAX_TIMELINE_BUCKETS` / `MAX_GRANULARITY_MS`).
17. `crates/corelink-container/src/routes/audit_analytics/shadow_factory.rs:121-156` — `resolve_shadow_via_prelude`: prelude-preferred region with `request_prelude_missing` marker + factory fallback.
17b. `crates/corelink-container/src/routes/audit_analytics/d1_sink.rs:100-103` — `D1ShadowSinkFactory::from_env`: the `#71` D1-backed analytics factory over `customer_audit_events` (migration 0077), replacing the never-wired Neon shadow; `for_tenant` → read-only `D1AuditAnalyticsSink` (`:113-119`); tenant-scoped `WHERE tenant_id = ?1` + BOUND `event_type` filter in `aggregate_event_count` (`:206-232`); `((ts_ms - CAST(?2 AS INTEGER))/CAST(?4 AS INTEGER))` integer-floored timeline buckets (`:246-284`); `sync_chunk` rejected on the read-only sink (`:183-196`).
18. `crates/corelink-container/src/routes/audit_export/handler.rs:83-99` — export `:tenant` path-segment constant-time cross-tenant check → SEV-1 row + 403 (fail-CLOSED).
19. `crates/corelink-container/src/routes/audit_export/parse.rs:25-32` — `parse_timestamp`: epoch-ms-or-minimal-RFC3339 window parser.
20. `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_03.rs:1-126` — `handle_drain`: LIVE S-09 `POST /_internal/audit/drain` entry (partition scan → seal → summary).
21. `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs:165-337` — `drain_partition_inner`: crash-safe resume (sealed-tail authoritative) + idempotent `emitted_at IS NULL`-guarded seal + compare-and-set head advance (anti-fork) + CF-6 resume-signature check (SEV-1 fail-CLOSED on tamper at `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part3.rs:55-160`); wrapped by `drain_partition` `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs:72-156` (B-038 flag-gated per-partition lease + seal-loop fence, default OFF).
22. `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01_part2.rs:244-288` — `seal_rows`: deterministic `chain_hash = BLAKE3(prev || RFC-8785-JCS(payload))` over the exact persisted `canonical_jcs` bytes.
23. `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs:511-518` — constant-time internal-auth gate (`internal_auth_ok`); `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs:522-610` — fail-CLOSED env mount (≥32-char DEDICATED `CORELINK_ERASE_AUTH_KEY`, NO shared-key fallback per finding H4, + D1 required) in `build_state_from_env`.
24. `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs:317-344` (`canonical_head_bytes`) + `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs:366-382` (`sign_head`) + `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs:418-447` (`verify_head`) + `crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs:1-67` (`check_head_on_resume`) — CF-6 keyed (Ed25519-signed) `audit_chain_head` sign-on-advance + verify-on-resume (migration 0080 columns `head_signature`/`head_signed_at_ms`/`signing_key_id`).
25. `apps/signup-worker/src/webhooks/audit_drain_cron.ts:264-314`, `:117-126`, `:173-175` — `runAuditDrainSweep`: dedicated-key-only, strict typed 2xx contract, terminal non-complete arms, and bounded retry only after real progress; `scripts/verify-signup-worker-secrets.sh:51-59` + `.github/workflows/signup-worker-deploy.yml:126-130` — secret-name propagation gate before deploy.
25. `scripts/verify-signup-worker-secrets.sh:1` — declared source anchor.
